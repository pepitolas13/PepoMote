#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod dsu;
mod icon;
mod input;
mod net;
mod pairing;
mod pointer;
mod singleton;
mod sound;
mod state;
mod theme;
#[cfg(windows)]
mod tray;

fn main() -> eframe::Result {
    // Instancia única: si ya hay un PepoMote vivo (quizá escondido en la
    // bandeja), se le pide que se muestre y este proceso termina.
    match singleton::acquire() {
        singleton::Singleton::Primary(lock) => singleton::watch(lock),
        singleton::Singleton::AlreadyRunning => return Ok(()),
    }

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

    // --minimized (autoarranque): en Windows nace escondido en la bandeja.
    // En Linux se ignora (no todos los escritorios tienen bandeja).
    // eframe recrea la ventana durante el arranque (~1 s) y re-muestra si se
    // esconde por WinAPI externo; el escondite fiable es el ViewportCommand
    // (fija el estado interno de eframe, igual que cerrar-a-bandeja), enviado
    // DESPUÉS de la recreación. Lo hace la propia app a los 1.5 s.
    #[cfg(windows)]
    let start_hidden = std::env::args().any(|a| a == "--minimized");
    #[cfg(not(windows))]
    let start_hidden = false;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 640.0])
            .with_min_inner_size([360.0, 480.0])
            .with_visible(!start_hidden)
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
            singleton::set_ctx(cc.egui_ctx.clone());
            #[cfg(windows)]
            let _ = ctx_tx.send(cc.egui_ctx.clone());
            Ok(Box::new(app::PepoMoteApp::new(
                cc,
                shared,
                pairing,
                start_hidden,
            )))
        }),
    )
}
