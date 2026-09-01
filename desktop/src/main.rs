#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod pairing;
mod state;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 640.0])
            .with_min_inner_size([380.0, 540.0])
            .with_title("PepoMote"),
        ..Default::default()
    };
    eframe::run_native(
        "PepoMote",
        options,
        Box::new(|cc| Ok(Box::new(app::PepoMoteApp::new(cc)))),
    )
}
