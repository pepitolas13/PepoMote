//! Puertos ocupados: si el DSU (26760) o el canal del móvil (26761) ya los
//! tiene otro proceso, PepoMote no funciona y nadie sabe por qué. Aquí se
//! averigua QUIÉN los tiene (otro servidor DSU como DS4Windows o BetterJoy,
//! o un PepoMote antiguo que se quedó colgado), se cierra ese proceso y se
//! vuelve a abrir el puerto. Se deja dicho en la ventana y en los móviles.
//!
//! Un puerto en manos de un proceso que YA NO EXISTE (el receptor que acaba
//! de cerrarse en una actualización en caliente, o al cerrar y abrir deprisa)
//! tarda un instante en soltarse: se espera y se reintenta en vez de darlo
//! por perdido, y los hilos del móvil insisten hasta conseguirlo.
//!
//! Windows: tablas UDP/TCP con PID (iphlpapi) + TerminateProcess.
//! Linux: /proc/net/{udp,tcp} → inodo → /proc/*/fd → PID + `kill`.

use crate::state::LockTolerant;
use crate::state::SharedState;
use std::net::{TcpListener, UdpSocket};
use std::time::Duration;
use crate::tr;

/// Proceso que tiene abierto un puerto local: (pid, nombre).
#[derive(Clone, Debug, PartialEq)]
pub struct Owner {
    pub pid: u32,
    pub name: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Proto {
    Udp,
    Tcp,
}

/// Cuánto se espera a que el proceso desalojado muera y suelte el puerto.
const EVICT_WAIT: Duration = Duration::from_millis(2500);
/// Puerto ocupado por un proceso que ya no existe, o cuyo dueño no se sabe:
/// el sistema tarda un instante en soltar el endpoint tras morir su dueño.
/// La actualización en caliente arranca el receptor nuevo menos de un segundo
/// después de cerrar el viejo, y en Windows el UDP del móvil (dos handles y un
/// `recv` pendiente) aún seguía cogido: se reintenta hasta este tope antes de
/// darlo por perdido.
const LINGER_WAIT: Duration = Duration::from_millis(3000);
/// Cada cuánto se reintenta el bind mientras se espera.
const RETRY_EVERY: Duration = Duration::from_millis(100);
/// Los hilos que viven del puerto del móvil (control TCP y telemetría UDP)
/// insisten cada tanto si ni con la espera se consiguió.
pub const BIND_RETRY: Duration = Duration::from_secs(2);

/// Cómo se consiguió el puerto.
#[derive(Clone, Debug, PartialEq)]
pub enum Recovered {
    /// A la primera.
    AtOnce,
    /// Se cerró al proceso que lo tenía y quedó libre.
    Evicted(Owner),
    /// Estaba ocupado por un proceso ya muerto (o sin identificar, o que no se
    /// pudo cerrar) y quedó libre solo tras esperar `waited`.
    Lingered { owner: Option<Owner>, waited: Duration },
}

/// Por qué no se consiguió.
#[derive(Debug)]
pub enum Failure {
    /// Un error que no es «puerto ocupado» (sin red, sandbox…): no se insiste.
    CannotListen(std::io::Error),
    /// El puerto ya está abierto en este mismo proceso: no va a soltarse.
    SameProcess,
    /// Ocupado, sin dueño identificable, y no se soltó en [`LINGER_WAIT`].
    BusyUnknown,
    /// Ocupado por ese proceso, que no se pudo cerrar (o cerrado no lo soltó).
    BusyKnown(Owner),
}

/// Abre un socket UDP; si el puerto está ocupado, desaloja al dueño o espera
/// a que se suelte, y reintenta.
pub fn bind_udp(shared: &SharedState, addr: &str, port: u16, what: &str) -> Result<UdpSocket, String> {
    bind_evicting(shared, port, Proto::Udp, what, || UdpSocket::bind((addr, port)))
}

/// Abre un listener TCP; si el puerto está ocupado, desaloja al dueño o espera
/// a que se suelte, y reintenta.
pub fn bind_tcp(shared: &SharedState, addr: &str, port: u16, what: &str) -> Result<TcpListener, String> {
    bind_evicting(shared, port, Proto::Tcp, what, || TcpListener::bind((addr, port)))
}

/// Núcleo puro de la apertura de un puerto: `bind` lo intenta, `owner` dice
/// quién lo tiene, `kill` lo cierra y `sleep` espera (en los tests ninguno
/// toca el sistema). Reglas:
/// - libre → a la primera;
/// - ocupado por este mismo proceso → error al instante (no va a soltarse);
/// - ocupado por otro proceso vivo → se le cierra y se reintenta hasta
///   [`EVICT_WAIT`];
/// - ocupado por un proceso que ya no existe (o que no se pudo cerrar), o sin
///   dueño conocido → se reintenta hasta [`LINGER_WAIT`]: es lo que pasa justo
///   después de reiniciar el receptor (actualización, cerrar y abrir deprisa).
pub fn bind_with_policy<T>(
    mut bind: impl FnMut() -> std::io::Result<T>,
    owner: impl FnOnce() -> Option<Owner>,
    kill: impl FnOnce(u32) -> bool,
    self_pid: u32,
    mut sleep: impl FnMut(Duration),
) -> Result<(T, Recovered), Failure> {
    let first = match bind() {
        Ok(s) => return Ok((s, Recovered::AtOnce)),
        Err(e) => e,
    };
    if first.kind() != std::io::ErrorKind::AddrInUse {
        return Err(Failure::CannotListen(first));
    }
    let owner = owner();
    if owner.as_ref().is_some_and(|o| o.pid == self_pid) {
        return Err(Failure::SameProcess);
    }
    let killed = owner.as_ref().is_some_and(|o| kill(o.pid));
    let limit = if killed { EVICT_WAIT } else { LINGER_WAIT };
    let mut waited = Duration::ZERO;
    while waited < limit {
        sleep(RETRY_EVERY);
        waited += RETRY_EVERY;
        if let Ok(s) = bind() {
            let how = match owner {
                Some(o) if killed => Recovered::Evicted(o),
                owner => Recovered::Lingered { owner, waited },
            };
            return Ok((s, how));
        }
    }
    Err(match owner {
        Some(o) => Failure::BusyKnown(o),
        None => Failure::BusyUnknown,
    })
}

/// [`bind_with_policy`] contra el sistema de verdad, con lo que hay que contar
/// al usuario: el desalojo va a la ventana, al log y a los móviles; la espera
/// por un puerto que se suelta solo, al log (para saber qué pasó al arrancar).
fn bind_evicting<T>(
    shared: &SharedState,
    port: u16,
    proto: Proto,
    what: &str,
    bind: impl FnMut() -> std::io::Result<T>,
) -> Result<T, String> {
    let name = proto_name(proto);
    match bind_with_policy(bind, || owner(port, proto), kill, std::process::id(), std::thread::sleep) {
        Ok((s, Recovered::AtOnce)) => Ok(s),
        Ok((s, Recovered::Evicted(o))) => {
            let msg = tr!("port.freed", name, port, o.name, o.pid);
            shared.lock_tolerant().port_notice = Some(msg.clone());
            crate::log_line!("{msg}");
            crate::net::notify_all(&msg);
            Ok(s)
        }
        Ok((s, Recovered::Lingered { owner, waited })) => {
            let who = match owner {
                Some(o) => format!("lo tenía {} (PID {}) y no se pudo cerrar", o.name, o.pid),
                None => "sin dueño identificable".to_owned(),
            };
            crate::log_line!(
                "Puerto {name} {port} ({what}): ocupado al arrancar ({who}); quedó libre solo a los {} ms",
                waited.as_millis()
            );
            Ok(s)
        }
        Err(Failure::CannotListen(e)) => Err(tr!("port.cannot_listen", what, name, port, e)),
        Err(Failure::SameProcess) => Err(tr!("port.same_process", what, name, port)),
        Err(Failure::BusyUnknown) => Err(tr!("port.busy_unknown", what, name, port)),
        Err(Failure::BusyKnown(o)) => Err(tr!("port.busy_known", what, name, port, o.name, o.pid)),
    }
}

/// Para los hilos que viven del puerto del móvil (control TCP y telemetría
/// UDP): si ni con la espera de [`bind_with_policy`] se consiguió, no se
/// rinden: lo vuelven a intentar cada [`BIND_RETRY`] con el error a la vista
/// en la ventana (y una sola vez en el log por texto distinto), y lo retiran
/// al abrirlo. Antes el hilo moría a la primera y el receptor se quedaba sin
/// puntero (con «Inyección: ninguna» en el pie) hasta reabrirlo.
pub fn bind_insisting<T>(shared: &SharedState, what: &str, bind: impl FnMut() -> Result<T, String>) -> T {
    bind_insisting_with(shared, what, bind, std::thread::sleep)
}

/// [`bind_insisting`] con la espera inyectada (los tests no duermen).
pub fn bind_insisting_with<T>(
    shared: &SharedState,
    what: &str,
    mut bind: impl FnMut() -> Result<T, String>,
    mut sleep: impl FnMut(Duration),
) -> T {
    let mut shown: Option<String> = None;
    loop {
        match bind() {
            Ok(s) => {
                if let Some(msg) = shown {
                    crate::log_line!("{what}: puerto conseguido tras insistir");
                    let mut st = shared.lock_tolerant();
                    if st.last_error.as_deref() == Some(msg.as_str()) {
                        st.last_error = None;
                    }
                }
                return s;
            }
            Err(e) => {
                let msg = format!("{e}; {}", tr!("port.will_retry"));
                if shown.as_deref() != Some(msg.as_str()) {
                    crate::log_line!("{msg}");
                    shown = Some(msg.clone());
                }
                shared.lock_tolerant().last_error = Some(msg);
                sleep(BIND_RETRY);
            }
        }
    }
}

fn proto_name(p: Proto) -> &'static str {
    match p {
        Proto::Udp => "UDP",
        Proto::Tcp => "TCP",
    }
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub fn owner(port: u16, proto: Proto) -> Option<Owner> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCPROW_OWNER_PID, MIB_UDPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
        UDP_TABLE_OWNER_PID,
    };
    const AF_INET: u32 = 2;
    unsafe {
        let mut size: u32 = 0;
        let pid = match proto {
            Proto::Udp => {
                let _ = GetExtendedUdpTable(None, &mut size, false, AF_INET, UDP_TABLE_OWNER_PID, 0);
                let mut buf = vec![0u8; size as usize + 64];
                if GetExtendedUdpTable(Some(buf.as_mut_ptr() as *mut _), &mut size, false, AF_INET, UDP_TABLE_OWNER_PID, 0) != 0 {
                    return None;
                }
                let n = u32::from_ne_bytes(buf[..4].try_into().ok()?) as usize;
                let rows = buf.as_ptr().add(4) as *const MIB_UDPROW_OWNER_PID;
                (0..n)
                    .map(|i| *rows.add(i))
                    .find(|r| u16::from_be((r.dwLocalPort & 0xffff) as u16) == port)
                    .map(|r| r.dwOwningPid)
            }
            Proto::Tcp => {
                let _ = GetExtendedTcpTable(None, &mut size, false, AF_INET, TCP_TABLE_OWNER_PID_ALL, 0);
                let mut buf = vec![0u8; size as usize + 64];
                if GetExtendedTcpTable(Some(buf.as_mut_ptr() as *mut _), &mut size, false, AF_INET, TCP_TABLE_OWNER_PID_ALL, 0) != 0 {
                    return None;
                }
                let n = u32::from_ne_bytes(buf[..4].try_into().ok()?) as usize;
                let rows = buf.as_ptr().add(4) as *const MIB_TCPROW_OWNER_PID;
                // MIB_TCP_STATE_LISTEN = 2
                (0..n)
                    .map(|i| *rows.add(i))
                    .find(|r| r.dwState == 2 && u16::from_be((r.dwLocalPort & 0xffff) as u16) == port)
                    .map(|r| r.dwOwningPid)
            }
        }?;
        if pid == 0 {
            return None;
        }
        Some(Owner { pid, name: process_name(pid).unwrap_or_else(|| "desconocido".to_owned()) })
    }
}

