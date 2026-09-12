//! Icono de bandeja (Windows). No depende de la UI: con --minimized existe
//! desde el arranque aunque la ventana ni se haya creado, y "Mostrar" pide
//! crearla/restaurarla vía singleton::request_show. El icono y su tooltip
//! cuentan el estado sin abrir nada: punto verde con móviles conectados,
//! cuántos son, en qué modo y quiénes.
#![cfg(windows)]

use crate::state::{Mode, SharedState};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, SetTimer, TranslateMessage, MSG, WM_TIMER,
};

/// Lo que la bandeja enseña; solo se toca cuando cambia.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraySnapshot {
    pub players: usize,
    pub mode: Mode,
    /// Nombres (modelo, o nombre si no hay) de los móviles conectados, por slot.
    pub names: Vec<String>,
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
        Self { players: s.player_count(), mode: s.mode, names }
    }
}

/// Texto del tooltip. Windows lo corta a 128 caracteres: nos quedamos en 120.
pub fn tooltip(snap: &TraySnapshot) -> String {
    if snap.players == 0 {
        return "PepoMote · esperando al móvil".to_owned();
    }
    let mode = match snap.mode {
        Mode::Pointer => "Puntero",
        Mode::Dolphin => "Dolphin",
        Mode::Cemu => "Wii U",
    };
    let plural = if snap.players == 1 { "móvil" } else { "móviles" };
    let mut text = format!("PepoMote · {} {plural} · {mode}", snap.players);
    let names = snap.names.join(", ");
    if !names.is_empty() {
        text.push('\n');
        text.push_str(&names);
    }
    if text.chars().count() > 120 {
        text = text.chars().take(119).collect::<String>() + "…";
    }
    text
}

pub fn start(shared: SharedState) {
    std::thread::Builder::new()
        .name("pmp-tray".into())
        .spawn(move || {
            let icon_of = |rgba: Vec<u8>| Icon::from_rgba(rgba, 32, 32).ok();
            let (Some(idle), Some(live)) =
                (icon_of(crate::icon::logo_rgba(32)), icon_of(crate::icon::logo_rgba_badge(32)))
            else {
                return;
            };
            let menu = Menu::new();
            let show = MenuItem::new("Mostrar PepoMote", true, None);
            let quit = MenuItem::new("Salir", true, None);
            let _ = menu.append(&show);
            let _ = menu.append(&PredefinedMenuItem::separator());
            let _ = menu.append(&quit);

            // Handlers directos (los canales drenados a mano perdían clicks)
            let show_id = show.id().clone();
            let quit_id = quit.id().clone();
            MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
                if *ev.id() == show_id {
                    crate::singleton::request_show();
                } else if *ev.id() == quit_id {
                    std::process::exit(0);
                }
            }));

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

            let mut snap = TraySnapshot::of(&shared);
            let tray = match TrayIconBuilder::new()
                .with_icon(if snap.players > 0 { live.clone() } else { idle.clone() })
                .with_menu(Box::new(menu))
                // click izquierdo = abrir la ventana (nuestro handler);
                // el menú solo con click derecho — sin esto la librería
                // ademas abría el menú con el izquierdo, descolocado
                .with_menu_on_left_click(false)
                .with_tooltip(tooltip(&snap))
                .build()
            {
                Ok(t) => t,
                Err(_) => return,
            };

            // Bomba de mensajes win32: los handlers corren en DispatchMessage.
            // Un WM_TIMER por segundo (sin ventana llega a la cola del hilo)
            // refresca icono y tooltip cuando el estado cambia.
            unsafe {
                let _ = SetTimer(None, 0, 1000, None);
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    if msg.message == WM_TIMER {
                        let now = TraySnapshot::of(&shared);
                        if now != snap {
                            let _ = tray.set_icon(Some(if now.players > 0 { live.clone() } else { idle.clone() }));
                            let _ = tray.set_tooltip(Some(tooltip(&now)));
                            snap = now;
                        }
                        continue;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        })
        .expect("hilo tray");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(players: usize, mode: Mode, names: &[&str]) -> TraySnapshot {
        TraySnapshot { players, mode, names: names.iter().map(|s| s.to_string()).collect() }
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
    fn el_tooltip_no_pasa_de_120_caracteres() {
        let long = "x".repeat(200);
        let t = tooltip(&snap(4, Mode::Pointer, &[&long, &long, &long, &long]));
        assert_eq!(t.chars().count(), 120);
        assert!(t.ends_with('…'));
    }
}
