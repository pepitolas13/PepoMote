use crate::pairing::PairingInfo;
use crate::state::{LinkStatus, Mode, PlayerInfo, SharedState};
use crate::theme;
use egui::{Pos2, Rect, RichText, Rounding, Stroke, Vec2};
use std::time::{Duration, Instant};

pub struct PepoMoteApp {
    shared: SharedState,
    pairing: PairingInfo,
    qr_modules: Vec<bool>,
    qr_width: usize,
    autostart: bool,
    /// Última comprobación de la IP local (el QR debe llevar la IP viva).
    ip_checked: Instant,
    /// Ajustes cambiados en la UI pendientes de escribir a disco.
    config_dirty: bool,
    /// Linux: hay pkexec para el botón "Reparar ahora" (se mira una vez).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pkexec_ok: bool,
    /// Linux: cuándo se copió el comando manual (para el «Copiado» efímero).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    copied_at: Option<Instant>,
}

impl PepoMoteApp {
    pub fn new(cc: &eframe::CreationContext<'_>, shared: SharedState, pairing: PairingInfo) -> Self {
        theme::apply(&cc.egui_ctx);
        let pref = shared.lock().unwrap().config.theme;
        theme::set_preference(&cc.egui_ctx, pref);
        let (qr_modules, qr_width) = build_qr(&pairing.pair_url());
        Self {
            shared,
            pairing,
            qr_modules,
            qr_width,
            autostart: crate::autostart::is_enabled(),
            ip_checked: Instant::now(),
            config_dirty: false,
            #[cfg(target_os = "linux")]
            pkexec_ok: crate::fixes::pkexec_available(),
            #[cfg(not(target_os = "linux"))]
            pkexec_ok: false,
            copied_at: None,
        }
    }

    /// Con autoarranque el receptor suele nacer antes que la red (QR con
    /// 127.0.0.1), y la IP puede cambiar con la Wi-Fi o el DHCP: cada 3 s se
    /// re-consulta (un socket UDP sin tráfico) y el QR se regenera si cambió.
    fn refresh_ip(&mut self) {
        if self.ip_checked.elapsed() < Duration::from_secs(3) {
            return;
        }
        self.ip_checked = Instant::now();
        let ip = crate::pairing::local_ip();
        if ip != self.pairing.host {
            self.pairing.host = ip;
            let (m, w) = build_qr(&self.pairing.pair_url());
            self.qr_modules = m;
            self.qr_width = w;
        }
    }
}

struct Snapshot {
    status: LinkStatus,
    mode: Mode,
    players: [Option<PlayerInfo>; crate::net::MAX_PLAYERS],
    player_count: usize,
    pps: f32,
    sensor_hz: f32,
    dsu_clients: usize,
    dolphin_status: Option<String>,
    cemu_status: Option<String>,
    cemu_screen: Option<String>,
    error: Option<String>,
    port_notice: Option<String>,
    firewall_hint: Option<String>,
    /// Backend de inyección activo (pie de la ventana).
    injector: Option<&'static str>,
    uinput_denied: bool,
    uinput_missing: bool,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fixing: bool,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fix_failed: Option<String>,
}

