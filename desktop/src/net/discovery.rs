//! Anuncio mDNS del receptor: _pepomote._tcp.local.
//! Es opcional: si falla, el broadcast UDP y el QR siguen funcionando.

use crate::pairing::PairingInfo;
use crate::state::SharedState;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;

/// Nombre del PC → etiqueta DNS para el host mDNS: minúsculas, `[a-z0-9-]`,
/// sin guiones repetidos ni en los extremos, 63 caracteres como mucho; vacío
/// → «pepomote». Un ComputerName de macOS («MacBook de Dani») no vale tal
/// cual; un hostname de Windows o Linux queda igual que antes (en minúsculas).
pub fn dns_label(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_matches('-').chars().take(63).collect();
    if out.is_empty() {
        "pepomote".to_owned()
    } else {
        out
    }
}

pub fn run(shared: SharedState, pairing: PairingInfo) {
    // Un fallo de mDNS no es un error para el usuario en macOS (mDNSResponder
    // ya ocupa el 5353 y a veces no comparte); el QR y el broadcast siguen
    let fail = |msg: String| {
        crate::log_line!("{msg}");
        if !cfg!(target_os = "macos") {
            shared.lock().unwrap().last_error = Some(msg);
        }
    };
    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            fail(format!("mDNS no disponible: {e}"));
            return;
        }
    };

    let mut props = HashMap::new();
    props.insert("pv".to_owned(), "1".to_owned());
    props.insert("name".to_owned(), pairing.name.clone());

    let instance = format!("PepoMote-{}", pairing.name);
    let host = format!("{}.local.", dns_label(&pairing.name));
    let info = match ServiceInfo::new(
        "_pepomote._tcp.local.",
        &instance,
        &host,
        pairing.host,
        pairing.port,
        Some(props),
    ) {
        Ok(i) => i,
        Err(e) => {
            fail(format!("mDNS: {e}"));
            return;
        }
    };

    if let Err(e) = daemon.register(info) {
        fail(format!("mDNS: {e}"));
        return;
    }

    // Mantener vivo el daemon
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

#[cfg(test)]
mod tests {
    use super::dns_label;

    #[test]
    fn el_nombre_del_pc_se_convierte_en_etiqueta_dns() {
        assert_eq!(dns_label("DESKTOP-ABC123"), "desktop-abc123");
        assert_eq!(dns_label("salon"), "salon");
        assert_eq!(dns_label("MacBook de Dani"), "macbook-de-dani");
        assert_eq!(dns_label("  Mac mini (2024) de Ángel  "), "mac-mini-2024-de-angel");
        assert_eq!(dns_label("PC de Peña"), "pc-de-pena");
        assert_eq!(dns_label("---"), "pepomote");
        assert_eq!(dns_label(""), "pepomote");
        assert_eq!(dns_label(&"a".repeat(80)).len(), 63);
    }
}
