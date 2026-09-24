//! Icono de bandeja (Windows) / de la barra de menús (macOS). No depende de
//! la UI: con --minimized existe desde el arranque aunque la ventana ni se
//! haya creado (Windows) o esté oculta (macOS), y "Mostrar" la crea o la
//! restaura. El icono y su tooltip cuentan el estado sin abrir nada: punto
//! verde con móviles conectados, cuántos son, en qué modo y quiénes; y si hay
//! versión nueva, el menú lo dice y la abre en el navegador.
//!
//! Windows: hilo propio con su bomba de mensajes win32. macOS: el icono solo
//! puede crearse y tocarse desde el hilo principal con el bucle en marcha, y
//! egui deja de repintar con la ventana oculta o minimizada; por eso el menú
//! se atiende en el handler (hilo principal) con AppKit directo y el refresco
//! va por la cola principal desde un hilo auxiliar.
#![cfg(any(windows, target_os = "macos"))]

use crate::state::{Mode, SharedState};
use crate::tr;
use crate::update::Version;
use std::sync::{Arc, Mutex};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Lo que la bandeja enseña; solo se toca cuando cambia.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraySnapshot {
    pub players: usize,
    pub mode: Mode,
    /// Nombres (modelo, o nombre si no hay) de los móviles conectados, por slot.
    pub names: Vec<String>,
    /// Versión nueva publicada (mayor que esta y no ocultada), si la hay.
    pub update: Option<Version>,
}

impl TraySnapshot {
    fn of(shared: &SharedState) -> Self {
        let s = shared.lock().unwrap_or_else(|e| e.into_inner());
        let names = s
            .players
            .iter()
            .flatten()
            .map(|p| if p.model.is_empty() { p.name.clone() } else { p.model.clone() })
            .collect();
        let update = crate::update::pending(&Version::current(), s.config.update_latest, None);
        Self { players: s.player_count(), mode: s.mode, names, update }
    }
}

/// Texto del tooltip. Windows lo corta a 128 caracteres: nos quedamos en 120.
pub fn tooltip(snap: &TraySnapshot) -> String {
    let mut text = if snap.players == 0 {
        tr!("tray.waiting").to_owned()
    } else {
        let mode = match snap.mode {
            Mode::Pointer => tr!("tray.mode_pointer"),
            Mode::Dolphin => tr!("tray.mode_dolphin"),
            Mode::Cemu => tr!("tray.mode_cemu"),
            Mode::Switch => tr!("tray.mode_switch"),
            Mode::RetroArch => tr!("tray.mode_retroarch"),
            Mode::Gamepad => tr!("tray.mode_gamepad"),
        };
        let plural = if snap.players == 1 { tr!("tray.phone_one") } else { tr!("tray.phone_many") };
        let mut text = tr!("tray.line", snap.players, plural, mode);
        let names = snap.names.join(", ");
        if !names.is_empty() {
            text.push('\n');
            text.push_str(&names);
        }
        text
    };
    if let Some(v) = &snap.update {
        text.push('\n');
        text.push_str(&tr!("tray.update", v));
    }
    if text.chars().count() > 120 {
        text = text.chars().take(119).collect::<String>() + "…";
    }
    text
}

/// El icono, su menú (Mostrar | — | Salir, y «Nueva versión X…» cuando la
/// hay) y su refresco. Común a Windows y macOS; quien lo crea decide el hilo.
pub struct TrayHandle {
    tray: TrayIcon,
    idle: Icon,
    live: Icon,
    menu: Menu,
    snap: TraySnapshot,
    update_item: MenuItem,
    update_inserted: bool,
    /// URL de la release anunciada (la lee el handler del menú).
    update_url: Arc<Mutex<Option<String>>>,
    pub show_id: MenuId,
    pub quit_id: MenuId,
    pub update_id: MenuId,
}

