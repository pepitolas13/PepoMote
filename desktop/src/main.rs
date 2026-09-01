#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod dsu;
mod input;
mod net;
mod pairing;
mod pointer;
mod state;
mod theme;

fn main() -> eframe::Result {
    let shared = state::new_shared();
    let pairing = pairing::PairingInfo::generate();

    let dsu_tx = dsu::start(shared.clone());
    net::start(shared.clone(), pairing.clone(), dsu_tx);

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
        Box::new(move |cc| Ok(Box::new(app::PepoMoteApp::new(cc, shared, pairing)))),
    )
}