#[cfg(windows)]
fn process_name(pid: u32) -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(|s| s.to_owned())
    }
}

#[cfg(windows)]
pub fn kill(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) else {
            return false;
        };
        let ok = TerminateProcess(h, 1).is_ok();
        let _ = CloseHandle(h);
        ok
    }
}

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
pub fn owner(port: u16, proto: Proto) -> Option<Owner> {
    let table = std::fs::read_to_string(match proto {
        Proto::Udp => "/proc/net/udp",
        Proto::Tcp => "/proc/net/tcp",
    })
    .ok()?;
    let inode = linux_socket_inode(&table, port, proto)?;
    let pid = linux_pid_of_inode(inode)?;
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).map(|s| s.trim().to_owned()).unwrap_or_else(|_| "desconocido".to_owned());
    Some(Owner { pid, name })
}

/// Inodo del socket que escucha en `port` según /proc/net/{udp,tcp}
/// (columna local_address «IP:PUERTO» en hex; en TCP solo st = 0A, LISTEN).
#[cfg(any(target_os = "linux", test))]
fn linux_socket_inode(table: &str, port: u16, proto: Proto) -> Option<u64> {
    for line in table.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 10 {
            continue;
        }
        let local_port = cols[1].rsplit(':').next().and_then(|p| u16::from_str_radix(p, 16).ok());
        if local_port != Some(port) {
            continue;
        }
        if proto == Proto::Tcp && cols[3] != "0A" {
            continue;
        }
        if let Ok(inode) = cols[9].parse::<u64>() {
            return Some(inode);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn linux_pid_of_inode(inode: u64) -> Option<u32> {
    let want = format!("socket:[{inode}]");
    for e in std::fs::read_dir("/proc").ok()?.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
        let Ok(fds) = std::fs::read_dir(e.path().join("fd")) else { continue };
        for fd in fds.flatten() {
            if std::fs::read_link(fd.path()).map(|l| l.to_string_lossy() == want).unwrap_or(false) {
                return Some(pid);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub fn kill(pid: u32) -> bool {
    let term = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status().map(|s| s.success()).unwrap_or(false);
    if term {
        std::thread::sleep(Duration::from_millis(500));
        if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
            return true;
        }
    }
    std::process::Command::new("kill").args(["-KILL", &pid.to_string()]).status().map(|s| s.success()).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// macOS: lsof (solo cuando un puerto está ocupado, no en caliente)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub fn owner(port: u16, proto: Proto) -> Option<Owner> {
    let mut cmd = std::process::Command::new("lsof");
    cmd.args(["-nP", "-Fpc"]);
    match proto {
        Proto::Udp => {
            cmd.arg(format!("-iUDP:{port}"));
        }
        Proto::Tcp => {
            cmd.arg(format!("-iTCP:{port}")).arg("-sTCP:LISTEN");
        }
    }
    let out = cmd.output().ok()?;
    parse_lsof(&String::from_utf8_lossy(&out.stdout))
}

/// Salida de `lsof -F pc`: una línea `p<pid>` y luego `c<nombre>` por proceso.
#[cfg(any(target_os = "macos", test))]
fn parse_lsof(out: &str) -> Option<Owner> {
    let mut pid: Option<u32> = None;
    for line in out.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.trim().parse().ok();
        } else if let Some(c) = line.strip_prefix('c') {
            if let Some(pid) = pid {
                return Some(Owner { pid, name: c.trim().to_owned() });
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub fn kill(pid: u32) -> bool {
    let pid = pid as i32;
    if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
        std::thread::sleep(Duration::from_millis(500));
        if unsafe { libc::kill(pid, 0) } != 0 {
            return true;
        }
    }
    unsafe { libc::kill(pid, libc::SIGKILL) == 0 }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn owner(_port: u16, _proto: Proto) -> Option<Owner> {
    None
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn kill(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_puerto_libre_se_abre_sin_mas() {
        let shared = crate::state::new_shared();
        let s = bind_udp(&shared, "127.0.0.1", 0, "prueba").expect("puerto libre");
        assert!(s.local_addr().unwrap().port() > 0);
        assert!(shared.lock_tolerant().port_notice.is_none());
    }

    #[test]
    fn el_propio_proceso_no_se_mata() {
        // El mismo proceso ya tiene el puerto: nada de desalojarse a sí mismo
        let shared = crate::state::new_shared();
        let first = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let port = first.local_addr().unwrap().port();
        let r = bind_udp(&shared, "127.0.0.1", port, "prueba");
        assert!(r.is_err(), "no debía abrirse: {:?}", r.as_ref().map(|_| ()));
        assert!(shared.lock_tolerant().port_notice.is_none());
    }

    /// `bind` de mentira: falla con `kind` las `fails` primeras veces y luego
    /// devuelve el número del intento que lo consiguió.
    fn flaky_bind(fails: usize, kind: std::io::ErrorKind) -> impl FnMut() -> std::io::Result<usize> {
        let mut n = 0;
        move || {
            n += 1;
            if n <= fails {
                Err(std::io::Error::new(kind, "ocupado"))
            } else {
                Ok(n)
            }
        }
    }

    fn slept(log: &std::cell::RefCell<Vec<Duration>>) -> Duration {
        log.borrow().iter().sum()
    }

    fn alguien(pid: u32) -> Option<Owner> {
        Some(Owner { pid, name: "otro.exe".into() })
    }

    #[test]
    fn libre_a_la_primera_sin_esperar() {
        let slept_log = std::cell::RefCell::new(Vec::new());
        let r = bind_with_policy(
            flaky_bind(0, std::io::ErrorKind::AddrInUse),
            || panic!("no debe mirar el dueño"),
            |_| panic!("no debe matar"),
            1,
            |d| slept_log.borrow_mut().push(d),
        );
        assert!(matches!(r, Ok((1, Recovered::AtOnce))));
        assert_eq!(slept(&slept_log), Duration::ZERO);
    }

    #[test]
    fn ocupado_por_un_proceso_muerto_se_espera_a_que_lo_suelte() {
        // El caso de la actualización en caliente: el receptor viejo acaba de
        // morir y su puerto tarda un instante en soltarse; `kill` falla
        // porque el PID ya no existe
        let slept_log = std::cell::RefCell::new(Vec::new());
        let killed = std::cell::Cell::new(0);
        let r = bind_with_policy(
            flaky_bind(5, std::io::ErrorKind::AddrInUse),
            || alguien(2256),
            |pid| {
                assert_eq!(pid, 2256);
                killed.set(killed.get() + 1);
                false
            },
            1,
            |d| slept_log.borrow_mut().push(d),
        );
        let (n, how) = r.expect("debía abrirse al soltarse");
        assert_eq!(n, 6);
        assert_eq!(how, Recovered::Lingered { owner: alguien(2256), waited: Duration::from_millis(500) });
        assert_eq!(killed.get(), 1, "se intenta cerrar una vez y ya");
        assert_eq!(slept(&slept_log), Duration::from_millis(500));
    }

    #[test]
    fn sin_dueno_conocido_tambien_se_espera() {
        let slept_log = std::cell::RefCell::new(Vec::new());
        let r = bind_with_policy(
            flaky_bind(3, std::io::ErrorKind::AddrInUse),
            || None,
            |_| panic!("sin dueño no hay a quién cerrar"),
            1,
            |d| slept_log.borrow_mut().push(d),
        );
        let (n, how) = r.expect("debía abrirse");
        assert_eq!(n, 4);
        assert_eq!(how, Recovered::Lingered { owner: None, waited: Duration::from_millis(300) });
    }

    #[test]
    fn dueno_vivo_se_cierra_y_se_espera_a_que_muera() {
        let slept_log = std::cell::RefCell::new(Vec::new());
        let r = bind_with_policy(
            flaky_bind(2, std::io::ErrorKind::AddrInUse),
            || alguien(777),
            |pid| pid == 777,
            1,
            |d| slept_log.borrow_mut().push(d),
        );
        let (n, how) = r.expect("debía abrirse tras cerrar al dueño");
        assert_eq!(n, 3);
        assert_eq!(how, Recovered::Evicted(alguien(777).unwrap()));
        assert_eq!(slept(&slept_log), Duration::from_millis(200));
    }

    #[test]
    fn el_mismo_proceso_no_se_espera_ni_se_cierra() {
        let r = bind_with_policy(
            flaky_bind(99, std::io::ErrorKind::AddrInUse),
            || alguien(4242),
            |_| panic!("no debe cerrarse a sí mismo"),
            4242,
            |_| panic!("no debe esperar"),
        );
        assert!(matches!(r, Err(Failure::SameProcess)));
    }

    #[test]
    fn otro_error_no_se_reintenta() {
        let r = bind_with_policy(
            flaky_bind(99, std::io::ErrorKind::PermissionDenied),
            || panic!("no debe mirar el dueño"),
            |_| panic!("no debe matar"),
            1,
            |_| panic!("no debe esperar"),
        );
        assert!(matches!(r, Err(Failure::CannotListen(e)) if e.kind() == std::io::ErrorKind::PermissionDenied));
    }

    #[test]
    fn agotado_el_plazo_se_rinde_con_el_dueno_que_haya() {
        // sin dueño: toda la espera de un puerto que se suelta solo, y BusyUnknown
        let slept_log = std::cell::RefCell::new(Vec::new());
        let r = bind_with_policy(flaky_bind(999, std::io::ErrorKind::AddrInUse), || None, |_| false, 1, |d| {
            slept_log.borrow_mut().push(d)
        });
        assert!(matches!(r, Err(Failure::BusyUnknown)));
        assert_eq!(slept(&slept_log), LINGER_WAIT);
        // dueño muerto (no se pudo cerrar): la misma espera, y BusyKnown
        let slept_log = std::cell::RefCell::new(Vec::new());
        let r = bind_with_policy(flaky_bind(999, std::io::ErrorKind::AddrInUse), || alguien(5), |_| false, 1, |d| {
            slept_log.borrow_mut().push(d)
        });
        assert!(matches!(r, Err(Failure::BusyKnown(o)) if o.pid == 5));
        assert_eq!(slept(&slept_log), LINGER_WAIT);
        // dueño vivo cerrado que no suelta: la espera del desalojo, y BusyKnown
        let slept_log = std::cell::RefCell::new(Vec::new());
        let r = bind_with_policy(flaky_bind(999, std::io::ErrorKind::AddrInUse), || alguien(6), |_| true, 1, |d| {
            slept_log.borrow_mut().push(d)
        });
        assert!(matches!(r, Err(Failure::BusyKnown(o)) if o.pid == 6));
        assert_eq!(slept(&slept_log), EVICT_WAIT);
    }

    #[test]
    fn insistir_ensena_el_error_y_lo_retira_al_conseguirlo() {
        let shared = crate::state::new_shared();
        let slept_log = std::cell::RefCell::new(Vec::new());
        let mut n = 0;
        let got = bind_insisting_with(
            &shared,
            "prueba UDP 1",
            || {
                n += 1;
                if n < 3 {
                    Err("ocupado".to_owned())
                } else {
                    Ok(7)
                }
            },
            |d| {
                // mientras se insiste, el error (y que se insiste) está a la vista
                let err = shared.lock_tolerant().last_error.clone().unwrap_or_default();
                assert!(err.starts_with("ocupado; "), "{err}");
                slept_log.borrow_mut().push(d);
            },
        );
        assert_eq!(got, 7);
        assert_eq!(slept(&slept_log), BIND_RETRY * 2);
        assert!(shared.lock_tolerant().last_error.is_none(), "el error del puerto se retira al abrirlo");
    }

    #[test]
    fn insistir_no_borra_un_error_que_ya_es_de_otro() {
        let shared = crate::state::new_shared();
        let mut n = 0;
        let got = bind_insisting_with(
            &shared,
            "prueba UDP 2",
            || {
                n += 1;
                if n < 2 {
                    Err("ocupado".to_owned())
                } else {
                    Ok(1)
                }
            },
            |_| shared.lock_tolerant().last_error = Some("otro aviso".to_owned()),
        );
        assert_eq!(got, 1);
        assert_eq!(shared.lock_tolerant().last_error.as_deref(), Some("otro aviso"));
    }

    #[test]
    fn inodo_del_socket_en_proc_net() {
        let udp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops\n\
                   1234: 0100007F:6888 00000000:0000 07 00000000:00000000 00:00000000 00000000  1000        0 555001 2 0000000000000000 0\n\
                   1235: 00000000:6889 00000000:0000 07 00000000:00000000 00:00000000 00000000  1000        0 555002 2 0000000000000000 0\n";
        assert_eq!(linux_socket_inode(udp, 26760, Proto::Udp), Some(555001));
        assert_eq!(linux_socket_inode(udp, 26761, Proto::Udp), Some(555002));
        assert_eq!(linux_socket_inode(udp, 1, Proto::Udp), None);
        let tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
                   0: 00000000:6889 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 777001 1 0000000000000000 100 0 0 10 0\n\
                   1: 0100007F:6889 0100007F:D431 01 00000000:00000000 00:00000000 00000000  1000        0 777002 1 0000000000000000 20 4 30 10 -1\n";
        assert_eq!(linux_socket_inode(tcp, 26761, Proto::Tcp), Some(777001), "solo el LISTEN");
    }

    #[test]
    fn lsof_en_formato_f_da_pid_y_nombre() {
        let o = parse_lsof("p1234\ncCemu\nf5\nn*:26760\n").unwrap();
        assert_eq!((o.pid, o.name.as_str()), (1234, "Cemu"));
        assert!(parse_lsof("").is_none());
        assert!(parse_lsof("cSinPid\n").is_none());
    }
}
