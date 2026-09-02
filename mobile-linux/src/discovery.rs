//! Descubrimiento del receptor (PROTOCOL.md §1), por dos caminos a la vez:
//! - broadcast UDP PMPDISCOVER1 → PMPHERE1, al limitado (255.255.255.255) y al
//!   DIRIGIDO de cada interfaz (con su máscara real, no un /24 supuesto);
//! - mDNS: el receptor anuncia `_pepomote._tcp.local.`.
//! Bastantes routers/APs descartan el broadcast limitado, y algunos también el
//! dirigido; el mDNS es multicast y suele pasar donde el broadcast no.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct Receiver {
    pub name: String,
    pub host: String,
    pub port: u16,
}

/// Sondeo bloqueante durante `timeout`; llamar desde un hilo aparte.
pub fn scan(timeout: Duration) -> Vec<Receiver> {
    let mdns = std::thread::spawn(move || mdns_browse(timeout));
    let mut found = broadcast_scan(timeout);
    for r in mdns.join().unwrap_or_default() {
        if !found.iter().any(|f| f.host == r.host) {
            found.push(r);
        }
    }
    found
}

/// Direcciones de broadcast: la limitada más la dirigida de cada interfaz.
pub fn broadcast_targets() -> Vec<Ipv4Addr> {
    let mut set: HashSet<Ipv4Addr> = HashSet::new();
    set.insert(Ipv4Addr::BROADCAST);
    if let Ok(ifs) = if_addrs::get_if_addrs() {
        for i in ifs {
            if i.is_loopback() {
                continue;
            }
            if let if_addrs::IfAddr::V4(v4) = i.addr {
                if let Some(b) = v4.broadcast {
                    set.insert(b);
                }
            }
        }
    }
    if let Some(ip) = local_ipv4() {
        let o = ip.octets();
        set.insert(Ipv4Addr::new(o[0], o[1], o[2], 255)); // por si la interfaz no da broadcast
    }
    let mut v: Vec<Ipv4Addr> = set.into_iter().collect();
    v.sort();
    v
}

fn broadcast_scan(timeout: Duration) -> Vec<Receiver> {
    let mut found: Vec<Receiver> = Vec::new();
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return found;
    };
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
    let targets = broadcast_targets();
    let deadline = Instant::now() + timeout;
    let mut last_probe = Instant::now() - Duration::from_secs(1);
    let mut buf = [0u8; 512];
    while Instant::now() < deadline {
        // re-sondeo cada 500 ms: un datagrama perdido no debe dejar la lista vacía
        if last_probe.elapsed() >= Duration::from_millis(500) {
            last_probe = Instant::now();
            for t in &targets {
                let _ = sock.send_to(pmp::DISCOVER, (*t, pmp::DEFAULT_PORT));
            }
        }
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue;
        };
        if let Some(r) = parse_here(&buf[..n], from) {
            if !found.iter().any(|f| f.host == r.host) {
                found.push(r);
            }
        }
    }
    found
}

fn mdns_browse(timeout: Duration) -> Vec<Receiver> {
    let mut out: Vec<Receiver> = Vec::new();
    let Ok(daemon) = mdns_sd::ServiceDaemon::new() else {
        return out;
    };
    let Ok(rx) = daemon.browse("_pepomote._tcp.local.") else {
        return out;
    };
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match rx.recv_timeout(left) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                let name = info
                    .get_property_val_str("name")
                    .map(|s| s.to_owned())
                    .unwrap_or_else(|| info.get_fullname().split('.').next().unwrap_or("PC").to_owned());
                for addr in info.get_addresses() {
                    let host = addr.to_string();
                    if host.contains(':') || out.iter().any(|r| r.host == host) {
                        continue; // solo IPv4, sin repetir
                    }
                    out.push(Receiver {
                        name: name.clone(),
                        host,
                        port: info.get_port(),
                    });
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();
    out
}

pub fn parse_here(buf: &[u8], from: SocketAddr) -> Option<Receiver> {
    let json = buf.strip_prefix(pmp::HERE_PREFIX)?;
    let v: serde_json::Value = serde_json::from_slice(json).ok()?;
    if v["pv"].as_i64() != Some(1) {
        return None;
    }
    let host = from.ip().to_string();
    Some(Receiver {
        name: v["name"].as_str().unwrap_or(&host).to_owned(),
        port: v["tcp"]
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(pmp::DEFAULT_PORT),
        host,
    })
}

/// IP local de la ruta por defecto (el connect UDP no envía nada).
pub fn local_ipv4() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) => Some(ip),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_here() {
        let from: SocketAddr = "192.168.1.20:26761".parse().unwrap();
        let r = parse_here(br#"PMPHERE1 {"pv":1,"name":"TORRE","tcp":26761}"#, from).unwrap();
        assert_eq!(
            r,
            Receiver {
                name: "TORRE".into(),
                host: "192.168.1.20".into(),
                port: 26761
            }
        );
        assert!(parse_here(br#"PMPHERE1 {"pv":2,"name":"X","tcp":1}"#, from).is_none());
        assert!(parse_here(b"otra cosa", from).is_none());
    }

    #[test]
    fn objetivos_de_broadcast_incluyen_el_limitado() {
        let t = broadcast_targets();
        assert!(t.contains(&Ipv4Addr::BROADCAST));
    }
}
