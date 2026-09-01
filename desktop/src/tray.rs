//! Icono de bandeja (Windows). No depende de la UI: con --minimized existe
//! desde el arranque aunque la ventana ni se haya creado, y "Mostrar" pide
//! crearla/restaurarla vía singleton::request_show.
#![cfg(windows)]

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

pub fn start() {
    std::thread::Builder::new()
        .name("pmp-tray".into())
        .spawn(move || {
            let icon = match Icon::from_rgba(crate::icon::logo_rgba(32), 32, 32) {
                Ok(i) => i,
                Err(_) => return,
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

            let _tray = match TrayIconBuilder::new()
                .with_icon(icon)
                .with_menu(Box::new(menu))
                .with_tooltip("PepoMote")
                .build()
            {
                Ok(t) => t,
                Err(_) => return,
            };

            // Bomba de mensajes win32: los handlers corren en DispatchMessage
            unsafe {
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        })
        .expect("hilo tray");
}
