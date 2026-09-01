//! Anuncio mDNS del receptor: _pepomote._tcp.local.
//! Es opcional: si falla, el broadcast UDP y el QR siguen funcionando.

use crate::pairing::PairingInfo;
use crate::state::SharedState;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;

pub fn run(shared: SharedState, pairing: PairingInfo) {
    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            shared.lock().unwrap().last_error = Some(format!("mDNS no disponible: {e}"));
            return;
        }
    };

    let mut props = HashMap::new();
    props.insert("pv".to_owned(), "1".to_owned());
    props.insert("name".to_owned(), pairing.name.clone());

    let instance = format!("PepoMote-{}", pairing.name);
    let host = format!("{}.local.", pairing.name.to_lowercase());
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
            shared.lock().unwrap().last_error = Some(format!("mDNS: {e}"));
            return;
        }
    };

    if let Err(e) = daemon.register(info) {
        shared.lock().unwrap().last_error = Some(format!("mDNS: {e}"));
        return;
    }

    // Mantener vivo el daemon
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