impl TrayHandle {
    pub fn build(shared: &SharedState) -> Result<TrayHandle, String> {
        // 32 px en Windows; 44 px (22 pt @2x) en la barra de menús de macOS
        let px: u32 = if cfg!(target_os = "macos") { 44 } else { 32 };
        let icon_of = |rgba: Vec<u8>| Icon::from_rgba(rgba, px, px).map_err(|e| e.to_string());
        let idle = icon_of(crate::icon::logo_rgba(px))?;
        let live = icon_of(crate::icon::logo_rgba_badge(px))?;
        let menu = Menu::new();
        let show = MenuItem::new(tr!("tray.show"), true, None);
        let quit = MenuItem::new(tr!("tray.quit"), true, None);
        // «Nueva versión X…»: existe desde el principio pero solo entra en
        // el menú (arriba del todo) cuando hay una que anunciar
        let update_item = MenuItem::new("", true, None);
        menu.append(&show).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator()).map_err(|e| e.to_string())?;
        menu.append(&quit).map_err(|e| e.to_string())?;
        let snap = TraySnapshot::of(shared);
        let builder = TrayIconBuilder::new()
            .with_icon(if snap.players > 0 { live.clone() } else { idle.clone() })
            .with_menu(Box::new(menu.clone()))
            // Windows: clic izquierdo = abrir la ventana (nuestro handler); el
            // menú solo con el derecho — sin esto la librería además abría el
            // menú con el izquierdo, descolocado. macOS: el menú con el
            // izquierdo, como cualquier icono de la barra de menús.
            .with_menu_on_left_click(cfg!(target_os = "macos"))
            .with_tooltip(tooltip(&snap));
        let tray = builder.build().map_err(|e| e.to_string())?;
        let mut handle = TrayHandle {
            tray,
            idle,
            live,
            menu,
            snap,
            show_id: show.id().clone(),
            quit_id: quit.id().clone(),
            update_id: update_item.id().clone(),
            update_item,
            update_inserted: false,
            update_url: Arc::new(Mutex::new(None)),
        };
        handle.announce();
        Ok(handle)
    }

    /// Icono y tooltip al día; la versión nueva, anunciada en cuanto aparece.
    pub fn refresh(&mut self, shared: &SharedState) {
        let now = TraySnapshot::of(shared);
        if now == self.snap {
            return;
        }
        let _ = self.tray.set_icon(Some(if now.players > 0 { self.live.clone() } else { self.idle.clone() }));
        let _ = self.tray.set_tooltip(Some(tooltip(&now)));
        self.snap = now;
        self.announce();
    }

    fn announce(&mut self) {
        let Some(v) = &self.snap.update else { return };
        if self.update_inserted {
            return;
        }
        self.update_item.set_text(tr!("tray.update", v));
        *self.update_url.lock().unwrap_or_else(|e| e.into_inner()) = Some(crate::update::release_url(v));
        let _ = self.menu.insert(&self.update_item, 0);
        let _ = self.menu.insert(&PredefinedMenuItem::separator(), 1);
        self.update_inserted = true;
    }

    /// La ranura de la URL, para el handler del menú (vive en otro hilo o closure).
    pub fn url_slot(&self) -> Arc<Mutex<Option<String>>> {
        self.update_url.clone()
    }
}

/// Handler del menú común: Mostrar / Salir / Nueva versión.
fn menu_handler(
    show_id: MenuId,
    quit_id: MenuId,
    update_id: MenuId,
    _url: Arc<Mutex<Option<String>>>,
    show: impl Fn() + Send + Sync + 'static,
) -> impl Fn(MenuEvent) + Send + Sync + 'static {
    move |ev: MenuEvent| {
        if *ev.id() == show_id {
            show();
        } else if *ev.id() == quit_id {
            quit_from_menu();
        } else if *ev.id() == update_id {
            crate::update::request_open();
            show();
        }
    }
}

