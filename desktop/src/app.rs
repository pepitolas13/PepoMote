use crate::pairing::PairingInfo;
use crate::state::{LinkStatus, Mode, SharedState};
use crate::theme;
use egui::{Color32, Pos2, Rect, RichText, Rounding, Stroke, Vec2};
use std::time::Duration;

pub struct PepoMoteApp {
    shared: SharedState,
    pairing: PairingInfo,
    qr_modules: Vec<bool>,
    qr_width: usize,
    autostart: bool,
    /// Arranque --minimized: minimizar a la barra durante los primeros 2.5 s
    /// (estado ortogonal a la visibilidad: la recreación de ventana y el
    /// re-show de eframe no lo deshacen; SW_HIDE y Visible(false) sí perdían).
    minimize_until: Option<std::time::Instant>,
}

impl PepoMoteApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shared: SharedState,
        pairing: PairingInfo,
        start_minimized: bool,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        let (qr_modules, qr_width) = build_qr(&pairing.pair_url());
        Self {
            shared,
            pairing,
            qr_modules,
            qr_width,
            autostart: crate::autostart::is_enabled(),
            minimize_until: start_minimized
                .then(|| std::time::Instant::now() + Duration::from_millis(2500)),
        }
    }
}

struct Snapshot {
    status: LinkStatus,
    mode: Mode,
    device: String,
    battery: u8,
    pps: f32,
    sensor_hz: f32,
    rtt_ms: Option<f32>,
    dsu_clients: usize,
    error: Option<String>,
}

impl eframe::App for PepoMoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(100));

        if let Some(t) = self.minimize_until {
            if std::time::Instant::now() < t {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                ctx.request_repaint_after(Duration::from_millis(100));
            } else {
                self.minimize_until = None;
            }
        }

        // En Windows, cerrar = esconder a la bandeja ("Salir" está en el tray)
        #[cfg(windows)]
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        let snap = {
            let s = self.shared.lock().unwrap();
            Snapshot {
                status: s.status,
                mode: s.mode,
                device: if s.device_model.is_empty() {
                    s.device_name.clone()
                } else {
                    format!("{} ({})", s.device_name, s.device_model)
                },
                battery: s.battery_pct,
                pps: s.pps,
                sensor_hz: s.sensor_hz,
                rtt_ms: s.rtt_ms,
                dsu_clients: s.dsu_clients,
                error: s.last_error.clone(),
            }
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme::BACKGROUND).inner_margin(24.0))
            .show(ctx, |ui| {
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

                    match snap.status {
                        LinkStatus::Waiting => self.ui_waiting(ui),
                        LinkStatus::Connected => ui_connected(ui, &snap),
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
    }
}

impl PepoMoteApp {
    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let mut config = self.shared.lock().unwrap().config;
        let before = (config.sens_deg, config.abs_mode);

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

        if (config.sens_deg, config.abs_mode) != before {
            config.save();
            self.shared.lock().unwrap().config = config;
        }
    }

    fn ui_waiting(&self, ui: &mut egui::Ui) {
        draw_qr_card(ui, &self.qr_modules, self.qr_width);
        ui.add_space(14.0);
        status_row(ui, theme::TEXT_DIM, "Esperando al móvil…");
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "Escanea con la app PepoMote · {} : {}",
                self.pairing.host, self.pairing.port
            ))
            .size(12.0)
            .color(theme::TEXT_DIM),
        );
    }
}

fn ui_connected(ui: &mut egui::Ui, snap: &Snapshot) {
    let card_w = 320.0f32.min(ui.available_width());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(card_w, 214.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        Rounding::same(theme::RADIUS),
        theme::CARD,
        Stroke::new(1.5_f32,theme::CARD_BORDER),
    );

    let mut child = ui.child_ui(
        rect.shrink(18.0),
        egui::Layout::top_down(egui::Align::Min),
        None,
    );
    child.label(RichText::new(&snap.device).size(17.0).strong().color(theme::TEXT));
    child.add_space(6.0);
    let mode_txt = match snap.mode {
        Mode::Pointer => "Modo puntero",
        Mode::Dolphin => "Modo Dolphin",
    };
    child.label(RichText::new(mode_txt).size(13.0).color(theme::BLUE));
    child.add_space(10.0);
    stat_line(&mut child, "Paquetes/s", &format!("{:.0}", snap.pps));
    stat_line(&mut child, "Sensor", &format!("{:.0} Hz", snap.sensor_hz));
    stat_line(
        &mut child,
        "RTT",
        &snap
            .rtt_ms
            .map(|r| format!("{r:.1} ms"))
            .unwrap_or_else(|| "—".into()),
    );
    stat_line(&mut child, "Batería del móvil", &format!("{}%", snap.battery));
    if snap.mode == Mode::Dolphin {
        stat_line(
            &mut child,
            "Dolphin",
            &format!("{} cliente(s) DSU", snap.dsu_clients),
        );
    }

    ui.add_space(14.0);
    status_row(ui, theme::OK, "Conectado");
}

fn stat_line(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(13.0).color(theme::TEXT_DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(13.0).strong().color(theme::TEXT));
        });
    });
}

fn status_row(ui: &mut egui::Ui, dot: Color32, label: &str) {
    ui.horizontal(|ui| {
        let total = ui.available_width();
        ui.add_space((total - 150.0).max(0.0) / 2.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 5.0, dot);
        ui.label(RichText::new(label).color(theme::TEXT_DIM));
    });
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

fn draw_qr_card(ui: &mut egui::Ui, modules: &[bool], width: usize) {
    let card_size = ui.available_width().clamp(180.0, 280.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(card_size), egui::Sense::hover());
    let painter = ui.painter();

    painter.rect(
        rect,
        Rounding::same(theme::RADIUS),
        theme::CARD,
        Stroke::new(1.5_f32,theme::CARD_BORDER),
    );

    if width == 0 {
        return;
    }

    let quiet = 4;
    let total = width + quiet * 2;
    let module = (card_size - 24.0) / total as f32;
    let origin = Pos2::new(
        rect.min.x + 12.0 + quiet as f32 * module,
        rect.min.y + 12.0 + quiet as f32 * module,
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
