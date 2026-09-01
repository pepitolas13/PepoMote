//! Icono de bandeja (Windows): cerrar la ventana la esconde aquí.
#![cfg(windows)]

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

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

            let _tray = match TrayIconBuilder::new()
                .with_icon(icon)
                .with_menu(Box::new(menu))
                .with_tooltip("PepoMote")
                .build()
            {
                Ok(t) => t,
                Err(_) => return,
            };

            let show_window = |ctx: &egui::Context| {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.request_repaint();
            };

            // Bomba de mensajes win32: los eventos del tray llegan por los
            // canales de la crate durante DispatchMessage.
            unsafe {
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);

                    while let Ok(ev) = MenuEvent::receiver().try_recv() {
                        if ev.id == show.id() {
                            show_window(&ctx);
                        } else if ev.id == quit.id() {
                            std::process::exit(0);
                        }
                    }
                    while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                        if let TrayIconEvent::DoubleClick { .. } = ev {
                            show_window(&ctx);
                        }
                    }
                }
            }
        })
        .expect("hilo tray");
}
