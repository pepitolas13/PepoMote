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
//! `PMPDIAG1` (lo manda `--diag`) devuelve en una línea cómo está la
//! ventana del receptor abierto: último fotograma y en qué paso se quedó.
//!
//! La segunda copia pide con `PMPSHOW2`, y la primera, si se colgó antes de
//! pintar con OpenGL (Windows, ver `gpu::handover_allowed`), contesta
//! `PMPHUNG1`, quita su icono y se termina: la nueva sigue como principal
//! con Direct3D. Un PepoMote de antes (hasta la 1.13.2) no conoce
//! `PMPSHOW2` (no contesta) y se le pregunta con el `PMPSHOW1` de siempre,
//! que solo recibe `PMPACK01`.

use crate::state::LockTolerant;
use std::net::UdpSocket;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const SHOW: &[u8] = b"PMPSHOW1";
/// Mostrar, de una copia que entiende `PMPHUNG1`.
const SHOW2: &[u8] = b"PMPSHOW2";
const ACK: &[u8] = b"PMPACK01";
/// «Me he colgado antes de pintar: te dejo el sitio» (solo a `PMPSHOW2`).
const HUNG: &[u8] = b"PMPHUNG1";
/// Lo que espera la copia nueva a que la colgada suelte el cerrojo (quitar
/// su icono de la bandeja le lleva hasta 1,5 s).
const HANDOVER_WAIT: Duration = Duration::from_secs(5);
/// `--diag` pregunta por la ventana del receptor abierto; la respuesta es
/// una línea de texto (versión, último fotograma, último paso).
const DIAG: &[u8] = b"PMPDIAG1";
/// Cuánto espera la segunda copia el acuse de la primera.
const ACK_TIMEOUT: Duration = Duration::from_millis(600);

/// Cerrojo = puerto PMP + 1 (26762 por defecto; sigue a PEPOMOTE_PORT).
pub fn port() -> u16 {
    crate::pairing::port().wrapping_add(1)
}

static UI_CTX: OnceLock<egui::Context> = OnceLock::new();
static SHOW_SIGNAL: Mutex<Option<Sender<()>>> = Mutex::new(None);
/// Windows: el HWND de NUESTRA ventana, guardado al crearla. Antes se
/// buscaba con `FindWindowW(null, "PepoMote")`, que devuelve la primera
/// ventana de CUALQUIER programa con ese título: en Windows 10 el
/// Explorador abierto en una carpeta llamada «PepoMote» (donde mucha gente
/// guarda el exe) se titula así y se llevaba el empujón, y la ventana
/// escondida en la bandeja no volvía nunca.
#[cfg(windows)]
static UI_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// La UI registra su contexto en cuanto existe.
pub fn set_ctx(ctx: egui::Context) {
    let _ = UI_CTX.set(ctx);
}

