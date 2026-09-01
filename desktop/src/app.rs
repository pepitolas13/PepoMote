use crate::pairing::PairingInfo;
use crate::state::{LinkStatus, Mode, PlayerInfo, SharedState};
use crate::theme;
use egui::{Color32, Pos2, Rect, RichText, Rounding, Stroke, Vec2};
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
}

impl PepoMoteApp {
    pub fn new(cc: &eframe::CreationContext<'_>, shared: SharedState, pairing: PairingInfo) -> Self {
        theme::apply(&cc.egui_ctx);
        let (qr_modules, qr_width) = build_qr(&pairing.pair_url());
        Self {
            shared,
            pairing,
            qr_modules,
            qr_width,
            autostart: crate::autostart::is_enabled(),
            ip_checked: Instant::now(),
            config_dirty: false,
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
    error: Option<String>,
}

impl eframe::App for PepoMoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                error: s.last_error.clone(),
            }
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme::BACKGROUND).inner_margin(24.0))
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
                                    .color(theme::TEXT),
                            );
                            ui.label(
                                RichText::new("Apunta. Haz clic. Juega.")
                                    .size(14.0)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.add_space(18.0);

                            if snap.player_count == 0 {
                                self.ui_qr(ui, 280.0, "Esperando al móvil…");
                            } else {
                                ui_players(ui, &snap);
                                if snap.mode == Mode::Dolphin {
                                    self.ui_dolphin(ui, &snap);
                                }
                                if snap.player_count < crate::net::MAX_PLAYERS {
                                    ui.add_space(12.0);
                                    self.ui_qr(ui, 170.0, "¿Otro jugador? Escanea");
                                }
                            }

                            ui.add_space(10.0);
                            self.ui_settings(ui);

                            if let Some(err) = &snap.error {
                                ui.add_space(10.0);
                                ui.label(RichText::new(err).size(12.0).color(theme::ERROR));
                            }

                            ui.add_space(12.0);
                            ui.label(
                                RichText::new(format!("v{} · pv1", env!("CARGO_PKG_VERSION")))
                                    .size(11.0)
                                    .color(theme::TEXT_DIM),
                            );
                        });
                    });
            });

        let _ = snap.status;
    }
}

impl PepoMoteApp {
    fn ui_qr(&self, ui: &mut egui::Ui, size: f32, caption: &str) {
        draw_qr_card(ui, &self.qr_modules, self.qr_width, size);
        ui.add_space(8.0);
        ui.label(RichText::new(caption).size(13.0).color(theme::TEXT_DIM));
        ui.label(
            RichText::new(format!("{} : {}", self.pairing.host, self.pairing.port))
                .size(11.0)
                .color(theme::TEXT_DIM),
        );
        // Sin cámara (Linux móvil): código de 4 dígitos, un solo uso, 120 s
        let (code, left) = self.shared.lock().unwrap().pair_code.current();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Sin cámara: código").size(12.0).color(theme::TEXT_DIM));
            ui.label(RichText::new(code).size(20.0).strong().color(theme::TEXT));
            ui.label(
                RichText::new(format!("({} s)", left.as_secs()))
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
        });
    }

    fn ui_dolphin(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Dolphin: {} cliente(s) DSU", snap.dsu_clients))
                .size(13.0)
                .color(theme::BLUE),
        );
        if ui
            .button(RichText::new("Configurar Dolphin").size(13.0))
            .clicked()
        {
            crate::dolphin::configure_now(&self.shared);
        }
        if let Some(msg) = &snap.dolphin_status {
            let color = if msg.starts_with("Dolphin configurado") {
                theme::OK
            } else {
                theme::WARN
            };
            ui.label(RichText::new(msg).size(12.0).color(color));
        }
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let mut config = self.shared.lock().unwrap().config;
        let before = (config.sens_deg, config.abs_mode, config.auto_dolphin);

        egui::CollapsingHeader::new(
            RichText::new("Ajustes").size(14.0).color(theme::TEXT_DIM),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sensibilidad").size(13.0).color(theme::TEXT_DIM));
                ui.add(
                    egui::Slider::new(&mut config.sens_deg, 15.0..=60.0)
                        .suffix("°")
                        .fixed_decimals(0),
                );
            });
            ui.label(
                RichText::new("Grados de giro para cruzar la pantalla (menos = más rápido)")
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(4.0);
            ui.checkbox(
                &mut config.abs_mode,
                RichText::new("Apuntado absoluto (desactívalo para juegos)").size(13.0),
            );
            ui.add_space(4.0);
            ui.checkbox(
                &mut config.auto_dolphin,
                RichText::new("Configurar Dolphin automáticamente (multijugador)").size(13.0),
            );
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

        if (config.sens_deg, config.abs_mode, config.auto_dolphin) != before {
            // En caliente para el puntero ya; a disco cuando sueltes el
            // slider (arrastrarlo escribía el archivo en cada frame)
            self.shared.lock().unwrap().config = config;
            self.config_dirty = true;
        }
        if self.config_dirty && !ui.input(|i| i.pointer.any_down()) {
            config.save();
            self.config_dirty = false;
        }
    }
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
        theme::CARD,
        Stroke::new(1.5_f32, theme::CARD_BORDER),
    );

    let mut child = ui.child_ui(
        rect.shrink(16.0),
        egui::Layout::top_down(egui::Align::Min),
        None,
    );
    let mode_txt = match snap.mode {
        Mode::Pointer => "Modo puntero (apunta el Jugador 1)",
        Mode::Dolphin => "Modo Dolphin: todos juegan",
    };
    child.label(RichText::new(mode_txt).size(13.0).color(theme::BLUE));
    child.label(
        RichText::new(format!(
            "{:.0} paquetes/s · sensor {:.0} Hz",
            snap.pps, snap.sensor_hz
        ))
        .size(11.0)
        .color(theme::TEXT_DIM),
    );
    child.add_space(6.0);

    for (i, slot) in snap.players.iter().enumerate() {
        let Some(p) = slot else { continue };
        child.horizontal(|ui| {
            ui.label(
                RichText::new(format!("J{}", i + 1))
                    .size(13.0)
                    .strong()
                    .color(theme::CARD)
                    .background_color(theme::BLUE),
            );
            let name = if p.model.is_empty() {
                p.name.clone()
            } else {
                p.model.clone()
            };
            ui.label(RichText::new(name).size(13.0).color(theme::TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let rtt = p
                    .rtt_ms
                    .map(|r| format!("{r:.0} ms"))
                    .unwrap_or_else(|| "—".into());
                ui.label(
                    RichText::new(format!("{}% · {rtt}", p.battery_pct))
                        .size(12.0)
                        .color(theme::TEXT_DIM),
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

    painter.rect(
        rect,
        Rounding::same(theme::RADIUS),
        theme::CARD,
        Stroke::new(1.5_f32, theme::CARD_BORDER),
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
                    Color32::from_rgb(0x3B, 0x47, 0x50),
                );
            }
        }
    }
}