/// «Salir» del menú. En Windows el manejador corre en el hilo del icono,
/// dentro del procedimiento de ventana de tray-icon: ahí no se puede soltar
/// el icono, así que se pide que el bucle termine (`PostQuitMessage`, que
/// ningún bucle modal pierde); el bucle lo suelta (sin icono fantasma) y sale.
fn quit_from_menu() {
    #[cfg(windows)]
    {
        use std::sync::atomic::Ordering::SeqCst;
        use windows::Win32::System::Threading::GetCurrentThreadId;
        QUIT.store(true, SeqCst);
        EXITING.store(true, SeqCst);
        let icon_thread = ICON_THREAD.load(SeqCst);
        if icon_thread != 0 && icon_thread == unsafe { GetCurrentThreadId() } {
            unsafe { windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0) };
            return;
        }
        crate::launch::exit(0);
    }
    #[cfg(not(windows))]
    std::process::exit(0);
}

/// Windows: el icono vive y muere en su hilo (`TrayIcon` no es `Send`); los
/// demás solo miran estas marcas y piden.
#[cfg(windows)]
static ICON_PRESENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Se está saliendo: no crear ni reponer más iconos.
#[cfg(windows)]
static EXITING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Quitar el icono ya (lo mira el bucle en cada mensaje y en cada `WM_TIMER`).
#[cfg(windows)]
static REMOVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// «Salir»: tras quitar el icono, terminar.
#[cfg(windows)]
static QUIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// El hilo que tiene el icono (para `PostThreadMessageW`); 0 = ninguno.
#[cfg(windows)]
static ICON_THREAD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Mensaje al hilo del icono: quítalo (una salida desde otro hilo).
#[cfg(windows)]
const WM_PMP_REMOVE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x51;

/// ¿Hay icono en la bandeja? Sin él, la X de la ventana minimiza en vez de
/// esconder (no habría desde dónde volver a abrirla).
#[cfg(windows)]
pub fn icon_present() -> bool {
    ICON_PRESENT.load(std::sync::atomic::Ordering::SeqCst)
}

