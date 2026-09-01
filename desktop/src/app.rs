use crate::state::{AppState, LinkStatus};
use crate::theme;
use egui::{Color32, Pos2, Rect, RichText, Rounding, Stroke, Vec2};

pub struct PepoMoteApp {
    state: AppState,
    qr_modules: Vec<bool>,
    qr_width: usize,
}

impl PepoMoteApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let state = AppState::new();
        let (qr_modules, qr_width) = build_qr(&state.pairing.pair_url());
        Self {
            state,
            qr_modules,
            qr_width,
        }
    }
}

impl eframe::App for PepoMoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

                    draw_qr_card(ui, &self.qr_modules, self.qr_width);

                    ui.add_space(14.0);
                    let (dot, label) = match self.state.status {
                        LinkStatus::Waiting => (theme::TEXT_DIM, "Esperando al móvil…"),
                        LinkStatus::Connected => (theme::OK, "Conectado"),
                    };
                    ui.horizontal(|ui| {
                        let total = ui.available_width();
                        let text_w = 150.0;
                        ui.add_space((total - text_w).max(0.0) / 2.0);
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 5.0, dot);
                        ui.label(RichText::new(label).color(theme::TEXT_DIM));
                    });

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!(
                            "Escanea con la app PepoMote · {} : {}",
                            self.state.pairing.host, self.state.pairing.port
                        ))
                        .size(12.0)
                        .color(theme::TEXT_DIM),
                    );

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

/// Genera la matriz de módulos del QR de emparejamiento.
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

/// Tarjeta blanca con el QR dibujado a mano (sin dependencia de imágenes).
fn draw_qr_card(ui: &mut egui::Ui, modules: &[bool], width: usize) {
    let card_size = 280.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(card_size), egui::Sense::hover());
    let painter = ui.painter();

    painter.rect(
        rect,
        Rounding::same(theme::RADIUS),
        theme::CARD,
        Stroke::new(1.5, theme::CARD_BORDER),
    );

    if width == 0 {
        return;
    }

    // Zona tranquila de 4 módulos
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
                let min = Pos2::new(
                    origin.x + x as f32 * module,
                    origin.y + y as f32 * module,
                );
                painter.rect_filled(
                    Rect::from_min_size(min, Vec2::splat(module + 0.5)),
                    0.0,
                    Color32::from_rgb(0x3B, 0x47, 0x50),
                );
            }
        }
    }
}
