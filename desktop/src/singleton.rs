//! Instancia única + petición de "mostrar la ventana".
//!
//! Con --minimized la ventana NI SE CREA al arrancar (solo red + bandeja):
//! el primer "mostrar" (bandeja o relanzar el exe) desbloquea su creación.
//! Con la UI ya viva, "mostrar" restaura por WinAPI + comandos de viewport.

use std::net::UdpSocket;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

const SINGLETON_PORT: u16 = 26762;
const SHOW: &[u8] = b"PMPSHOW1";

static UI_CTX: OnceLock<egui::Context> = OnceLock::new();
static SHOW_SIGNAL: Mutex<Option<Sender<()>>> = Mutex::new(None);

/// La UI registra su contexto en cuanto existe.
pub fn set_ctx(ctx: egui::Context) {
    let _ = UI_CTX.set(ctx);
}

/// Canal que desbloquea la CREACIÓN de la ventana (arranque --minimized).
pub fn set_show_signal(tx: Sender<()>) {
    *SHOW_SIGNAL.lock().unwrap() = Some(tx);
}

/// Muestra la ventana: si aún no existe, desbloquea su creación; si existe,
/// la restaura. Con la ventana minimizada u oculta el bucle de eframe DUERME
/// y los comandos de viewport se encolan: el empujón NATIVO (SW_RESTORE)
/// genera mensajes reales que lo despiertan, y entonces los comandos entran.
pub fn request_show() {
    if let Some(ctx) = UI_CTX.get() {
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
    } else if let Some(tx) = SHOW_SIGNAL.lock().unwrap().as_ref() {
        let _ = tx.send(());
    }
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

/// Hilo del cerrojo: cada "PMPSHOW1" pide mostrar la ventana.
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
                    request_show();
                }
            }
        })
        .expect("hilo singleton");
}
