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
        let update = crate::update::pending(&Version::current(), s.config.update_latest, s.config.update_dismissed);
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
    pub fn build(shared: &SharedState) -> Option<TrayHandle> {
        // 32 px en Windows; 44 px (22 pt @2x) en la barra de menús de macOS
        let px: u32 = if cfg!(target_os = "macos") { 44 } else { 32 };
        let icon_of = |rgba: Vec<u8>| Icon::from_rgba(rgba, px, px).ok();
        let idle = icon_of(crate::icon::logo_rgba(px))?;
        let live = icon_of(crate::icon::logo_rgba_badge(px))?;
        let menu = Menu::new();
        let show = MenuItem::new(tr!("tray.show"), true, None);
        let quit = MenuItem::new(tr!("tray.quit"), true, None);
        // «Nueva versión X…»: existe desde el principio pero solo entra en
        // el menú (arriba del todo) cuando hay una que anunciar
        let update_item = MenuItem::new("", true, None);
        menu.append(&show).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&quit).ok()?;
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
        let tray = builder.build().ok()?;
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
        Some(handle)
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
    url: Arc<Mutex<Option<String>>>,
    show: impl Fn() + Send + Sync + 'static,
) -> impl Fn(MenuEvent) + Send + Sync + 'static {
    move |ev: MenuEvent| {
        if *ev.id() == show_id {
            show();
        } else if *ev.id() == quit_id {
            std::process::exit(0);
        } else if *ev.id() == update_id {
            if let Some(u) = url.lock().unwrap_or_else(|e| e.into_inner()).clone() {
                let _ = webbrowser::open(&u);
            }
        }
    }
}

/// Windows: hilo propio con bomba de mensajes win32 (los handlers corren en
/// DispatchMessage) y un WM_TIMER por segundo que refresca icono y tooltip.
#[cfg(windows)]
pub fn start(shared: SharedState) {
    use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, SetTimer, TranslateMessage, MSG, WM_TIMER,
    };

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
            let Some(mut handle) = TrayHandle::build(&shared) else { return };
            MenuEvent::set_event_handler(Some(menu_handler(
                handle.show_id.clone(),
                handle.quit_id.clone(),
                handle.update_id.clone(),
                handle.url_slot(),
                crate::singleton::request_show,
            )));
            unsafe {
                let _ = SetTimer(None, 0, 1000, None);
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    if msg.message == WM_TIMER {
                        handle.refresh(&shared);
                        continue;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        })
        .expect("hilo tray");
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
    let Some(handle) = TrayHandle::build(&shared) else { return };
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
    fn el_tooltip_no_pasa_de_120_caracteres() {
        let long = "x".repeat(200);
        let mut t = snap(4, Mode::Pointer, &[&long, &long, &long, &long]);
        t.update = Some(Version([1, 6, 0]));
        let t = tooltip(&t);
        assert_eq!(t.chars().count(), 120);
        assert!(t.ends_with('…'));
    }
}