impl eframe::App for PepoMoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::sync(ctx);
        ctx.request_repaint_after(Duration::from_millis(100));

        // En Windows, cerrar = esconder a la bandeja ("Salir" está en el tray)
        #[cfg(windows)]
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        self.refresh_ip();

        let snap = {
            let s = self.shared.lock().unwrap();
            Snapshot {
                status: s.status,
                mode: s.mode,
                players: s.players.clone(),
                player_count: s.player_count(),
                pps: s.pps,
                sensor_hz: s.sensor_hz,
                dsu_clients: s.dsu_clients,
                dolphin_status: s.dolphin_cfg_status.clone(),
                cemu_status: s.cemu_cfg_status.clone(),
                cemu_screen: s.cemu_screen_status.clone(),
                error: s.last_error.clone(),
                port_notice: s.port_notice.clone(),
                firewall_hint: s.firewall_hint.clone(),
                injector: s.injector,
                uinput_denied: s.uinput_denied,
                uinput_missing: s.uinput_missing,
                fixing: s.fixing,
                fix_failed: s.fix_failed.clone(),
            }
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme::background()).inner_margin(24.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("PepoMote")
                                    .size(34.0)
                                    .strong()
                                    .color(theme::text()),
                            );
                            ui.label(
                                RichText::new("Apunta. Haz clic. Juega.")
                                    .size(14.0)
                                    .color(theme::text_dim()),
                            );
                            ui.add_space(18.0);

                            if snap.player_count == 0 {
                                self.ui_qr(ui, 280.0, "Esperando al móvil…");
                            } else {
                                ui_players(ui, &snap);
                                if snap.mode == Mode::Dolphin {
                                    self.ui_dolphin(ui, &snap);
                                } else if snap.mode == Mode::Cemu {
                                    self.ui_cemu(ui, &snap);
                                }
                                if snap.player_count < crate::net::MAX_PLAYERS {
                                    ui.add_space(12.0);
                                    self.ui_qr(ui, 170.0, "¿Otro jugador? Escanea");
                                }
                            }

                            ui.add_space(10.0);
                            self.ui_settings(ui);

                            self.ui_repair(ui, &snap);

                            if let Some(err) = &snap.error {
                                ui.add_space(10.0);
                                ui.label(RichText::new(err).size(12.0).color(theme::error()));
                            }
                            if let Some(n) = &snap.port_notice {
                                ui.add_space(6.0);
                                ui.label(RichText::new(n).size(12.0).color(theme::warn()));
                            }

                            ui.add_space(12.0);
                            ui.label(
                                RichText::new(format!(
                                    "v{} · pv1 · Inyección: {}",
                                    env!("CARGO_PKG_VERSION"),
                                    snap.injector.unwrap_or("ninguna")
                                ))
                                .size(11.0)
                                .color(theme::text_dim()),
                            );
                        });
                    });
            });

        let _ = snap.status;
    }
}

