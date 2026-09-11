//! Puertos ocupados: si el DSU (26760) o el canal del móvil (26761) ya los
//! tiene otro proceso, PepoMote no funciona y nadie sabe por qué. Aquí se
//! averigua QUIÉN los tiene (otro servidor DSU como DS4Windows o BetterJoy,
//! o un PepoMote antiguo que se quedó colgado), se cierra ese proceso y se
//! vuelve a abrir el puerto. Se deja dicho en la ventana y en los móviles.
//!
//! Windows: tablas UDP/TCP con PID (iphlpapi) + TerminateProcess.
//! Linux: /proc/net/{udp,tcp} → inodo → /proc/*/fd → PID + `kill`.

use crate::state::SharedState;
use std::net::{TcpListener, UdpSocket};
use std::time::{Duration, Instant};

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

/// Cuánto se espera a que el proceso muera y suelte el puerto.
const EVICT_WAIT: Duration = Duration::from_millis(2500);

/// Abre un socket UDP; si el puerto está ocupado, desaloja al dueño y reintenta.
pub fn bind_udp(shared: &SharedState, addr: &str, port: u16, what: &str) -> Result<UdpSocket, String> {
    bind_evicting(shared, port, Proto::Udp, what, || UdpSocket::bind((addr, port)))
}

/// Abre un listener TCP; si el puerto está ocupado, desaloja al dueño y reintenta.
pub fn bind_tcp(shared: &SharedState, addr: &str, port: u16, what: &str) -> Result<TcpListener, String> {
    bind_evicting(shared, port, Proto::Tcp, what, || TcpListener::bind((addr, port)))
}

fn bind_evicting<T>(
    shared: &SharedState,
    port: u16,
    proto: Proto,
    what: &str,
    bind: impl Fn() -> std::io::Result<T>,
) -> Result<T, String> {
    let first = match bind() {
        Ok(s) => return Ok(s),
        Err(e) => e,
    };
    if first.kind() != std::io::ErrorKind::AddrInUse {
        return Err(format!("{what}: no puedo escuchar en {} {port}: {first}", proto_name(proto)));
    }
    let Some(owner) = owner(port, proto) else {
        return Err(format!(
            "{what}: el puerto {} {port} está ocupado por otro programa que no he podido identificar; ciérralo (o reinicia) y vuelve a abrir PepoMote",
            proto_name(proto)
        ));
    };
    if owner.pid == std::process::id() {
        return Err(format!("{what}: el puerto {} {port} ya está abierto en este mismo proceso", proto_name(proto)));
    }
    let killed = kill(owner.pid);
    if killed {
        let start = Instant::now();
        while start.elapsed() < EVICT_WAIT {
            std::thread::sleep(Duration::from_millis(100));
            if let Ok(s) = bind() {
                let msg = format!(
                    "Puerto {} {port} estaba ocupado por {} (PID {}): lo he cerrado para que PepoMote funcione",
                    proto_name(proto),
                    owner.name,
                    owner.pid
                );
                shared.lock().unwrap().port_notice = Some(msg.clone());
                crate::net::notify_all(&msg);
                return Ok(s);
            }
        }
    }
    Err(format!(
        "{what}: el puerto {} {port} lo tiene {} (PID {}) y no he podido cerrarlo (¿va como administrador?): ciérralo tú y vuelve a abrir PepoMote",
        proto_name(proto),
        owner.name,
        owner.pid
    ))
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

#[cfg(not(any(windows, target_os = "linux")))]
pub fn owner(_port: u16, _proto: Proto) -> Option<Owner> {
    None
}

#[cfg(not(any(windows, target_os = "linux")))]
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
        assert!(shared.lock().unwrap().port_notice.is_none());
    }

    #[test]
    fn el_propio_proceso_no_se_mata() {
        // El mismo proceso ya tiene el puerto: nada de desalojarse a sí mismo
        let shared = crate::state::new_shared();
        let first = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let port = first.local_addr().unwrap().port();
        let r = bind_udp(&shared, "127.0.0.1", port, "prueba");
        assert!(r.is_err(), "no debía abrirse: {:?}", r.as_ref().map(|_| ()));
        assert!(shared.lock().unwrap().port_notice.is_none());
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
}
