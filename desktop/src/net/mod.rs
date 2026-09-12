/// Codec compartido con el emisor Linux móvil (crate `pmp`).
pub use pmp as codec;
pub mod control;
pub mod discovery;
pub mod screen;
pub mod telemetry;

use crate::dsu::Dsu;
use crate::pairing::PairingInfo;
use crate::screen::ScreenHub;
use crate::state::{Role, SharedState};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex, OnceLock};

/// Límite del protocolo DSU: 4 mandos.
pub const MAX_PLAYERS: usize = 4;

/// Sesión de un móvil. La crea el canal de control; la consume telemetría.
pub struct Session {
    pub id: u32,
    pub slot: u8,
    pub last_seq: Option<u32>,
    pub phone_udp: Option<SocketAddr>,
    /// Identidad del móvil (IP + nombre): si vuelve a conectar con esta
    /// sesión aún viva, es una reconexión y la fantasma se desaloja.
    pub peer: std::net::IpAddr,
    pub device: String,
    /// Mando o Nunchuk (PROTOCOL.md §3).
    pub role: Role,
    /// Modo Wii U: pidió ser Mando Wii en vez de GamePad / Pro.
    pub pad_wii: bool,
    /// Canal de control de esta sesión, para avisarle desde fuera de su hilo
    /// (difusión de `mode` y `notice`). None solo en tests.
    pub writer: Option<Arc<Mutex<TcpStream>>>,
    /// Último `pad` (tipo de mando en Cemu) que se le dijo a este móvil, para
    /// avisarle solo cuando cambia (J2 pasa a GamePad si J1 se va, el
    /// Nunchuk entra en uso cuando su jugador elige Mando de Wii…).
    pub last_pad: Option<&'static str>,
}

/// Escribe una línea JSON en el canal de control de una sesión.
pub fn send_line(writer: &Arc<Mutex<TcpStream>>, v: &serde_json::Value) {
    use std::io::Write;
    let mut line = v.to_string();
    line.push('\n');
    if let Ok(mut w) = writer.lock() {
        let _ = w.write_all(line.as_bytes());
    }
}

/// Sesiones vivas, accesibles desde los módulos de configuración
/// (Dolphin/Cemu) para avisar a los móviles.
static SESSIONS: OnceLock<Sessions> = OnceLock::new();

/// Las sesiones vivas (None antes de `start`).
pub(crate) fn sessions() -> Option<Sessions> {
    SESSIONS.get().cloned()
}

/// Envía `v` (una línea JSON) a todas las sesiones salvo `except`.
pub fn broadcast(v: &serde_json::Value, except: Option<u32>) {
    let Some(sessions) = SESSIONS.get() else { return };
    let writers: Vec<Arc<Mutex<TcpStream>>> = sessions
        .lock()
        .unwrap()
        .values()
        .filter(|s| Some(s.id) != except)
        .filter_map(|s| s.writer.clone())
        .collect();
    for w in writers {
        send_line(&w, v);
    }
}

/// Aviso legible para todos los móviles (PROTOCOL.md §3, `notice`).
pub fn notify_all(text: &str) {
    broadcast(&serde_json::json!({"m":"notice","text":text}), None);
}

/// Sesiones de ESTE mismo móvil que siguen vivas (reconexión tras caída de
/// Wi-Fi, app en segundo plano…). Sin desalojarlas, el móvil entraría como
/// Jugador 2: sin puntero y sin poder cambiar a Dolphin hasta que caducaran.
pub fn ghosts_of(sessions: &HashMap<u32, Session>, peer: std::net::IpAddr, device: &str) -> Vec<u32> {
    sessions
        .values()
        .filter(|s| s.peer == peer && s.device == device)
        .map(|s| s.id)
        .collect()
}

/// session_id → Session (hasta MAX_PLAYERS a la vez).
pub type Sessions = Arc<Mutex<HashMap<u32, Session>>>;