impl PepoMoteApp {
    /// Linux: aviso de firewall/uinput con reparación de un clic (pkexec) y,
    /// si no hay diálogo de contraseña (sin pkexec, o sesión sin agente de
    /// polkit), el comando manual listo para copiar. Con el backend Wayland
    /// nunca hay nada que pintar (uinput ni se intenta); en Windows tampoco
    /// (los flags jamás se activan).
    fn ui_repair(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        let uinput_problem = snap.uinput_denied || snap.uinput_missing;
        if snap.firewall_hint.is_none() && !uinput_problem {
            return;
        }
        ui.add_space(10.0);
        if let Some(hint) = &snap.firewall_hint {
            ui.label(RichText::new(hint).size(12.0).color(theme::warn()));
        }
        if snap.uinput_missing {
            ui.label(
                RichText::new(
                    "El módulo uinput no está cargado (no existe /dev/uinput): sin él no puedo mover el cursor.",
                )
                .size(12.0)
                .color(theme::warn()),
            );
        } else if snap.uinput_denied {
            ui.label(
                RichText::new("Sin permiso para mover el cursor (/dev/uinput).")
                    .size(12.0)
                    .color(theme::warn()),
            );
        }
        #[cfg(target_os = "linux")]
        {
            ui.add_space(4.0);
            if snap.fixing {
                ui.label(
                    RichText::new("Aplicando… responde al diálogo de contraseña")
                        .size(12.0)
                        .color(theme::text_dim()),
                );
            } else if self.pkexec_ok {
                let label = if snap.fix_failed.is_some() { "🔧 Reintentar" } else { "🔧 Reparar ahora" };
                if ui.button(RichText::new(label).size(14.0)).clicked() {
                    crate::fixes::fix_all(self.shared.clone(), self.pairing.port);
                }
                match &snap.fix_failed {
                    Some(why) => {
                        ui.label(RichText::new(why).size(11.0).color(theme::warn()));
                    }
                    None => {
                        ui.label(
                            RichText::new("Un diálogo del sistema pedirá tu contraseña una sola vez")
                                .size(11.0)
                                .color(theme::text_dim()),
                        );
                    }
                }
            }
            // Sin pkexec, o con el diálogo fallando (sesión sin agente de
            // polkit): el comando manual, listo para copiar
            if uinput_problem && (!self.pkexec_ok || snap.fix_failed.is_some()) {
                ui.add_space(6.0);
                let intro = if self.pkexec_ok {
                    "Si el diálogo no aparece (sesión sin agente de polkit), pega esto en un terminal:"
                } else {
                    "No hay pkexec en este sistema. Pega esto en un terminal (pide tu contraseña una vez):"
                };
                ui.label(RichText::new(intro).size(11.0).color(theme::text_dim()));
                ui.label(
                    RichText::new(crate::fixes::UINPUT_MANUAL_CMD)
                        .monospace()
                        .size(11.0)
                        .color(theme::text()),
                );
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Copiar comando").size(12.0)).clicked() {
                        ui.output_mut(|o| o.copied_text = crate::fixes::UINPUT_MANUAL_CMD.to_owned());
                        self.copied_at = Some(Instant::now());
                    }
                    if self.copied_at.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
                        ui.label(RichText::new("Copiado").size(11.0).color(theme::ok()));
                    }
                });
                ui.label(
                    RichText::new(
                        "Si después sigue sin permiso, cierra sesión y vuelve a entrar (la regla se aplica al iniciar sesión).",
                    )
                    .size(11.0)
                    .color(theme::text_dim()),
                );
            }
        }
    }

    fn ui_qr(&self, ui: &mut egui::Ui, size: f32, caption: &str) {
        draw_qr_card(ui, &self.qr_modules, self.qr_width, size);
        ui.add_space(8.0);
        ui.label(RichText::new(caption).size(13.0).color(theme::text_dim()));
        ui.label(
            RichText::new(format!("{} : {}", self.pairing.host, self.pairing.port))
                .size(11.0)
                .color(theme::text_dim()),
        );
        // Sin cámara (Linux móvil): código de 4 dígitos, un solo uso, 120 s
        let (code, left) = self.shared.lock().unwrap().pair_code.current();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Sin cámara: código").size(12.0).color(theme::text_dim()));
            ui.label(RichText::new(code).size(20.0).strong().color(theme::text()));
            ui.label(
                RichText::new(format!("({} s)", left.as_secs()))
                    .size(11.0)
                    .color(theme::text_dim()),
            );
        });
    }

    fn ui_dolphin(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Dolphin: {} cliente(s) DSU", snap.dsu_clients))
                .size(13.0)
                .color(theme::blue()),
        );
        if ui
            .button(RichText::new("Configurar Dolphin").size(13.0))
            .clicked()
        {
            crate::dolphin::configure_now(&self.shared);
        }
        if let Some(msg) = &snap.dolphin_status {
            let color = if msg.starts_with("Dolphin configurado") {
                theme::ok()
            } else {
                theme::warn()
            };
            ui.label(RichText::new(msg).size(12.0).color(color));
        }
        ui.label(
            RichText::new(
                "¿Dolphin enseña el mando desconectado? Modo Dolphin en el móvil, juego de Wii, y reinicia \
                 Dolphin; la carpeta configurada tiene que ser la suya (en Dolphin: Archivo, «Abrir carpeta de usuario»).",
            )
            .size(11.0)
            .color(theme::text_dim()),
        );
    }

    fn ui_cemu(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Cemu (Wii U): {} cliente(s) DSU", snap.dsu_clients))
                .size(13.0)
                .color(theme::blue()),
        );
        if ui
            .button(RichText::new("Configurar Cemu").size(13.0))
            .clicked()
        {
            crate::cemu::configure_now(&self.shared);
        }
        if let Some(msg) = &snap.cemu_status {
            let color = if msg.starts_with("Cemu configurado") || msg.starts_with("Cemu encontrado") {
                theme::ok()
            } else {
                theme::warn()
            };
            ui.label(RichText::new(msg).size(12.0).color(color));
        }
        if let Some(msg) = &snap.cemu_screen {
            let color = if msg.starts_with("Pantalla del GamePad:") && !msg.contains("sin móvil") {
                theme::ok()
            } else {
                theme::text_dim()
            };
            ui.label(RichText::new(msg).size(12.0).color(color));
        }
        ui.label(
            RichText::new("Con Cemu cerrado. Jugador 1 = GamePad (y ve la pantalla del GamePad en el móvil), los demás Pro Controller; «Mando de Wii» se elige en el móvil.")
                .size(11.0)
                .color(theme::text_dim()),
        );
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let mut config = self.shared.lock().unwrap().config.clone();
        let before = config.clone();

        egui::CollapsingHeader::new(
            RichText::new("Ajustes").size(14.0).color(theme::text_dim()),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sensibilidad").size(13.0).color(theme::text_dim()));
                ui.add(
                    egui::Slider::new(&mut config.sens_deg, 15.0..=60.0)
                        .suffix("°")
                        .fixed_decimals(0),
                );
            });
            ui.label(
                RichText::new("Grados de giro para cruzar la pantalla (menos = más rápido)")
                    .size(11.0)
                    .color(theme::text_dim()),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut config.abs_mode, RichText::new("Apuntado absoluto").size(13.0));
                info_icon(
                    ui,
                    "Activado: el cursor está donde apunta el móvil, y vuelve al mismo sitio si vuelves a apuntar \
                     igual (la altura la fija la gravedad; el giro, el último recentrado). El recorrido es exacto: \
                     Windows no le aplica su aceleración de ratón.\n\n\
                     Desactivado (relativo): como un ratón, solo cuentan los desplazamientos, con la aceleración \
                     del sistema si la tienes activada. Es el modo para juegos que capturan el ratón.\n\n\
                     En movimiento los dos son idénticos (1:1 con el giroscopio). Home recentra en los dos.",
                );
            });
            ui.add_space(4.0);
            ui.checkbox(
                &mut config.auto_dolphin,
                RichText::new("Configurar Dolphin automáticamente (multijugador)").size(13.0),
            );
            ui.add_space(4.0);
            ui.checkbox(
                &mut config.auto_cemu,
                RichText::new("Configurar Cemu automáticamente (modo Wii U)").size(13.0),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Carpeta de Cemu").size(13.0).color(theme::text_dim()));
                ui.add(
                    egui::TextEdit::singleline(&mut config.cemu_dir)
                        .desired_width(200.0)
                        .hint_text("automática"),
                );
                if ui.button(RichText::new("Detectar").size(12.0)).clicked() {
                    crate::cemu::detect_now(&self.shared);
                }
            });
            ui.label(
                RichText::new("Solo hace falta si Cemu está en un sitio raro; se aprende sola al verlo abierto.")
                    .size(11.0)
                    .color(theme::text_dim()),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Carpeta de Dolphin").size(13.0).color(theme::text_dim()));
                ui.add(
                    egui::TextEdit::singleline(&mut config.dolphin_dir)
                        .desired_width(200.0)
                        .hint_text("automática"),
                );
                if ui.button(RichText::new("Detectar").size(12.0)).clicked() {
                    crate::dolphin::detect_now(&self.shared);
                }
            });
            ui.label(
                RichText::new("La del Dolphin.exe. Un Dolphin portable (RetroBat, LaunchBox…) guarda su configuración ahí; se aprende sola al verlo abierto.")
                    .size(11.0)
                    .color(theme::text_dim()),
            );
            // Linux con varios monitores: a cuál apunta el móvil
            #[cfg(target_os = "linux")]
            {
                let screens = self.shared.lock().unwrap().screens.clone();
                if screens.len() > 1 {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Apuntado absoluto en").size(13.0).color(theme::text_dim()));
                        let current = if config.screen.is_empty() {
                            "Todas las pantallas".to_owned()
                        } else {
                            config.screen.clone()
                        };
                        egui::ComboBox::from_id_salt("pantalla_apuntado")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut config.screen, String::new(), "Todas las pantallas");
                                for (name, w, h) in &screens {
                                    ui.selectable_value(&mut config.screen, name.clone(), format!("Solo {name} ({w}×{h})"));
                                }
                            });
                    });
                    ui.label(
                        RichText::new("Todas = el cursor llega a los tres monitores. Una sola = apuntado preciso para jugar.")
                            .size(11.0)
                            .color(theme::text_dim()),
                    );
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Tema").size(13.0).color(theme::text_dim()));
                egui::ComboBox::from_id_salt("tema")
                    .selected_text(theme_label(config.theme))
                    .show_ui(ui, |ui| {
                        for p in theme::ThemePref::ALL {
                            ui.selectable_value(&mut config.theme, p, theme_label(p));
                        }
                    });
            });
            if config.theme != before.theme {
                theme::set_preference(ui.ctx(), config.theme);
            }
            ui.add_space(4.0);
            let before_auto = self.autostart;
            ui.checkbox(
                &mut self.autostart,
                RichText::new("Arrancar con el sistema").size(13.0),
            );
            if self.autostart != before_auto {
                if let Err(e) = crate::autostart::set_enabled(self.autostart) {
                    self.shared.lock().unwrap().last_error = Some(format!("Autoarranque: {e}"));
                    self.autostart = before_auto;
                }
            }
        });

        if config != before {
            // En caliente para el puntero ya; a disco cuando sueltes el
            // slider (arrastrarlo escribía el archivo en cada frame)
            self.shared.lock().unwrap().config = config.clone();
            self.config_dirty = true;
        }
        if self.config_dirty && !ui.input(|i| i.pointer.any_down()) {
            config.save();
            self.config_dirty = false;
        }
    }
}