/// Antes de salir desde otro hilo: el hilo del icono lo quita (su `Drop`
/// manda `NIM_DELETE`) y aquí se espera como mucho 1,5 s. Si el mensaje se
/// pierde (un menú abierto), el bucle lo ve en su `WM_TIMER` de cada
/// segundo. Sin icono, o desde el propio hilo del icono, vuelve al momento.
#[cfg(windows)]
pub fn remove_before_exit() {
    use std::sync::atomic::Ordering::SeqCst;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
    EXITING.store(true, SeqCst);
    if !ICON_PRESENT.load(SeqCst) {
        return;
    }
    let icon_thread = ICON_THREAD.load(SeqCst);
    if icon_thread == 0 || icon_thread == unsafe { GetCurrentThreadId() } {
        return;
    }
    REMOVE.store(true, SeqCst);
    let _ = unsafe { PostThreadMessageW(icon_thread, WM_PMP_REMOVE, WPARAM(0), LPARAM(0)) };
    let start = std::time::Instant::now();
    while ICON_PRESENT.load(SeqCst) && start.elapsed() < std::time::Duration::from_millis(1500) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Espera entre intentos de crear el icono: 2, 4, 8, 16 y luego cada 30 s.
#[cfg(any(windows, test))]
pub fn retry_after(tries: u32) -> std::time::Duration {
    std::time::Duration::from_secs(match tries {
        0 | 1 => 2,
        2 => 4,
        3 => 8,
        4 => 16,
        _ => 30,
    })
}

/// Windows: un hilo vigila y cada intento de icono va en su propio hilo con
/// bomba de mensajes win32 (los handlers corren en DispatchMessage) y un
/// WM_TIMER por segundo que refresca icono y tooltip.
///
/// Al arrancar con Windows (autoarranque) la barra de tareas puede no
/// existir aún: se espera a que exista y, si crear el icono falla igual, se
/// reintenta. Antes el hilo se rendía a la primera y el receptor quedaba
/// vivo sin icono ni ventana (solo en el Administrador de tareas). Cada
/// intento en un hilo nuevo: un intento fallido de tray-icon deja una
/// ventana oculta que, al reiniciarse el Explorador, pondría un icono sin
/// menú; Windows la destruye al terminar el hilo que la creó.
#[cfg(windows)]
pub fn start(shared: SharedState) {
    use std::sync::atomic::Ordering::SeqCst;
    use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

    std::thread::Builder::new()
        .name("pmp-tray".into())
        .spawn(move || {
            // Handlers directos (los canales drenados a mano perdían clicks)
            TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| match ev {
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
                | TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => crate::singleton::request_show(),
                _ => {}
            }));
            let mut tries = 0u32;
            let mut waited_taskbar = false;
            loop {
                if EXITING.load(SeqCst) {
                    return;
                }
                if !taskbar_ready() {
                    if !waited_taskbar {
                        crate::log_line!("Bandeja: la barra de tareas aún no existe; espero para poner el icono");
                        waited_taskbar = true;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
                tries += 1;
                let (tx, rx) = std::sync::mpsc::channel();
                let s = shared.clone();
                let Ok(icon) = std::thread::Builder::new().name("pmp-tray-icon".into()).spawn(move || run_icon(s, tx))
                else {
                    return;
                };
                match rx.recv() {
                    Ok(Ok(())) => {
                        if tries > 1 || waited_taskbar {
                            crate::log_line!("Bandeja: icono puesto (intento {tries})");
                        }
                        let _ = icon.join();
                        if EXITING.load(SeqCst) {
                            return;
                        }
                        // El bucle del icono terminó sin que nadie lo pidiera:
                        // se vuelve a poner, que el receptor no quede sin él
                        crate::log_line!("Bandeja: el icono se fue sin pedirlo; lo pongo otra vez");
                        std::thread::sleep(retry_after(tries));
                    }
                    Ok(Err(e)) => {
                        let _ = icon.join();
                        let wait = retry_after(tries);
                        crate::log_line!("Bandeja: no se pudo poner el icono ({e}); reintento en {} s", wait.as_secs());
                        std::thread::sleep(wait);
                    }
                    // El hilo del icono murió sin contestar (un pánico, ya en el log)
                    Err(_) => {
                        let _ = icon.join();
                        std::thread::sleep(retry_after(tries));
                    }
                }
            }
        })
        .expect("hilo tray");
}

/// ¿Existe ya la barra de tareas del Explorador?
#[cfg(windows)]
fn taskbar_ready() -> bool {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.is_ok_and(|h| !h.is_invalid())
}

/// El hilo del icono: lo crea, cuenta si pudo, atiende sus mensajes y, al
/// terminar el bucle («Salir» o una salida desde otro hilo), lo suelta
/// (`NIM_DELETE`) antes de nada más.
#[cfg(windows)]
fn run_icon(shared: SharedState, report: std::sync::mpsc::Sender<Result<(), String>>) {
    use std::sync::atomic::Ordering::SeqCst;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PeekMessageW, SetTimer, TranslateMessage, MSG, PM_NOREMOVE, WM_TIMER,
    };
    unsafe {
        // La cola de mensajes del hilo, antes de dar su id: sin ella,
        // PostThreadMessageW no tiene dónde dejar nada
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        ICON_THREAD.store(GetCurrentThreadId(), SeqCst);
    }
    let mut handle = match TrayHandle::build(&shared) {
        Ok(h) => h,
        Err(e) => {
            let _ = report.send(Err(e));
            return;
        }
    };
    MenuEvent::set_event_handler(Some(menu_handler(
        handle.show_id.clone(),
        handle.quit_id.clone(),
        handle.update_id.clone(),
        handle.url_slot(),
        crate::singleton::request_show,
    )));
    ICON_PRESENT.store(true, SeqCst);
    let _ = report.send(Ok(()));
    if !EXITING.load(SeqCst) {
        unsafe {
            let _ = SetTimer(None, 0, 1000, None);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if REMOVE.load(SeqCst) || (msg.hwnd.is_invalid() && msg.message == WM_PMP_REMOVE) {
                    break;
                }
                if msg.message == WM_TIMER {
                    handle.refresh(&shared);
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    drop(handle);
    ICON_PRESENT.store(false, SeqCst);
    crate::log_line!("Bandeja: icono retirado");
    if QUIT.load(SeqCst) {
        crate::log_line!("Salir (bandeja): salgo");
        std::process::exit(0);
    }
}

#[cfg(target_os = "macos")]
static TRAY: std::sync::OnceLock<objc2_foundation::MainThreadBound<std::cell::RefCell<TrayHandle>>> =
    std::sync::OnceLock::new();

/// macOS: se llama desde el hilo principal con el bucle de eventos ya en
/// marcha (el constructor de la app de eframe). El menú se atiende en su
/// handler (hilo principal) con AppKit directo; el refresco, cada segundo por
/// la cola principal desde un hilo auxiliar: ninguno de los dos depende del
/// repintado de egui.
#[cfg(target_os = "macos")]
pub fn start_main_thread(shared: SharedState) {
    use objc2_foundation::{MainThreadBound, MainThreadMarker};
    use std::cell::RefCell;

    let Some(mtm) = MainThreadMarker::new() else { return };
    let Ok(handle) = TrayHandle::build(&shared) else { return };
    let handler = menu_handler(
        handle.show_id.clone(),
        handle.quit_id.clone(),
        handle.update_id.clone(),
        handle.url_slot(),
        crate::macos::show_windows,
    );
    if TRAY.set(MainThreadBound::new(RefCell::new(handle), mtm)).is_err() {
        return;
    }
    MenuEvent::set_event_handler(Some(handler));
    let _ = std::thread::Builder::new().name("pmp-tray".into()).spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let shared = shared.clone();
        crate::macos::on_main(move || {
            if let (Some(mtm), Some(t)) = (MainThreadMarker::new(), TRAY.get()) {
                t.get(mtm).borrow_mut().refresh(&shared);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(players: usize, mode: Mode, names: &[&str]) -> TraySnapshot {
        TraySnapshot { players, mode, names: names.iter().map(|s| s.to_string()).collect(), update: None }
    }

    #[test]
    fn el_tooltip_dice_cuantos_en_que_modo_y_quienes() {
        assert_eq!(tooltip(&snap(0, Mode::Pointer, &[])), "PepoMote · esperando al móvil");
        assert_eq!(tooltip(&snap(1, Mode::Pointer, &["Pixel 8"])), "PepoMote · 1 móvil · Puntero\nPixel 8");
        assert_eq!(
            tooltip(&snap(2, Mode::Dolphin, &["Pixel 8", "OnePlus 6T"])),
            "PepoMote · 2 móviles · Dolphin\nPixel 8, OnePlus 6T"
        );
        assert_eq!(tooltip(&snap(3, Mode::Cemu, &[])), "PepoMote · 3 móviles · Wii U");
        assert_eq!(tooltip(&snap(1, Mode::Switch, &["Pixel 8"])), "PepoMote · 1 móvil · Switch\nPixel 8");
    }

    #[test]
    fn el_tooltip_anuncia_la_version_nueva() {
        let mut s = snap(0, Mode::Pointer, &[]);
        s.update = Some(Version([1, 6, 0]));
        assert_eq!(tooltip(&s), "PepoMote · esperando al móvil\nNueva versión 1.6.0…");
        let mut s = snap(1, Mode::Cemu, &["Pixel 8"]);
        s.update = Some(Version([2, 0, 0]));
        assert_eq!(tooltip(&s), "PepoMote · 1 móvil · Wii U\nPixel 8\nNueva versión 2.0.0…");
    }

    #[test]
    fn el_icono_se_reintenta_cada_vez_con_mas_calma() {
        let secs: Vec<u64> = (1..=7).map(|n| retry_after(n).as_secs()).collect();
        assert_eq!(secs, vec![2, 4, 8, 16, 30, 30, 30]);
    }

    #[test]
    fn el_tooltip_no_pasa_de_120_caracteres() {
        let long = "x".repeat(200);
        let mut t = snap(4, Mode::Pointer, &[&long, &long, &long, &long]);
        t.update = Some(Version([1, 6, 0]));
        let t = tooltip(&t);
        assert_eq!(t.chars().count(), 120);
        assert!(t.ends_with('…'));
    }
}
