pub mod codec;
pub mod control;
pub mod discovery;
pub mod telemetry;

use crate::pairing::PairingInfo;
use crate::state::SharedState;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

/// Sesión activa (un móvil a la vez en v1). La crea el canal de control,
/// la consume el hilo de telemetría.
pub struct Session {
    pub id: u32,
    pub last_seq: Option<u32>,
    pub phone_udp: Option<SocketAddr>,
}

pub type SharedSession = Arc<Mutex<Option<Session>>>;

pub fn start(shared: SharedState, pairing: PairingInfo) {
    let session: SharedSession = Arc::new(Mutex::new(None));

    {
        let shared = shared.clone();
        let session = session.clone();
        let pairing = pairing.clone();
        std::thread::Builder::new()
            .name("pmp-control".into())
            .spawn(move || control::run(shared, session, pairing))
            .expect("hilo control");
    }
    {
        let shared = shared.clone();
        let session = session.clone();
        let pairing = pairing.clone();
        std::thread::Builder::new()
            .name("pmp-telemetry".into())
            .spawn(move || telemetry::run(shared, session, pairing))
            .expect("hilo telemetría");
    }
    {
        let shared = shared.clone();
        std::thread::Builder::new()
            .name("pmp-mdns".into())
            .spawn(move || discovery::run(shared, pairing))
            .expect("hilo mdns");
    }
}