fn theme_label(p: theme::ThemePref) -> &'static str {
    match p {
        theme::ThemePref::System => "Como el sistema",
        theme::ThemePref::Light => "Claro",
        theme::ThemePref::Dark => "Oscuro",
    }
}

/// Circulito con una «i»: al pasar el ratón por encima enseña `text`.
fn info_icon(ui: &mut egui::Ui, text: &str) {
    let size = 15.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let color = if resp.hovered() { theme::blue() } else { theme::text_dim() };
    let painter = ui.painter();
    painter.circle_stroke(rect.center(), size / 2.0 - 1.0, Stroke::new(1.3_f32, color));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "i",
        egui::FontId::proportional(11.0),
        color,
    );
    resp.on_hover_text(text);
}

fn ui_players(ui: &mut egui::Ui, snap: &Snapshot) {
    let card_w = 340.0f32.min(ui.available_width());
    let row_h = 34.0;
    let head_h = 58.0;
    let card_h = head_h + row_h * snap.player_count as f32 + 14.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(card_w, card_h), egui::Sense::hover());
    ui.painter().rect(
        rect,
        Rounding::same(theme::RADIUS),
        theme::card(),
        Stroke::new(1.5_f32, theme::card_border()),
    );

    let mut child = ui.child_ui(
        rect.shrink(16.0),
        egui::Layout::top_down(egui::Align::Min),
        None,
    );
    let mode_txt = match snap.mode {
        Mode::Pointer => "Modo puntero (apunta el Jugador 1)",
        Mode::Dolphin => "Modo Dolphin: todos juegan",
        Mode::Cemu => "Modo Wii U (Cemu): todos juegan",
    };
    let cemu = snap.mode == Mode::Cemu;
    let cemu_layout = crate::state::cemu_layout(&snap.players);
    child.label(RichText::new(mode_txt).size(13.0).color(theme::blue()));
    child.label(
        RichText::new(format!(
            "{:.0} paquetes/s · sensor {:.0} Hz",
            snap.pps, snap.sensor_hz
        ))
        .size(11.0)
        .color(theme::text_dim()),
    );
    child.add_space(6.0);

    for (i, slot) in snap.players.iter().enumerate() {
        let Some(p) = slot else { continue };
        let number = crate::state::player_number(&snap.players, i as u8);
        let badge = if p.role == crate::state::Role::Nunchuk {
            if cemu && !cemu_layout.iter().any(|c| c.nunchuk_slot == Some(i as u8)) {
                format!("J{number} · Nunchuk (sin uso: J{number} no es Mando Wii)")
            } else {
                format!("J{number} · Nunchuk")
            }
        } else if cemu {
            match cemu_layout.iter().find(|c| c.dsu_slot == i as u8).map(|c| c.kind) {
                Some(crate::state::PadKind::GamePad) => format!("J{number} · GamePad"),
                Some(crate::state::PadKind::Pro) => format!("J{number} · Pro"),
                Some(crate::state::PadKind::Wiimote) => format!("J{number} · Mando Wii"),
                None => format!("J{number}"),
            }
        } else {
            format!("J{number}")
        };
        child.horizontal(|ui| {
            ui.label(
                RichText::new(badge)
                    .size(13.0)
                    .strong()
                    .color(theme::ON_ACCENT)
                    .background_color(theme::blue()),
            );
            let name = if p.model.is_empty() {
                p.name.clone()
            } else {
                p.model.clone()
            };
            ui.label(RichText::new(name).size(13.0).color(theme::text()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let rtt = p
                    .rtt_ms
                    .map(|r| format!("{r:.0} ms"))
                    .unwrap_or_else(|| "—".into());
                ui.label(
                    RichText::new(format!("{}% · {rtt}", p.battery_pct))
                        .size(12.0)
                        .color(theme::text_dim()),
                );
            });
        });
    }
}

