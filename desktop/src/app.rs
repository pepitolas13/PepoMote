use crate::pairing::PairingInfo;
use crate::state::{LinkStatus, Mode, PlayerInfo, SharedState};
use crate::theme;
use egui::{Color32, Pos2, Rect, RichText, Rounding, Stroke, Vec2};
use std::time::{Duration, Instant};
use crate::i18n;
use crate::state::CfgStatus;
use crate::tr;

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
    /// RTT de los últimos latidos del Jugador 1 (sparkline).
    rtt_hist: Vec<f32>,
    dsu_clients: usize,
    dolphin_status: Option<CfgStatus>,
    cemu_status: Option<CfgStatus>,
    cemu_screen: Option<CfgStatus>,
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
                rtt_hist: s.rtt_hist.iter().copied().collect(),
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
        // Con móviles, 20 fps (el latido respira); en espera, 10 bastan
        ctx.request_repaint_after(Duration::from_millis(if snap.player_count > 0 { 50 } else { 100 }));

        // Idioma: ES / EN arriba a la derecha (se guarda en Ajustes)
        egui::Area::new(egui::Id::new("lang"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 12.0))
            .show(ctx, |ui| {
                let lang = i18n::current();
                let button = egui::Button::new(RichText::new(lang.code().to_uppercase()).size(11.0).color(theme::text_dim()))
                    .fill(theme::card())
                    .stroke(Stroke::new(1.0_f32, theme::card_border()));
                if ui.add(button).on_hover_text(tr!("win.lang_switch", lang.other().name())).clicked() {
                    let next = i18n::toggle();
                    let mut s = self.shared.lock().unwrap();
                    s.config.lang = Some(next.code().to_owned());
                    s.config.save();
                }
            });

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
                                RichText::new(tr!("win.tagline"))
                                    .size(14.0)
                                    .color(theme::text_dim()),
                            );
                            ui.add_space(18.0);

                            if snap.player_count == 0 {
                                self.ui_qr(ui, 280.0, tr!("win.waiting"));
                            } else {
                                ui_players(ui, &snap);
                                if snap.mode == Mode::Dolphin {
                                    self.ui_dolphin(ui, &snap);
                                } else if snap.mode == Mode::Cemu {
                                    self.ui_cemu(ui, &snap);
                                }
                                if snap.player_count < crate::net::MAX_PLAYERS {
                                    ui.add_space(12.0);
                                    self.ui_qr(ui, 170.0, tr!("win.another_player"));
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
                                RichText::new(tr!(
                                    "win.footer",
                                    env!("CARGO_PKG_VERSION"),
                                    snap.injector.unwrap_or(tr!("win.inj_none"))
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
                RichText::new(tr!("fix.uinput_missing"))
                .size(12.0)
                .color(theme::warn()),
            );
        } else if snap.uinput_denied {
            ui.label(
                RichText::new(tr!("fix.uinput_denied"))
                    .size(12.0)
                    .color(theme::warn()),
            );
        }
        #[cfg(target_os = "linux")]
        {
            ui.add_space(4.0);
            if snap.fixing {
                ui.label(
                    RichText::new(tr!("fix.applying"))
                        .size(12.0)
                        .color(theme::text_dim()),
                );
            } else if self.pkexec_ok {
                let label = if snap.fix_failed.is_some() { tr!("fix.retry") } else { tr!("fix.now") };
                if ui.button(RichText::new(label).size(14.0)).clicked() {
                    crate::fixes::fix_all(self.shared.clone(), self.pairing.port);
                }
                match &snap.fix_failed {
                    Some(why) => {
                        ui.label(RichText::new(why).size(11.0).color(theme::warn()));
                    }
                    None => {
                        ui.label(
                            RichText::new(tr!("fix.dialog_once"))
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
                    tr!("fix.manual_no_dialog")
                } else {
                    tr!("fix.manual_no_pkexec")
                };
                ui.label(RichText::new(intro).size(11.0).color(theme::text_dim()));
                ui.label(
                    RichText::new(crate::fixes::UINPUT_MANUAL_CMD)
                        .monospace()
                        .size(11.0)
                        .color(theme::text()),
                );
                ui.horizontal(|ui| {
                    if ui.button(RichText::new(tr!("fix.copy_cmd")).size(12.0)).clicked() {
                        ui.output_mut(|o| o.copied_text = crate::fixes::UINPUT_MANUAL_CMD.to_owned());
                        self.copied_at = Some(Instant::now());
                    }
                    if self.copied_at.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
                        ui.label(RichText::new(tr!("fix.copied")).size(11.0).color(theme::ok()));
                    }
                });
                ui.label(
                    RichText::new(tr!("fix.relogin"))
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
            ui.label(RichText::new(tr!("win.no_camera_code")).size(12.0).color(theme::text_dim()));
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
            RichText::new(tr!("win.dolphin_clients", snap.dsu_clients))
                .size(13.0)
                .color(theme::blue()),
        );
        if ui
            .button(RichText::new(tr!("win.configure_dolphin")).size(13.0))
            .clicked()
        {
            crate::dolphin::configure_now(&self.shared);
        }
        if let Some(st) = &snap.dolphin_status {
            let color = if st.ok { theme::ok() } else { theme::warn() };
            ui.label(RichText::new(&st.text).size(12.0).color(color));
        }
        ui.label(
            RichText::new(tr!("win.dolphin_help"))
            .size(11.0)
            .color(theme::text_dim()),
        );
    }

    fn ui_cemu(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.add_space(8.0);
        ui.label(
            RichText::new(tr!("win.cemu_clients", snap.dsu_clients))
                .size(13.0)
                .color(theme::blue()),
        );
        if ui
            .button(RichText::new(tr!("win.configure_cemu")).size(13.0))
            .clicked()
        {
            crate::cemu::configure_now(&self.shared);
        }
        if let Some(st) = &snap.cemu_status {
            let color = if st.ok { theme::ok() } else { theme::warn() };
            ui.label(RichText::new(&st.text).size(12.0).color(color));
        }
        if let Some(st) = &snap.cemu_screen {
            let color = if st.ok { theme::ok() } else { theme::text_dim() };
            ui.label(RichText::new(&st.text).size(12.0).color(color));
        }
        ui.label(
            RichText::new(tr!("win.cemu_help"))
                .size(11.0)
                .color(theme::text_dim()),
        );
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let mut config = self.shared.lock().unwrap().config.clone();
        let before = config.clone();

        egui::CollapsingHeader::new(
            RichText::new(tr!("cfg.title")).size(14.0).color(theme::text_dim()),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("cfg.sensitivity")).size(13.0).color(theme::text_dim()));
                ui.add(
                    egui::Slider::new(&mut config.sens_deg, 15.0..=60.0)
                        .suffix("°")
                        .fixed_decimals(0),
                );
            });
            ui.label(
                RichText::new(tr!("cfg.sens_help"))
                    .size(11.0)
                    .color(theme::text_dim()),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut config.abs_mode, RichText::new(tr!("cfg.abs")).size(13.0));
                info_icon(ui, tr!("cfg.abs_help"));
            });
            ui.add_space(4.0);
            ui.checkbox(
                &mut config.auto_dolphin,
                RichText::new(tr!("cfg.auto_dolphin")).size(13.0),
            );
            ui.add_space(4.0);
            ui.checkbox(
                &mut config.auto_cemu,
                RichText::new(tr!("cfg.auto_cemu")).size(13.0),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut config.auto_mode,
                    RichText::new(tr!("cfg.auto_mode")).size(13.0),
                );
                info_icon(ui, tr!("cfg.auto_mode_help"));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("cfg.cemu_folder")).size(13.0).color(theme::text_dim()));
                ui.add(
                    egui::TextEdit::singleline(&mut config.cemu_dir)
                        .desired_width(200.0)
                        .hint_text(tr!("cfg.auto_hint")),
                );
                if ui.button(RichText::new(tr!("cfg.detect")).size(12.0)).clicked() {
                    crate::cemu::detect_now(&self.shared);
                }
            });
            ui.label(
                RichText::new(tr!("cfg.cemu_folder_help"))
                    .size(11.0)
                    .color(theme::text_dim()),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("cfg.dolphin_folder")).size(13.0).color(theme::text_dim()));
                ui.add(
                    egui::TextEdit::singleline(&mut config.dolphin_dir)
                        .desired_width(200.0)
                        .hint_text(tr!("cfg.auto_hint")),
                );
                if ui.button(RichText::new(tr!("cfg.detect")).size(12.0)).clicked() {
                    crate::dolphin::detect_now(&self.shared);
                }
            });
            ui.label(
                RichText::new(tr!("cfg.dolphin_folder_help"))
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
                        ui.label(RichText::new(tr!("cfg.pointing_on")).size(13.0).color(theme::text_dim()));
                        let current = if config.screen.is_empty() {
                            tr!("cfg.all_screens").to_owned()
                        } else {
                            config.screen.clone()
                        };
                        egui::ComboBox::from_id_salt("pantalla_apuntado")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut config.screen, String::new(), tr!("cfg.all_screens"));
                                for (name, w, h) in &screens {
                                    ui.selectable_value(&mut config.screen, name.clone(), tr!("cfg.only_screen", name, w, h));
                                }
                            });
                    });
                    ui.label(
                        RichText::new(tr!("cfg.screens_help"))
                            .size(11.0)
                            .color(theme::text_dim()),
                    );
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("cfg.theme")).size(13.0).color(theme::text_dim()));
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
                RichText::new(tr!("cfg.autostart")).size(13.0),
            );
            if self.autostart != before_auto {
                if let Err(e) = crate::autostart::set_enabled(self.autostart) {
                    self.shared.lock().unwrap().last_error = Some(tr!("cfg.autostart_err", e));
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
        theme::ThemePref::System => tr!("cfg.theme_system"),
        theme::ThemePref::Light => tr!("cfg.theme_light"),
        theme::ThemePref::Dark => tr!("cfg.theme_dark"),
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

/// Alfa del punto de latido: respira a 1 Hz entre 89 y 255.
fn breath_alpha(t: f64) -> u8 {
    let s = 0.5 - 0.5 * (t * std::f64::consts::TAU).cos();
    (89.0 + s * 166.0).round() as u8
}

/// Los últimos `cap` valores normalizados a 0..1 contra el máximo (o contra
/// `floor_ms` si es mayor: que un RTT de 5 ms no parezca una montaña).
fn sparkline_norm(values: &[f32], cap: usize, floor_ms: f32) -> Vec<f32> {
    let tail = &values[values.len().saturating_sub(cap)..];
    let max = tail.iter().copied().fold(floor_ms, f32::max);
    tail.iter().map(|v| (v / max).clamp(0.0, 1.0)).collect()
}

/// Sparkline (80×16) del RTT de los últimos latidos del Jugador 1.
fn sparkline(ui: &mut egui::Ui, values: &[f32]) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(80.0, 16.0), egui::Sense::hover());
    let norm = sparkline_norm(values, crate::state::RTT_HIST, 20.0);
    if norm.len() < 2 {
        return;
    }
    let n = norm.len() as f32;
    let pts: Vec<Pos2> = norm
        .iter()
        .enumerate()
        .map(|(i, v)| {
            Pos2::new(
                rect.left() + rect.width() * i as f32 / (n - 1.0),
                rect.bottom() - 1.0 - v * (rect.height() - 2.0),
            )
        })
        .collect();
    ui.painter().add(egui::Shape::line(pts, Stroke::new(1.2_f32, theme::blue())));
    let last = values.last().copied().unwrap_or(0.0);
    resp.on_hover_text(tr!("win.heartbeat_tip", last.round() as i64, values.len().min(crate::state::RTT_HIST)));
}