/// Slot libre según el papel: los Wiimotes desde el 0 hacia arriba, los
/// Nunchuks desde el 3 hacia abajo (así el Nunchuk i-ésimo por slot
/// descendente es el del Wiimote i-ésimo por slot ascendente).
pub fn free_slot(sessions: &HashMap<u32, Session>, role: Role) -> Option<u8> {
    let taken = |s: u8| sessions.values().any(|x| x.slot == s);
    match role {
        Role::Wiimote => (0..MAX_PLAYERS as u8).find(|s| !taken(*s)),
        Role::Nunchuk => (0..MAX_PLAYERS as u8).rev().find(|s| !taken(*s)),
    }
}

pub fn start(shared: SharedState, pairing: PairingInfo, dsu: Option<Arc<Dsu>>, hub: Arc<ScreenHub>) {
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
    let _ = SESSIONS.set(sessions.clone());

    {
        let shared = shared.clone();
        let sessions = sessions.clone();
        let pairing = pairing.clone();
        let hub = hub.clone();
        std::thread::Builder::new()
            .name("pmp-control".into())
            .spawn(move || control::run(shared, sessions, pairing, hub))
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
            peer: std::net::IpAddr::from([192, 168, 1, 10 + slot]),
            device: format!("Movil{slot}"),
            role: Role::Wiimote,
            pad_wii: false,
            writer: None,
            last_pad: None,
        }
    }

    #[test]
    fn nunchuks_desde_arriba_y_wiimotes_desde_abajo() {
        let mut m = HashMap::new();
        assert_eq!(free_slot(&m, Role::Nunchuk), Some(3));
        m.insert(1, Session { role: Role::Nunchuk, ..sess(1, 3) });
        assert_eq!(free_slot(&m, Role::Wiimote), Some(0), "el mando sigue siendo el Jugador 1");
        m.insert(2, sess(2, 0));
        assert_eq!(free_slot(&m, Role::Nunchuk), Some(2));
        assert_eq!(free_slot(&m, Role::Wiimote), Some(1));
        m.insert(3, sess(3, 1));
        m.insert(4, Session { role: Role::Nunchuk, ..sess(4, 2) });
        assert_eq!(free_slot(&m, Role::Wiimote), None);
        assert_eq!(free_slot(&m, Role::Nunchuk), None);
    }

    #[test]
    fn el_mismo_movil_que_reconecta_desaloja_su_fantasma() {
        let mut m = HashMap::new();
        m.insert(1, sess(1, 0));
        m.insert(2, sess(2, 1));
        // misma IP y nombre que la sesión 1: es ella reconectando
        let ghosts = ghosts_of(&m, std::net::IpAddr::from([192, 168, 1, 10]), "Movil0");
        assert_eq!(ghosts, vec![1]);
        // otro móvil (misma IP pero otro nombre, u otra IP): nada que desalojar
        assert!(ghosts_of(&m, std::net::IpAddr::from([192, 168, 1, 10]), "Otro").is_empty());
        assert!(ghosts_of(&m, std::net::IpAddr::from([192, 168, 1, 99]), "Movil0").is_empty());
        for id in ghosts {
            m.remove(&id);
        }
        // y recupera SU plaza (la 0), no la siguiente libre
        assert_eq!(free_slot(&m, Role::Wiimote), Some(0));
    }

    #[test]
    fn slots_se_asignan_y_reutilizan() {
        let mut m = HashMap::new();
        assert_eq!(free_slot(&m, Role::Wiimote), Some(0));
        m.insert(1, sess(1, 0));
        m.insert(2, sess(2, 1));
        assert_eq!(free_slot(&m, Role::Wiimote), Some(2));
        // se va el jugador 1: su slot 0 queda libre y es el siguiente en asignarse
        m.remove(&1);
        assert_eq!(free_slot(&m, Role::Wiimote), Some(0));
        m.insert(3, sess(3, 0));
        m.insert(4, sess(4, 2));
        m.insert(5, sess(5, 3));
        assert_eq!(free_slot(&m, Role::Wiimote), None); // lleno: busy
    }
}