fn build_qr(url: &str) -> (Vec<bool>, usize) {
    match qrcode::QrCode::new(url.as_bytes()) {
        Ok(code) => {
            let width = code.width();
            let modules = code
                .to_colors()
                .into_iter()
                .map(|c| c == qrcode::Color::Dark)
                .collect();
            (modules, width)
        }
        Err(_) => (vec![], 0),
    }
}

fn draw_qr_card(ui: &mut egui::Ui, modules: &[bool], width: usize, size: f32) {
    let card_size = ui.available_width().clamp(120.0, size);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(card_size), egui::Sense::hover());
    let painter = ui.painter();

    // El QR va SIEMPRE claro (módulos oscuros sobre blanco): con el tema
    // oscuro invertido no lo lee cualquier cámara
    painter.rect(
        rect,
        Rounding::same(theme::RADIUS),
        theme::LIGHT.card,
        Stroke::new(1.5_f32, theme::LIGHT.card_border),
    );

    if width == 0 {
        return;
    }

    let quiet = 4;
    let total = width + quiet * 2;
    let module = (card_size - 20.0) / total as f32;
    let origin = Pos2::new(
        rect.min.x + 10.0 + quiet as f32 * module,
        rect.min.y + 10.0 + quiet as f32 * module,
    );

    for y in 0..width {
        for x in 0..width {
            if modules[y * width + x] {
                let min = Pos2::new(origin.x + x as f32 * module, origin.y + y as f32 * module);
                painter.rect_filled(
                    Rect::from_min_size(min, Vec2::splat(module + 0.5)),
                    0.0,
                    theme::LIGHT.text,
                );
            }
        }
    }
}