/// Windows: la UI registra su ventana (el HWND que le ha dado winit) al
/// crearse, junto al contexto.
#[cfg(windows)]
pub fn remember_window(cc: &eframe::CreationContext<'_>) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(handle) = cc.window_handle() {
        if let RawWindowHandle::Win32(w) = handle.as_raw() {
            UI_HWND.store(w.hwnd.get(), std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// Qué `ShowWindow` hacen falta para tener delante una ventana: escondida
/// (la X la esconde en la bandeja) → `SW_SHOW`; minimizada → `SW_RESTORE`.
/// Una ventana a la vista y sin minimizar no se toca: `SW_RESTORE` la
/// desmaximizaría.
#[cfg(any(windows, test))]
pub fn show_steps(visible: bool, minimized: bool) -> (bool, bool) {
    (!visible, minimized)
}

/// Windows: nuestra ventana, solo si el HWND guardado sigue vivo y es de
/// este proceso (nunca la de otro programa).
#[cfg(windows)]
fn own_window() -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};
    let raw = UI_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if raw == 0 {
        return None;
    }
    let hwnd = HWND(raw as *mut core::ffi::c_void);
    unsafe {
        if !IsWindow(hwnd).as_bool() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (pid == GetCurrentProcessId()).then_some(hwnd)
    }
}

/// Canal que desbloquea la CREACIÓN de la ventana (arranque --minimized).
pub fn set_show_signal(tx: Sender<()>) {
    *SHOW_SIGNAL.lock_tolerant() = Some(tx);
}

/// Muestra la ventana: si aún no existe, desbloquea su creación; si existe,
/// la restaura. Con la ventana minimizada u oculta el bucle de eframe DUERME
/// y los comandos de viewport se encolan: el empujón NATIVO (ShowWindow en
/// NUESTRO HWND) genera mensajes reales que lo despiertan, y entonces los
/// comandos entran (`Visible(true)` el primero: resincroniza la bandera de
/// visibilidad que winit guarda aparte).
pub fn request_show() {
    crate::launch::set_window_hidden(false);
    if let Some(ctx) = UI_CTX.get() {
        // macOS: por AppKit en la cola principal (vale con la ventana oculta
        // o minimizada, donde egui no repinta y sus comandos no llegarían)
        #[cfg(target_os = "macos")]
        crate::macos::on_main(crate::macos::show_windows);
        // `ShowWindowAsync`: desde otro hilo, `ShowWindow` espera a que el de
        // la ventana conteste, y con él colgado se quedarían colgados también
        // la bandeja o el cerrojo (que es quien contesta a `--diag`)
        #[cfg(windows)]
        if let Some(h) = own_window() {
            use windows::Win32::UI::WindowsAndMessaging::{
                IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindowAsync, SW_RESTORE, SW_SHOW,
            };
            unsafe {
                let (show, restore) = show_steps(IsWindowVisible(h).as_bool(), IsIconic(h).as_bool());
                if show {
                    let _ = ShowWindowAsync(h, SW_SHOW);
                }
                if restore {
                    let _ = ShowWindowAsync(h, SW_RESTORE);
                }
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
    /// La que había se colgó antes de pintar y nos ha dejado el sitio:
    /// somos la principal (y la ventana va con Direct3D).
    TookOver(UdpSocket),
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
            // Windows solo deja ponerse delante a quien tiene el primer plano:
            // esta copia, recién abierta por el usuario, se lo cede a la otra
            // antes de pedirle que se muestre
            #[cfg(windows)]
            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
                let _ = AllowSetForegroundWindow(ASFW_ANY);
            }
            match ask(port, SHOW2, ACK_TIMEOUT).as_deref() {
                Some(r) if r == ACK => Singleton::AlreadyRunning,
                Some(r) if r == HUNG => match wait_free(port, HANDOVER_WAIT) {
                    Some(sock) => Singleton::TookOver(sock),
                    None => Singleton::NoLock(e),
                },
                // Un PepoMote anterior no conoce PMPSHOW2: el de siempre
                _ => {
                    if ping(port, ACK_TIMEOUT) {
                        Singleton::AlreadyRunning
                    } else {
                        Singleton::NoLock(e)
                    }
                }
            }
        }
        Err(e) => Singleton::NoLock(e),
    }
}

/// Espera a que la copia que dejó el sitio suelte el cerrojo y lo coge.
fn wait_free(port: u16, limit: Duration) -> Option<UdpSocket> {
    let start = Instant::now();
    loop {
        match UdpSocket::bind(("127.0.0.1", port)) {
            Ok(sock) => return Some(sock),
            Err(_) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => return None,
        }
    }
}

/// Manda `msg` al cerrojo y devuelve lo primero que conteste desde ese
/// puerto antes de `timeout`.
fn ask(port: u16, msg: &[u8], timeout: Duration) -> Option<Vec<u8>> {
    let s = UdpSocket::bind(("127.0.0.1", 0)).ok()?;
    s.set_read_timeout(Some(timeout)).ok()?;
    s.send_to(msg, ("127.0.0.1", port)).ok()?;
    let mut buf = [0u8; 512];
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match s.recv_from(&mut buf) {
            Ok((len, from)) if from.port() == port => return Some(buf[..len].to_vec()),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
    None
}

/// Manda `PMPSHOW1` al cerrojo y espera el acuse. true = hay un PepoMote
/// vivo al otro lado (y ya sabe que tiene que mostrarse).
fn ping(port: u16, timeout: Duration) -> bool {
    ask(port, SHOW, timeout).is_some_and(|reply| reply == ACK)
}

/// `--diag`: cómo está la ventana del receptor abierto, en sus palabras.
/// None = nadie contesta en el cerrojo (no hay receptor, o no responde).
pub fn query_status(port: u16, timeout: Duration) -> Option<String> {
    ask(port, DIAG, timeout).map(|reply| String::from_utf8_lossy(&reply).into_owned())
}

/// La respuesta a `PMPDIAG1`: versión y estado de la ventana. La escribe
/// el hilo del cerrojo, que sigue vivo aunque el de la ventana se cuelgue.
fn status_reply() -> String {
    format!("PepoMote {} · {}", env!("CARGO_PKG_VERSION"), crate::launch::window_status())
}

/// Hilo del cerrojo: cada "PMPSHOW1" recibe su "PMPACK01" y pide mostrar la
/// ventana (dejando en el log cómo estaba: si el usuario reabre el exe
/// porque la ve negra, ahí queda el último fotograma y el último paso);
/// cada "PMPDIAG1" recibe ese mismo estado en texto.
pub fn watch(sock: UdpSocket) {
    crate::threads::spawn_guarded(
        "pmp-singleton",
        crate::threads::OnPanic::Restart { after: Duration::from_secs(1), max: 10 },
        move || {
            let mut buf = [0u8; 16];
            loop {
                match sock.recv_from(&mut buf) {
                    Ok((len, from)) => {
                        if &buf[..len] == SHOW || &buf[..len] == SHOW2 {
                            // Colgada antes de pintar con OpenGL: a la copia
                            // nueva (solo si entiende PMPHUNG1) se le deja el
                            // sitio, en vez de pedirle que se vaya
                            #[cfg(windows)]
                            if &buf[..len] == SHOW2 && crate::gpu::handover_now() {
                                let _ = sock.send_to(HUNG, from);
                                crate::log_line!(
                                    "Otra copia de PepoMote pide la ventana y esta no ha pintado con OpenGL en {} s: le dejo el sitio · {}",
                                    crate::gpu::attempt_age().map(|a| a.as_secs()).unwrap_or(0),
                                    crate::launch::window_status()
                                );
                                crate::launch::before_exit();
                                unsafe {
                                    use windows::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
                                    let _ = TerminateProcess(GetCurrentProcess(), 7);
                                }
                            }
                            let _ = sock.send_to(ACK, from);
                            crate::log_line!(
                                "Otra copia de PepoMote pide mostrar la ventana · {}",
                                crate::launch::window_status()
                            );
                            request_show();
                        } else if &buf[..len] == DIAG {
                            let _ = sock.send_to(status_reply().as_bytes(), from);
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
    fn el_diag_pregunta_y_la_primera_contesta_con_su_ventana() {
        let p = free_port();
        let Singleton::Primary(lock) = acquire_on(p) else {
            panic!("el puerto estaba libre");
        };
        watch(lock);
        let reply = query_status(p, ACK_TIMEOUT).expect("la primera contesta");
        assert!(reply.starts_with(&format!("PepoMote {} · ", env!("CARGO_PKG_VERSION"))), "{reply}");
        assert!(reply.contains("fotograma"), "{reply}");
        let mute = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        assert!(query_status(mute.local_addr().unwrap().port(), Duration::from_millis(100)).is_none());
    }

    #[test]
    fn una_copia_colgada_deja_el_sitio_a_la_nueva() {
        let p = free_port();
        let hung = UdpSocket::bind(("127.0.0.1", p)).unwrap();
        let t = std::thread::spawn(move || {
            let mut buf = [0u8; 16];
            let (len, from) = hung.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..len], SHOW2, "la copia nueva pregunta con PMPSHOW2");
            hung.send_to(HUNG, from).unwrap();
            // tarda un poco en terminarse (quitar el icono de la bandeja)
            std::thread::sleep(Duration::from_millis(400));
            drop(hung);
        });
        assert!(matches!(acquire_on(p), Singleton::TookOver(_)), "la nueva coge el cerrojo en cuanto se suelta");
        t.join().unwrap();
    }

    #[test]
    fn a_un_receptor_anterior_se_le_pide_como_siempre() {
        // Hasta la 1.13.2: PMPSHOW2 no le dice nada, PMPSHOW1 sí
        let p = free_port();
        let old = UdpSocket::bind(("127.0.0.1", p)).unwrap();
        let t = std::thread::spawn(move || {
            let mut buf = [0u8; 16];
            loop {
                let (len, from) = old.recv_from(&mut buf).unwrap();
                if &buf[..len] == SHOW {
                    old.send_to(ACK, from).unwrap();
                    return;
                }
            }
        });
        assert!(matches!(acquire_on(p), Singleton::AlreadyRunning));
        t.join().unwrap();
    }

    #[test]
    fn mostrar_solo_hace_lo_que_hace_falta() {
        // escondida en la bandeja (la X): enseñarla
        assert_eq!(show_steps(false, false), (true, false));
        // minimizada: restaurarla
        assert_eq!(show_steps(true, true), (false, true));
        // escondida estando minimizada: las dos cosas
        assert_eq!(show_steps(false, true), (true, true));
        // a la vista: ni se toca (SW_RESTORE la desmaximizaría)
        assert_eq!(show_steps(true, false), (false, false));
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
