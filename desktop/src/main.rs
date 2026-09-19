#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod auto_mode;
mod autostart;
mod cemu;
mod diag;
mod dolphin;
mod dsu;
mod eden;
mod firewall;
#[cfg(any(target_os = "linux", test))]
mod fixes;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod screens;
mod i18n;
mod icon;
mod ini;
mod input;
mod launch;
mod log;
#[cfg(target_os = "macos")]
mod macos;
mod net;
#[cfg(target_os = "macos")]
mod procs;
mod pairing;
mod pointer;
mod ports;
mod retroarch;
mod screen;
mod singleton;
mod sound;
mod state;
mod strings;
mod theme;
mod threads;
#[cfg(any(windows, target_os = "macos"))]
mod tray;
mod update;
#[cfg(windows)]
mod win_power;

use crate::state::LockTolerant;

fn main() {
    if update::install::run_from_args() { return; }
    update::set_wake(singleton::request_show);
    // Lo primero de todo: que ningún pánico se pierda ni cierre el receptor
    // (el perfil release desenrolla; el hook lo deja en receptor.log)
    log::install_panic_hook();
    log::init();

    // --replay <grabación>: solo el motor del puntero sobre una grabación
    // (PEPOMOTE_RECORD), CSV por stdout, y fuera. --dolphin-dirs y --diag:
    // informes por stdout, y fuera.
    if pointer::record::replay_from_args() || dolphin::print_dirs_from_args() || diag::run_from_args() {
        return;
    }
    // Windows 11 estrangula los procesos sin foco (el receptor casi siempre
    // está detrás del juego): fuera del ahorro de energía desde el principio
    #[cfg(windows)]
    win_power::opt_out_of_throttling();

    let args: Vec<String> = std::env::args().skip(1).collect();
    log_line!(
        "PepoMote {} arranca · {} {} · args {:?} · XDG_SESSION_TYPE={} WAYLAND_DISPLAY={} DISPLAY={} XDG_CURRENT_DESKTOP={} · ventana: {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        args,
        diag::env_or("XDG_SESSION_TYPE"),
        diag::env_or("WAYLAND_DISPLAY"),
        diag::env_or("DISPLAY"),
        diag::env_or("XDG_CURRENT_DESKTOP"),
        launch::describe(launch::attempt())
    );

    // Instancia única: si ya hay un PepoMote vivo (quizá solo en la
    // bandeja), se le pide que se muestre y este proceso termina.
    match singleton::acquire() {
        singleton::Singleton::Primary(lock) => singleton::watch(lock),
        singleton::Singleton::AlreadyRunning => {
            log_line!(
                "Ya hay un PepoMote escuchando en 127.0.0.1:{}: le he pedido que se muestre y salgo",
                singleton::port()
            );
            return;
        }
        singleton::Singleton::NoLock(e) => {
            log_line!(
                "Cerrojo de instancia única en 127.0.0.1:{} no disponible ({e}): sigo sin cerrojo",
                singleton::port()
            );
        }
    }

    let shared = state::new_shared();
    log::attach_shared(shared.clone());
    // Idioma: el guardado en Ajustes; si no, el del sistema (español si no es inglés)
    {
        let saved = shared.lock_tolerant().config.lang.as_deref().and_then(i18n::Lang::parse);
        i18n::set(saved.unwrap_or_else(i18n::detect_system));
    }
    let pairing = pairing::PairingInfo::generate();

    let dsu = dsu::start(shared.clone());
    // El enlace con RetroArch (mando en red + interfaz de comandos) vive
    // siempre: sondea en silencio y solo habla en modo RetroArch
    let _ = retroarch::start(shared.clone());
    let hub = screen::ScreenHub::new(shared.clone());
    net::start(shared.clone(), pairing.clone(), dsu, hub);
    screen::start_minder(shared.clone());
    auto_mode::start_watcher(shared.clone());

    // Aviso de versión nueva: el manifiesto de GitHub al arrancar y cada hora (se apaga en Ajustes);
    // el resultado se guarda en settings.json y la ventana lo enseña
    {
        let (s1, s2, s3) = (shared.clone(), shared.clone(), shared.clone());
        update::spawn(
            move || s1.lock_tolerant().config.update_check,
            move || s2.lock_tolerant().config.update_last_check,
            move |now, latest| {
                let mut s = s3.lock_tolerant();
                s.config.update_last_check = now;
                s.config.update_latest = Some(latest);
                let offer = latest > update::Version::current() && s.config.update_dismissed != Some(latest) && s.config.update_check;
                s.config.save();
                drop(s);
                if offer { singleton::request_show(); }
            },
            |m| log_line!("{m}"),
        );
    }

    #[cfg(target_os = "linux")]
    firewall::watch(shared.clone(), pairing.port);
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    screens::watch(shared.clone());

    #[cfg(windows)]
    if std::env::var_os("PEPOMOTE_NO_TRAY").is_none() {
        tray::start(shared.clone());
    }

    // --minimized (autoarranque, Windows): la ventana NI SE CREA — solo red
    // y bandeja. Ni flash, ni botón en la barra: imposible re-mostrar lo que
    // no existe. El primer "Mostrar" (bandeja o relanzar el exe) desbloquea
    // la creación y a partir de ahí es una ventana normal.
    // macOS: la ventana se crea oculta y la app sin icono en el Dock (el
    // icono de la barra de menús necesita el bucle de eventos); «Mostrar» la
    // enseña. En Linux se ignora (no todos los escritorios tienen bandeja).
    #[cfg(any(windows, target_os = "macos"))]
    let start_hidden = std::env::args().any(|a| a == "--minimized");
    #[cfg(not(any(windows, target_os = "macos")))]
    let start_hidden = false;
    #[cfg(windows)]
    if start_hidden {
        let (show_tx, show_rx) = std::sync::mpsc::channel::<()>();
        singleton::set_show_signal(show_tx);
        // Bloquea hasta que alguien pida la ventana. Los hilos de red y la
        // bandeja ya están vivos: el mando funciona sin UI.
        let _ = show_rx.recv();
    }
    let hidden_window = cfg!(target_os = "macos") && start_hidden;

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

    let attempt = launch::attempt();
    launch::start_smoke_watchdog();
    let smoke_fail_first = attempt.n == 1 && std::env::var_os(launch::ENV_SMOKE_FAIL_FIRST).is_some();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 640.0])
            .with_min_inner_size([360.0, 480.0])
            .with_title("PepoMote")
            // app_id (Wayland) / WM_CLASS (X11): así el compositor casa la
            // ventana con PepoMote.desktop (icono, agrupación, reglas)
            .with_app_id("PepoMote")
            .with_visible(!hidden_window)
            .with_icon(egui::IconData {
                rgba: icon::logo_rgba(64),
                width: 64,
                height: 64,
            }),
        // Linux: backend forzado por el relanzamiento (PEPOMOTE_UI_BACKEND)
        #[cfg(target_os = "linux")]
        event_loop_builder: launch::event_loop_hook(attempt.backend),
        ..Default::default()
    };
    // run_native va en catch_unwind: sin una configuración GL usable eframe
    // entra en pánico en el hilo principal (no devuelve Err), y winit no
    // permite un segundo bucle de eventos en el mismo proceso, así que la
    // salida es el log y, en Linux, relanzarse (launch::finish).
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
        if smoke_fail_first {
            return Err("fallo simulado (PEPOMOTE_SMOKE_FAIL_FIRST)".to_owned());
        }
        eframe::run_native(
            "PepoMote",
            options,
            Box::new(move |cc| {
                singleton::set_ctx(cc.egui_ctx.clone());
                Ok(Box::new(app::PepoMoteApp::new(cc, shared, pairing, start_hidden)))
            }),
        )
        .map_err(|e| format!("{e} ({e:?})"))
    }));
    let code = launch::finish(launch::classify(result, launch::first_frame_done()), attempt);
    std::process::exit(code);
}
