use rand::RngCore;
use std::net::{IpAddr, UdpSocket};

pub const TCP_PORT: u16 = 26761;

#[derive(Clone)]
pub struct PairingInfo {
    pub host: IpAddr,
    pub port: u16,
    pub token: String,
    pub name: String,
}

impl PairingInfo {
    pub fn generate() -> Self {
        Self {
            host: local_ip(),
            port: TCP_PORT,
            token: load_or_create_token(),
            name: host_name(),
        }
    }

    /// URL que va dentro del QR: pepomote://pair?v=1&host=..&port=..&t=..&name=..
    pub fn pair_url(&self) -> String {
        format!(
            "pepomote://pair?v=1&host={}&port={}&t={}&name={}",
            self.host,
            self.port,
            self.token,
            url_encode(&self.name)
        )
    }
}

/// IP local de la interfaz por defecto. El connect UDP no envía ningún paquete.
fn local_ip() -> IpAddr {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .map(|a| a.ip())
        .unwrap_or(IpAddr::from([127, 0, 0, 1]))
}

fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "PC".to_owned())
}

/// Token de emparejamiento de 128 bits, persistido en el directorio de config.
fn load_or_create_token() -> String {
    let path = directories::ProjectDirs::from("dev", "pepotech", "PepoMote")
        .map(|d| d.config_dir().join("token.txt"));

    if let Some(ref p) = path {
        if let Ok(t) = std::fs::read_to_string(p) {
            let t = t.trim().to_owned();
            if t.len() == 32 && t.chars().all(|c| c.is_ascii_hexdigit()) {
                return t;
            }
        }
    }

    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

    if let Some(p) = path {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&p, &token);
    }
    token
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
