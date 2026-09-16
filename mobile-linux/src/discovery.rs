//! Descubrimiento del receptor (PROTOCOL.md §1), por dos caminos a la vez:
//! - broadcast UDP PMPDISCOVER1 → PMPHERE1, al limitado (255.255.255.255) y al
//!   DIRIGIDO de cada interfaz (con su máscara real, no un /24 supuesto);
//! - mDNS: el receptor anuncia `_pepomote._tcp.local.`.
//! Bastantes routers/APs descartan el broadcast limitado, y algunos también el
//! dirigido; el mDNS es multicast y suele pasar donde el broadcast no.
//!
//! Cada receptor se avisa según contesta ([`ScanEvent::Found`]): el PC de al
//! lado sale en la lista en decenas de ms, sin esperar a que acabe el sondeo.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct Receiver {
    pub name: String,
    pub host: String,
    pub port: u16,
}

/// Lo que va soltando un sondeo: cada receptor nuevo según contesta (con la
/// lista vista hasta el momento) y, al acabar, la lista completa.
#[derive(Clone, Debug, PartialEq)]
pub enum ScanEvent {
    Found(Vec<Receiver>),
    Done(Vec<Receiver>),
}

/// Lo visto en un sondeo, sin repetir IP, desde los dos hilos (broadcast y
/// mDNS). Un receptor contesta a cada broadcast que le llega (el limitado y
/// el dirigido): solo el primero cuenta como nuevo y avisa.
struct Seen {
    list: Mutex<Vec<Receiver>>,
    tx: Option<Sender<ScanEvent>>,
}

impl Seen {
    fn new(tx: Option<Sender<ScanEvent>>) -> Self {
        Seen {
            list: Mutex::new(Vec::new()),
            tx,
        }
    }

    fn add(&self, r: Receiver) {
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        if list.iter().any(|f| f.host == r.host) {
            return;
        }
        list.push(r);
        if let Some(tx) = &self.tx {
            let _ = tx.send(ScanEvent::Found(list.clone()));
        }
    }

