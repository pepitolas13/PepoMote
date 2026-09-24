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
#[cfg(windows)]
mod gpu;
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
mod pad;
mod pairing;
mod pointer;
mod ports;
mod retroarch;
mod rumble;
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
    // La sonda de OpenGL (gpu.rs), antes que nada: sin log, cerrojo, red,
    // bandeja ni driver. Contesta una línea por stdout y termina
    #[cfg(windows)]
    if gpu::probe_from_args() {
        gpu::run_probe();
    }
    update::set_wake(singleton::request_show);
    // Lo primero de todo: que ningún pánico se pierda ni cierre el receptor
    // (el perfil release desenrolla; el hook lo deja en receptor.log)
    log::install_panic_hook();
    log::init();

    // --replay <grabación>: solo el motor del puntero sobre una grabación
    // (PEPOMOTE_RECORD), CSV por stdout, y fuera. --dolphin-dirs y --diag:
    // informes por stdout, y fuera.
    if pointer::record::replay_from_args()
        || dolphin::print_dirs_from_args()
        || diag::run_from_args()
        || rumble::install_driver_from_args()
    {
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
        launch::describe_startup()
    );

    // Instancia única: si ya hay un PepoMote vivo (quizá solo en la
    // bandeja), se le pide que se muestre y este proceso termina. Si el vivo
    // se había colgado antes de pintar con OpenGL, deja el sitio a este.
    #[cfg(windows)]
    let mut took_over = false;
    match singleton::acquire() {
        singleton::Singleton::Primary(lock) => singleton::watch(lock),
        singleton::Singleton::TookOver(lock) => {
            log_line!("El PepoMote abierto se había colgado antes de pintar y me ha dejado el sitio: sigo yo");
            #[cfg(windows)]
            {
                took_over = true;
            }
            singleton::watch(lock);
        }
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

    // Con qué pintará la ventana (Windows): la gráfica al log, lo recordado
    // para ella y, si hace falta, la sonda de OpenGL, que arranca ya y corre
    // en paralelo con la red y la bandeja (con --minimized, cuando se pida
    // la ventana). Antes de los hilos: la sonda no hereda nada de ellos.
    #[cfg(windows)]
    let mut window_plan =
        gpu::WindowPlan::begin(&shared, took_over, !std::env::args().any(|a| a == "--minimized"));

    let dsu = dsu::start(shared.clone());
    // El hub de la vibración sondea el driver del mando virtual en su propio
    // hilo y, en Windows, si falta lo instala él (el instalador viaja dentro
    // del exe). Aquí, en el hilo principal, no se le pregunta nada al driver:
    // un ViGEmBus que no contesta dejaba el receptor sin ventana ni red.
    rumble::start(shared.clone());
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
        // Ahora sí va a haber ventana: la sonda, si hace falta
        window_plan.ensure_probe();
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
    // Vigilante siempre activo: si la ventana no pinta, que quede en el log
    // qué la frena (con --minimized la ventana no existe hasta que se pide:
    // aquí ya se ha pedido)
    if !hidden_window {
        launch::start_paint_watchdog();
    }
    let smoke_fail_first = attempt.n == 1 && std::env::var_os(launch::ENV_SMOKE_FAIL_FIRST).is_some();
    let viewport = egui::ViewportBuilder::default()
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
        });

    // Windows: OpenGL, y si no sirve, Direct3D 12 (gpu.rs), en este mismo
    // proceso: eframe reutiliza su bucle de eventos entre intentos
    #[cfg(windows)]
    {
        let chain = window_plan.chain(&shared);
        let code = run_window_windows(&chain, viewport, shared, pairing, start_hidden, smoke_fail_first);
        launch::exit(code);
    }

    #[cfg(not(windows))]
    {
        let options = eframe::NativeOptions {
            viewport,
            renderer: eframe::Renderer::Glow,
            // Linux: backend forzado por el relanzamiento (PEPOMOTE_UI_BACKEND)
            #[cfg(target_os = "linux")]
            event_loop_builder: launch::event_loop_hook(attempt.backend),
            ..Default::default()
        };
        // run_native va en catch_unwind: sin una configuración GL usable eframe
        // puede entrar en pánico en el hilo principal (no devolver Err), y
        // aquí se sale por el log y, en Linux, relanzándose (launch::finish).
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
        launch::exit(code);
    }
}

/// Windows: la ventana con cada renderer de la cadena hasta que uno pinte.
/// Se pasa al siguiente solo si el intento falla antes de pintar Y sin
/// haber llegado a crear la app (la app no se crea dos veces). Devuelve el
/// código de salida.
#[cfg(windows)]
fn run_window_windows(
    chain: &[gpu::Renderer],
    viewport: egui::ViewportBuilder,
    shared: state::SharedState,
    pairing: pairing::PairingInfo,
    start_hidden: bool,
    smoke_fail_first: bool,
) -> i32 {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let mut outcome = launch::Outcome::FailedBeforeFrame("ningún renderer que probar".to_owned());
    for (i, &renderer) in chain.iter().enumerate() {
        let n = (i + 1) as u8;
        gpu::begin_attempt(&shared, renderer, n);
        let created = Arc::new(AtomicBool::new(false));
        let options = gpu::native_options(renderer, viewport.clone());
        let (s, p, c) = (shared.clone(), pairing.clone(), created.clone());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
            if smoke_fail_first && n == 1 {
                return Err("fallo simulado (PEPOMOTE_SMOKE_FAIL_FIRST)".to_owned());
            }
            eframe::run_native(
                "PepoMote",
                options,
                Box::new(move |cc| {
                    c.store(true, Ordering::SeqCst);
                    gpu::note_painter(cc);
                    gpu::fake_window_hang_if_requested();
                    singleton::set_ctx(cc.egui_ctx.clone());
                    singleton::remember_window(cc);
                    Ok(Box::new(app::PepoMoteApp::new(cc, s, p, start_hidden)))
                }),
            )
            .map_err(|e| format!("{e} ({e:?})"))
        }));
        outcome = launch::classify(result, launch::first_frame_done());
        gpu::end_attempt(&shared);
        if let launch::Outcome::FailedBeforeFrame(why) = &outcome {
            if !created.load(Ordering::SeqCst) {
                if let Some(next) = chain.get(i + 1) {
                    log_line!(
                        "Ventana: {} falló antes de pintar: {why}; pruebo {}",
                        launch::describe_current(),
                        next.name()
                    );
                    gpu::failed_before_frame(&shared, renderer);
                    continue;
                }
            }
        }
        break;
    }
    launch::finish(outcome, launch::attempt())
}
