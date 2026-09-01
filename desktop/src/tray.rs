//! Icono de bandeja (Windows): cerrar la ventana la esconde aquí.
#![cfg(windows)]

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

fn show_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

pub fn start(ctx_rx: std::sync::mpsc::Receiver<egui::Context>) {
    std::thread::Builder::new()
        .name("pmp-tray".into())
        .spawn(move || {
            // Espera al contexto de egui (la UI ya creada)
            let Ok(ctx) = ctx_rx.recv() else { return };

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

            // Handlers DIRECTOS (nada de drenar canales tras la bomba: el
            // evento del click podía quedarse encolado hasta el siguiente
            // mensaje y el menú parecía muerto).
            let show_id = show.id().clone();
            let quit_id = quit.id().clone();
            let ctx_menu = ctx.clone();
            MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
                if *ev.id() == show_id {
                    show_window(&ctx_menu);
                } else if *ev.id() == quit_id {
                    std::process::exit(0);
                }
            }));

            let ctx_tray = ctx.clone();
            TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
                match ev {
                    TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    }
                    | TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => show_window(&ctx_tray),
                    _ => {}
                }
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

            // Bomba de mensajes win32: los handlers se invocan durante
            // DispatchMessage.
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
