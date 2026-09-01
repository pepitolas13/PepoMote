#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod dsu;
mod icon;
mod input;
mod net;
mod pairing;
mod pointer;
mod sound;
mod state;
mod theme;
#[cfg(windows)]
mod tray;

fn main() -> eframe::Result {
    let shared = state::new_shared();
    let pairing = pairing::PairingInfo::generate();

    let dsu = dsu::start(shared.clone());
    net::start(shared.clone(), pairing.clone(), dsu);

    #[cfg(windows)]
    let (ctx_tx, ctx_rx) = std::sync::mpsc::channel::<egui::Context>();
    #[cfg(windows)]
    if std::env::var_os("PEPOMOTE_NO_TRAY").is_none() {
        tray::start(ctx_rx);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 640.0])
            .with_min_inner_size([360.0, 480.0])
            .with_title("PepoMote")
            .with_icon(egui::IconData {
                rgba: icon::logo_rgba(64),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "PepoMote",
        options,
        Box::new(move |cc| {
            #[cfg(windows)]
            let _ = ctx_tx.send(cc.egui_ctx.clone());
            Ok(Box::new(app::PepoMoteApp::new(cc, shared, pairing)))
        }),
    )
}