fn ui_players(ui: &mut egui::Ui, snap: &Snapshot) {
    let card_w = 340.0f32.min(ui.available_width());
    let row_h = 34.0;
    let head_h = 62.0;
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
        Mode::Pointer => tr!("win.mode_pointer"),
        Mode::Dolphin => tr!("win.mode_dolphin"),
        Mode::Cemu => tr!("win.mode_cemu"),
    };
    let cemu = snap.mode == Mode::Cemu;
    let cemu_layout = crate::state::cemu_layout(&snap.players);
    child.label(RichText::new(mode_txt).size(13.0).color(theme::blue()));
    child.horizontal(|ui| {
        // Latido: un punto verde que respira mientras llegan paquetes
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
        let color = if snap.pps > 0.0 {
            let c = theme::ok();
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), breath_alpha(ui.input(|i| i.time)))
        } else {
            theme::text_dim()
        };
        ui.painter().circle_filled(dot.center(), 4.0, color);
        ui.label(
            RichText::new(tr!("win.pps", snap.pps.round() as i64, snap.sensor_hz.round() as i64))
                .size(11.0)
                .color(theme::text_dim()),
        );
        if snap.rtt_hist.len() >= 2 {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| sparkline(ui, &snap.rtt_hist));
        }
    });
    child.add_space(4.0);

    for (i, slot) in snap.players.iter().enumerate() {
        let Some(p) = slot else { continue };
        let number = crate::state::player_number(&snap.players, i as u8);
        let badge = if p.role == crate::state::Role::Nunchuk {
            if cemu && !cemu_layout.iter().any(|c| c.nunchuk_slot == Some(i as u8)) {
                tr!("win.badge_nunchuk_unused", number)
            } else {
                tr!("win.badge_nunchuk", number)
            }
        } else if cemu {
            match cemu_layout.iter().find(|c| c.dsu_slot == i as u8).map(|c| c.kind) {
                Some(crate::state::PadKind::GamePad) => tr!("win.badge_gamepad", number),
                Some(crate::state::PadKind::Pro) => tr!("win.badge_pro", number),
                Some(crate::state::PadKind::Wiimote) => tr!("win.badge_wiimote", number),
                None => tr!("win.badge_player", number),
            }
        } else {
            tr!("win.badge_player", number)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_latido_respira_entre_89_y_255_a_un_hercio() {
        assert_eq!(breath_alpha(0.0), 89);
        assert_eq!(breath_alpha(0.5), 255);
        assert_eq!(breath_alpha(1.0), 89);
        for i in 0..=100 {
            let a = breath_alpha(i as f64 / 100.0);
            assert!((89..=255).contains(&a), "t={i}: {a}");
        }
    }

    #[test]
    fn la_sparkline_normaliza_con_suelo_y_se_queda_con_los_ultimos() {
        assert!(sparkline_norm(&[], 60, 20.0).is_empty());
        assert_eq!(sparkline_norm(&[5.0, 10.0], 60, 20.0), vec![0.25, 0.5]);
        let n = sparkline_norm(&[10.0, 40.0], 60, 20.0);
        assert!((n[0] - 0.25).abs() < 1e-6 && (n[1] - 1.0).abs() < 1e-6, "por encima del suelo manda el máximo");
        let many: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let n = sparkline_norm(&many, 60, 20.0);
        assert_eq!(n.len(), 60);
        assert!((n[59] - 1.0).abs() < 1e-6);
        assert!((n[0] - 40.0 / 99.0).abs() < 1e-6, "empieza en el valor 40");
    }
}
