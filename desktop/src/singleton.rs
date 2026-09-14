//! Instancia única + petición de "mostrar la ventana".
//!
//! Con --minimized la ventana NI SE CREA al arrancar (solo red + bandeja):
//! el primer "mostrar" (bandeja o relanzar el exe) desbloquea su creación.
//! Con la UI ya viva, "mostrar" restaura por WinAPI + comandos de viewport.
//!
//! El cerrojo es un socket UDP en 127.0.0.1:26762. La segunda copia manda
//! `PMPSHOW1` y solo se retira si la primera contesta `PMPACK01`: un puerto
//! ocupado por otra cosa, un cerrojo que no responde o un `bind` que falla
//! por otro motivo (sandbox, red rara) ya no dejan al usuario sin ventana.

use crate::state::LockTolerant;
use std::net::UdpSocket;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const SHOW: &[u8] = b"PMPSHOW1";
const ACK: &[u8] = b"PMPACK01";
/// Cuánto espera la segunda copia el acuse de la primera.
const ACK_TIMEOUT: Duration = Duration::from_millis(600);

/// Cerrojo = puerto PMP + 1 (26762 por defecto; sigue a PEPOMOTE_PORT).
pub fn port() -> u16 {
    crate::pairing::port().wrapping_add(1)
}

static UI_CTX: OnceLock<egui::Context> = OnceLock::new();
static SHOW_SIGNAL: Mutex<Option<Sender<()>>> = Mutex::new(None);

/// La UI registra su contexto en cuanto existe.
pub fn set_ctx(ctx: egui::Context) {
    let _ = UI_CTX.set(ctx);
}

/// Canal que desbloquea la CREACIÓN de la ventana (arranque --minimized).
pub fn set_show_signal(tx: Sender<()>) {
    *SHOW_SIGNAL.lock_tolerant() = Some(tx);
}

/// Muestra la ventana: si aún no existe, desbloquea su creación; si existe,
/// la restaura. Con la ventana minimizada u oculta el bucle de eframe DUERME
/// y los comandos de viewport se encolan: el empujón NATIVO (SW_RESTORE)
/// genera mensajes reales que lo despiertan, y entonces los comandos entran.
pub fn request_show() {
    if let Some(ctx) = UI_CTX.get() {
        // macOS: por AppKit en la cola principal (vale con la ventana oculta
        // o minimizada, donde egui no repinta y sus comandos no llegarían)
        #[cfg(target_os = "macos")]
        crate::macos::on_main(crate::macos::show_windows);
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
        // Linux: GNOME y KWin ignoran una petición de foco sin token de
        // activación; pedir atención (xdg_activation en Wayland, urgencia en
        // X11) hace que la barra avise aunque la ventana no suba sola
        #[cfg(target_os = "linux")]
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
            egui::UserAttentionType::Critical,
        ));
        ctx.request_repaint();
    } else if let Some(tx) = SHOW_SIGNAL.lock_tolerant().as_ref() {
        let _ = tx.send(());
    }
}

pub enum Singleton {
    /// Somos la primera instancia; el socket es el cerrojo (mantener vivo).
    Primary(UdpSocket),
    /// Hay otro PepoMote vivo y ha contestado: ya se le ha pedido que se
    /// muestre. Salir.
    AlreadyRunning,
    /// El cerrojo no se pudo coger y nadie contesta al otro lado (el puerto
    /// lo tiene otra cosa, o `bind` falla por otro motivo): seguir sin
    /// cerrojo, que es mejor que no arrancar.
    NoLock(std::io::Error),
}

pub fn acquire() -> Singleton {
    acquire_on(port())
}

pub fn acquire_on(port: u16) -> Singleton {
    match UdpSocket::bind(("127.0.0.1", port)) {
        Ok(sock) => Singleton::Primary(sock),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if ping(port, ACK_TIMEOUT) {
                Singleton::AlreadyRunning
            } else {
                Singleton::NoLock(e)
            }
        }
        Err(e) => Singleton::NoLock(e),
    }
}

/// Manda `PMPSHOW1` al cerrojo y espera el acuse. true = hay un PepoMote
/// vivo al otro lado (y ya sabe que tiene que mostrarse).
fn ping(port: u16, timeout: Duration) -> bool {
    let Ok(s) = UdpSocket::bind(("127.0.0.1", 0)) else {
        return false;
    };
    if s.set_read_timeout(Some(timeout)).is_err() || s.send_to(SHOW, ("127.0.0.1", port)).is_err() {
        return false;
    }
    let mut buf = [0u8; 16];
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match s.recv_from(&mut buf) {
            Ok((len, from)) if from.port() == port && &buf[..len] == ACK => return true,
            Ok(_) => continue,
            Err(_) => return false,
        }
    }
    false
}

/// Hilo del cerrojo: cada "PMPSHOW1" recibe su "PMPACK01" y pide mostrar la
/// ventana.
pub fn watch(sock: UdpSocket) {
    crate::threads::spawn_guarded(
        "pmp-singleton",
        crate::threads::OnPanic::Restart { after: Duration::from_secs(1), max: 10 },
        move || {
            let mut buf = [0u8; 16];
            loop {
                match sock.recv_from(&mut buf) {
                    Ok((len, from)) => {
                        if &buf[..len] == SHOW {
                            let _ = sock.send_to(ACK, from);
                            crate::log_line!("Otra copia de PepoMote pide mostrar la ventana");
                            request_show();
                        }
                    }
                    // Socket en error (en Windows, un ICMP de un destino que
                    // se fue): no girar en bucle a tope
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        },
    )
    .expect("hilo singleton");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free_port() -> u16 {
        UdpSocket::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port()
    }

    #[test]
    fn puerto_libre_somos_la_primera() {
        assert!(matches!(acquire_on(free_port()), Singleton::Primary(_)));
    }

    #[test]
    fn otra_instancia_viva_contesta_y_esta_se_retira() {
        let p = free_port();
        let Singleton::Primary(lock) = acquire_on(p) else {
            panic!("el puerto estaba libre");
        };
        watch(lock);
        let t = Instant::now();
        assert!(matches!(acquire_on(p), Singleton::AlreadyRunning));
        assert!(t.elapsed() < ACK_TIMEOUT, "contesta al instante, no por timeout");
    }

    #[test]
    fn puerto_ocupado_por_un_socket_mudo_no_es_otra_instancia() {
        let mute = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let p = mute.local_addr().unwrap().port();
        let t = Instant::now();
        assert!(matches!(acquire_on(p), Singleton::NoLock(_)));
        assert!(t.elapsed() < Duration::from_secs(3));
    }
}
