//! Descubrimiento del receptor por broadcast UDP (PROTOCOL.md §1):
//! PMPDISCOVER1 → PMPHERE1 <json>. Broadcast limitado (255.255.255.255) y
//! dirigido de la subred (x.y.z.255, /24: lo normal en casa); bastantes
//! routers descartan el limitado.

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
    let mut found: Vec<Receiver> = Vec::new();
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return found;
    };
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));

    let mut targets = vec![Ipv4Addr::BROADCAST];
    if let Some(ip) = local_ipv4() {
        let o = ip.octets();
        targets.push(Ipv4Addr::new(o[0], o[1], o[2], 255));
    }
    for t in &targets {
        let _ = sock.send_to(pmp::DISCOVER, (*t, pmp::DEFAULT_PORT));
    }

    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 512];
    while Instant::now() < deadline {
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
}
