//! Instancia única: con cerrar-a-bandeja, lanzar el exe otra vez debe
//! MOSTRAR la instancia viva, no crear una segunda peleando por los puertos.
//!
//! Mecanismo: un socket UDP en loopback como cerrojo. Si el bind falla, ya
//! hay una instancia — se le manda "PMPSHOW1" y este proceso muere en paz.

use std::net::UdpSocket;
use std::sync::OnceLock;

const SINGLETON_PORT: u16 = 26762;
const SHOW: &[u8] = b"PMPSHOW1";

static UI_CTX: OnceLock<egui::Context> = OnceLock::new();

/// La UI registra su contexto en cuanto existe.
pub fn set_ctx(ctx: egui::Context) {
    let _ = UI_CTX.set(ctx);
}

/// Contexto de la UI, si ya existe (para comandos de ventana desde hilos).
pub fn ui_ctx() -> Option<egui::Context> {
    UI_CTX.get().cloned()
}

/// Restaura y trae al frente la ventana principal. Con la ventana minimizada
/// u oculta, el bucle de eventos de eframe DUERME y los comandos de viewport
/// se quedan encolados: hace falta el empujón NATIVO (SW_RESTORE genera
/// mensajes reales que despiertan el bucle, y entonces los comandos entran).
pub fn show_main_window(ctx: &egui::Context) {
    #[cfg(windows)]
    unsafe {
        use windows::core::{w, PCWSTR};
        use windows::Win32::UI::WindowsAndMessaging::{
            FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
        };
        if let Ok(h) = FindWindowW(PCWSTR::null(), w!("PepoMote")) {
            let _ = ShowWindow(h, SW_RESTORE);
            let _ = SetForegroundWindow(h);
        }
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

pub enum Singleton {
    /// Somos la primera instancia; el socket es el cerrojo (mantener vivo).
    Primary(UdpSocket),
    /// Ya hay otra: se le ha pedido que se muestre. Salir.
    AlreadyRunning,
}

pub fn acquire() -> Singleton {
    match UdpSocket::bind(("127.0.0.1", SINGLETON_PORT)) {
        Ok(sock) => Singleton::Primary(sock),
        Err(_) => {
            if let Ok(s) = UdpSocket::bind(("127.0.0.1", 0)) {
                let _ = s.send_to(SHOW, ("127.0.0.1", SINGLETON_PORT));
            }
            Singleton::AlreadyRunning
        }
    }
}

/// Hilo del cerrojo: cada "PMPSHOW1" restaura la ventana.
pub fn watch(sock: UdpSocket) {
    std::thread::Builder::new()
        .name("pmp-singleton".into())
        .spawn(move || {
            let mut buf = [0u8; 16];
            loop {
                let Ok((len, _)) = sock.recv_from(&mut buf) else {
                    continue;
                };
                if &buf[..len] == SHOW {
                    if let Some(ctx) = UI_CTX.get() {
                        show_main_window(ctx);
                    }
                }
            }
        })
        .expect("hilo singleton");
}
