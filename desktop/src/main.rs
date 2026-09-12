#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod auto_mode;
mod autostart;
mod cemu;
mod diag;
mod dolphin;
mod dsu;
#[cfg(target_os = "linux")]
mod firewall;
#[cfg(any(target_os = "linux", test))]
mod fixes;
#[cfg(target_os = "linux")]
mod screens;
mod icon;
mod input;
mod log;
mod net;
mod pairing;
mod pointer;
mod ports;
mod screen;
mod singleton;
mod sound;
mod state;
mod theme;
#[cfg(windows)]
mod tray;

fn main() -> eframe::Result {
    // Lo primero de todo: que ningún pánico se pierda ni cierre el receptor
    // (el perfil release desenrolla; el hook lo deja en receptor.log)
    log::install_panic_hook();
    log::init();

    // --replay <grabación>: solo el motor del puntero sobre una grabación
    // (PEPOMOTE_RECORD), CSV por stdout, y fuera. --dolphin-dirs y --diag:
    // informes por stdout, y fuera.
    if pointer::record::replay_from_args() || dolphin::print_dirs_from_args() || diag::run_from_args() {
        return Ok(());
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    log_line!(
        "PepoMote {} arranca · {} {} · args {:?} · XDG_SESSION_TYPE={} WAYLAND_DISPLAY={} DISPLAY={} XDG_CURRENT_DESKTOP={}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        args,
        diag::env_or("XDG_SESSION_TYPE"),
        diag::env_or("WAYLAND_DISPLAY"),
        diag::env_or("DISPLAY"),
        diag::env_or("XDG_CURRENT_DESKTOP")
    );

    // Instancia única: si ya hay un PepoMote vivo (quizá solo en la
    // bandeja), se le pide que se muestre y este proceso termina.
    match singleton::acquire() {
        singleton::Singleton::Primary(lock) => singleton::watch(lock),
        singleton::Singleton::AlreadyRunning(e) => {
            log_line!(
                "Ya hay un PepoMote escuchando en 127.0.0.1:{} ({e}): le pido que se muestre y salgo",
                singleton::port()
            );
            return Ok(());
        }
    }

    let shared = state::new_shared();
    log::attach_shared(shared.clone());
    let pairing = pairing::PairingInfo::generate();

    let dsu = dsu::start(shared.clone());
    let hub = screen::ScreenHub::new(shared.clone());
    net::start(shared.clone(), pairing.clone(), dsu, hub);
    screen::start_minder(shared.clone());
    auto_mode::start_watcher(shared.clone());

    #[cfg(target_os = "linux")]
    firewall::watch(shared.clone(), pairing.port);
    #[cfg(target_os = "linux")]
    screens::watch(shared.clone());

    #[cfg(windows)]
    if std::env::var_os("PEPOMOTE_NO_TRAY").is_none() {
        tray::start(shared.clone());
    }

    // --minimized (autoarranque, Windows): la ventana NI SE CREA — solo red
    // y bandeja. Ni flash, ni botón en la barra: imposible re-mostrar lo que
    // no existe. El primer "Mostrar" (bandeja o relanzar el exe) desbloquea
    // la creación y a partir de ahí es una ventana normal.
    // En Linux se ignora (no todos los escritorios tienen bandeja).
    #[cfg(windows)]
    let start_hidden = std::env::args().any(|a| a == "--minimized");
    #[cfg(not(windows))]
    let start_hidden = false;
    if start_hidden {
        let (show_tx, show_rx) = std::sync::mpsc::channel::<()>();
        singleton::set_show_signal(show_tx);
        // Bloquea hasta que alguien pida la ventana. Los hilos de red y la
        // bandeja ya están vivos: el mando funciona sin UI.
        let _ = show_rx.recv();
    }

    // PEPOMOTE_NO_UI (Linux, solo pruebas): red e inyección sin ventana. El
    // e2e de CI corre en un sway sin cabeza donde no hay GPU ni EGL, y la
    // ventana no aporta nada a lo que se prueba.
    #[cfg(target_os = "linux")]
    if std::env::var_os("PEPOMOTE_NO_UI").is_some() {
        log_line!("PEPOMOTE_NO_UI: sin ventana (solo para pruebas)");
        loop {
            std::thread::park();
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 640.0])
            .with_min_inner_size([360.0, 480.0])
            .with_title("PepoMote")
            // app_id (Wayland) / WM_CLASS (X11): así el compositor casa la
            // ventana con PepoMote.desktop (icono, agrupación, reglas)
            .with_app_id("PepoMote")
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
            Ok(Box::new(app::PepoMoteApp::new(cc, shared, pairing)))
        }),
    )
}
