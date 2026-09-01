/// Codec compartido con el emisor Linux móvil (crate `pmp`).
pub use pmp as codec;
pub mod control;
pub mod discovery;
pub mod telemetry;

use crate::dsu::Dsu;
use crate::pairing::PairingInfo;
use crate::state::SharedState;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

/// Límite del protocolo DSU: 4 mandos.
pub const MAX_PLAYERS: usize = 4;

/// Sesión de un móvil. La crea el canal de control; la consume telemetría.
pub struct Session {
    pub id: u32,
    pub slot: u8,
    pub last_seq: Option<u32>,
    pub phone_udp: Option<SocketAddr>,
}

/// session_id → Session (hasta MAX_PLAYERS a la vez).
pub type Sessions = Arc<Mutex<HashMap<u32, Session>>>;

/// Slot libre más bajo (el Jugador 1 es el slot 0).
pub fn lowest_free_slot(sessions: &HashMap<u32, Session>) -> Option<u8> {
    (0..MAX_PLAYERS as u8).find(|s| !sessions.values().any(|x| x.slot == *s))
}

pub fn start(shared: SharedState, pairing: PairingInfo, dsu: Option<Arc<Dsu>>) {
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));

    {
        let shared = shared.clone();
        let sessions = sessions.clone();
        let pairing = pairing.clone();
        std::thread::Builder::new()
            .name("pmp-control".into())
            .spawn(move || control::run(shared, sessions, pairing))
            .expect("hilo control");
    }
    {
        let shared = shared.clone();
        let sessions = sessions.clone();
        let pairing = pairing.clone();
        std::thread::Builder::new()
            .name("pmp-telemetry".into())
            .spawn(move || telemetry::run(shared, sessions, pairing, dsu))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sess(id: u32, slot: u8) -> Session {
        Session {
            id,
            slot,
            last_seq: None,
            phone_udp: None,
        }
    }

    #[test]
    fn slots_se_asignan_y_reutilizan() {
        let mut m = HashMap::new();
        assert_eq!(lowest_free_slot(&m), Some(0));
        m.insert(1, sess(1, 0));
        m.insert(2, sess(2, 1));
        assert_eq!(lowest_free_slot(&m), Some(2));
        // se va el jugador 1: su slot 0 queda libre y es el siguiente en asignarse
        m.remove(&1);
        assert_eq!(lowest_free_slot(&m), Some(0));
        m.insert(3, sess(3, 0));
        m.insert(4, sess(4, 2));
        m.insert(5, sess(5, 3));
        assert_eq!(lowest_free_slot(&m), None); // lleno: busy
    }
}