    fn snapshot(&self) -> Vec<Receiver> {
        self.list.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Sondeo bloqueante durante `timeout`; llamar desde un hilo aparte.
pub fn scan(timeout: Duration) -> Vec<Receiver> {
    scan_into(timeout, None)
}

/// Sondeo bloqueante que además va avisando por `tx` de cada receptor según
/// contesta y, al acabar, manda la lista completa (ver [`ScanEvent`]).
pub fn scan_into(timeout: Duration, tx: Option<Sender<ScanEvent>>) -> Vec<Receiver> {
    let seen = Arc::new(Seen::new(tx));
    let mdns = {
        let seen = seen.clone();
        std::thread::spawn(move || mdns_browse(timeout, &seen))
    };
    broadcast_scan(&broadcast_targets(), pmp::DEFAULT_PORT, timeout, &seen);
    let _ = mdns.join();
    let list = seen.snapshot();
    if let Some(tx) = &seen.tx {
        let _ = tx.send(ScanEvent::Done(list.clone()));
    }
    list
}

/// Lo visto en un sondeo a medias, encima de lo que ya se enseñaba: se añade
/// y se actualiza por IP, y no se quita nada hasta que el sondeo acabe
/// (entonces la lista completa reemplaza a esta), así la marca «en la red»
/// no parpadea al empezar cada sondeo nuevo.
pub fn merge(shown: &[Receiver], seen: &[Receiver]) -> Vec<Receiver> {
    let mut out = shown.to_vec();
    for r in seen {
        if let Some(i) = out.iter().position(|s| s.host == r.host) {
            out[i] = r.clone();
        } else {
            out.push(r.clone());
        }
    }
    out
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

fn broadcast_scan(targets: &[Ipv4Addr], port: u16, timeout: Duration, seen: &Seen) {
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return;
    };
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
    let deadline = Instant::now() + timeout;
    let mut last_probe = Instant::now() - Duration::from_secs(1);
    let mut buf = [0u8; 512];
    while Instant::now() < deadline {
        // re-sondeo cada 500 ms: un datagrama perdido no debe dejar la lista vacía
        if last_probe.elapsed() >= Duration::from_millis(500) {
            last_probe = Instant::now();
            for t in targets {
                let _ = sock.send_to(pmp::DISCOVER, (*t, port));
            }
        }
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue;
        };
        if let Some(r) = parse_here(&buf[..n], from) {
            seen.add(r);
        }
    }
}

fn mdns_browse(timeout: Duration, seen: &Seen) {
    let Ok(daemon) = mdns_sd::ServiceDaemon::new() else {
        return;
    };
    let Ok(rx) = daemon.browse("_pepomote._tcp.local.") else {
        return;
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
                    if host.contains(':') {
                        continue; // solo IPv4
                    }
                    seen.add(Receiver {
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
    use std::sync::mpsc;

    fn rx(name: &str, host: &str) -> Receiver {
        Receiver {
            name: name.into(),
            host: host.into(),
            port: 26761,
        }
    }

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

    #[test]
    fn lo_visto_no_repite_ip_y_avisa_solo_de_lo_nuevo() {
        let (tx, events) = mpsc::channel();
        let seen = Seen::new(Some(tx));
        seen.add(rx("A", "192.168.1.5"));
        seen.add(rx("A otra vez", "192.168.1.5"));
        seen.add(rx("B", "192.168.1.6"));
        assert_eq!(events.try_recv().unwrap(), ScanEvent::Found(vec![rx("A", "192.168.1.5")]));
        assert_eq!(
            events.try_recv().unwrap(),
            ScanEvent::Found(vec![rx("A", "192.168.1.5"), rx("B", "192.168.1.6")])
        );
        assert!(events.try_recv().is_err(), "la misma IP no vuelve a avisar");
        assert_eq!(seen.snapshot().len(), 2);
    }

    #[test]
    fn merge_anade_y_actualiza_sin_quitar_nada() {
        let shown = vec![rx("SALÓN-PC", "192.168.1.5"), rx("Viejo", "192.168.1.7")];
        let seen = vec![
            Receiver {
                name: "SALON".into(),
                host: "192.168.1.5".into(),
                port: 26800,
            },
            rx("Nuevo", "192.168.1.8"),
        ];
        let merged = merge(&shown, &seen);
        let hosts: Vec<&str> = merged.iter().map(|r| r.host.as_str()).collect();
        assert_eq!(hosts, ["192.168.1.5", "192.168.1.7", "192.168.1.8"], "orden: lo enseñado y luego lo nuevo");
        assert_eq!(merged[0], seen[0], "misma IP: se actualiza");
        assert_eq!(merged[1].name, "Viejo", "un sondeo a medias no quita nada");
        assert_eq!(merge(&shown, &[]), shown);
        assert_eq!(merge(&[], &seen), seen);
    }

    /// Receptor falso en el loopback que contesta PMPHERE1 a cada sondeo.
    fn fake_receiver(name: &'static str) -> u16 {
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        let _ = sock.set_read_timeout(Some(Duration::from_secs(5)));
        std::thread::spawn(move || {
            let mut buf = [0u8; 64];
            while let Ok((n, from)) = sock.recv_from(&mut buf) {
                if &buf[..n] == pmp::DISCOVER {
                    let reply = format!("PMPHERE1 {{\"pv\":1,\"name\":\"{name}\",\"tcp\":26761}}");
                    let _ = sock.send_to(reply.as_bytes(), from);
                }
            }
        });
        port
    }

    #[test]
    fn el_primer_receptor_se_avisa_sin_esperar_al_final_del_sondeo() {
        let port = fake_receiver("TORRE");
        let (tx, events) = mpsc::channel();
        let seen = Seen::new(Some(tx));
        let t0 = Instant::now();
        broadcast_scan(&[Ipv4Addr::LOCALHOST], port, Duration::from_millis(1500), &seen);
        let total = t0.elapsed();
        assert!(total >= Duration::from_millis(1400), "el sondeo entero sigue durando lo suyo ({total:?})");
        // El aviso lleva su hora de llegada implícita: es el primer evento y
        // el hilo lo mandó nada más recibir la respuesta, no al acabar
        let first = events.try_recv().unwrap();
        assert_eq!(first, ScanEvent::Found(vec![rx("TORRE", "127.0.0.1")]));
        assert!(events.try_recv().is_err(), "el re-sondeo de los 500 ms no lo repite");
        assert_eq!(seen.snapshot(), vec![rx("TORRE", "127.0.0.1")]);
    }

    #[test]
    fn el_aviso_llega_antes_de_que_acabe_el_sondeo() {
        let port = fake_receiver("TORRE");
        let (tx, events) = mpsc::channel();
        let t0 = Instant::now();
        std::thread::spawn(move || {
            let seen = Seen::new(Some(tx));
            broadcast_scan(&[Ipv4Addr::LOCALHOST], port, Duration::from_millis(1500), &seen);
        });
        let first = events.recv_timeout(Duration::from_millis(700)).expect("el primer receptor sale mucho antes de los 1500 ms");
        assert_eq!(first, ScanEvent::Found(vec![rx("TORRE", "127.0.0.1")]));
        assert!(t0.elapsed() < Duration::from_millis(700));
    }
}
