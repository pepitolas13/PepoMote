#[cfg(test)]
mod cemu_sim;
mod mapping;
mod server;
pub mod wii_ir;

use crate::state::LockTolerant;
use crate::net::MAX_PLAYERS;
use crate::state::SharedState;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Qué lee el PadData: el perfil Wii (Dolphin: Home→PS, Touch = pulso de
/// recentrado), el Wii U (Cemu: Home→Touch, gatillos, stick derecho, táctil)
/// o el Switch (Eden: letras en el orden de su tabla, Home→PS, Capturar→Touch,
/// giroscopio en su escala, sticks centrados en 127).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DsuProfile {
    #[default]
    Wii,
    WiiU,
    Switch,
}

impl DsuProfile {
    /// Factor del giroscopio en el cable. Eden divide el valor DSU entre 312
    /// y lo trata como vueltas/s (yuzu lo calibró contra BetterJoy): para que
    /// una vuelta sea una vuelta hay que mandar °/s × 312/360. Dolphin y Cemu
    /// leen °/s tal cual.
    pub fn gyro_scale(self) -> f32 {
        match self {
            DsuProfile::Switch => 312.0 / 360.0,
            _ => 1.0,
        }
    }

    /// Centro de los sticks en el cable: 128 (DS4) para Dolphin y Cemu; Eden
    /// calcula `(v − 127) / 127`, así que con 127 el reposo es 0 exacto.
    pub fn stick_center(self) -> i32 {
        match self {
            DsuProfile::Switch => 127,
            _ => 128,
        }
    }
}

/// Puntero IR del perfil Wii (Dolphin ≥ 2407, grupo `IRPassthrough`): el
/// receptor genera los dos puntos de la barra sensora y los codifica en los
/// bytes DSU que el perfil Wii deja libres (protocol/DSU.md, «Puntero IR»).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WiiIr {
    /// Punto medio del par en la cámara, 0..32767 sobre 1023 × 767 (X en
    /// espejo, como la cámara real: apuntar a la derecha = X baja).
    pub x: u16,
    pub y: u16,
    /// Nivel de distancia 0..3 (2,0 / 1,3 / 0,85 / 0,55 m): L3 = bit 0, R3 = bit 1.
    pub level: u8,
    /// Roll del mando en Left X (byte 128..255 = −90°..+90°); `None` cuando
    /// ese stick lleva el Nunchuk del propio móvil (el par va horizontal).
    pub roll: Option<u8>,
}

/// Muestra de movimiento que la telemetría empuja al DSU.
#[derive(Clone, Copy)]
pub struct MotionSample {
    pub t_us: u64,
    pub accel_ms2: [f32; 3],
    pub gyro_rads: [f32; 3],
    pub buttons: u32,
    pub battery_pct: u8,
    pub recenter_count: u8,
    /// Stick del Nunchuk o izquierdo del GamePad (−127..127, +X derecha,
    /// +Y arriba); 0,0 en un Wiimote.
    pub stick_x: i8,
    pub stick_y: i8,
    /// Stick derecho del GamePad; 0,0 si no hay.
    pub stick_rx: i8,
    pub stick_ry: i8,
    /// Toque en el touchpad DSU (x 0..1920, y 0..942, origen arriba-izquierda):
    /// la pantalla táctil del GamePad o el puntero IR de un Mando Wii en Cemu.
    pub touch: Option<(u16, u16)>,
    /// Perfil Wii: los puntos IR que genera el receptor; `None` = fuera de
    /// pantalla o sin apuntado absoluto (en el cable, centinela Right X = 0).
    pub wii_ir: Option<WiiIr>,
    pub profile: DsuProfile,
}

/// Duración del pulso del botón Touch al recentrar (IMUIR/Recenter).
const RECENTER_PULSE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy)]
struct Pulse {
    last_recenter: Option<u8>,
    until: Instant,
}

pub type SlotSamples = Arc<Mutex<[Option<(MotionSample, Instant)>; MAX_PLAYERS]>>;

/// Clientes con registro caducable: Dolphin re-pide cada 1 s; Cemu vuelve a
/// pedir cada pad en cuanto recibe su PadData.
pub(crate) const CLIENT_TTL: Duration = Duration::from_secs(3);

/// Un cliente DSU registrado y QUÉ slots pidió. Dolphin abre un socket por
/// mando y se suscribe solo a su slot (al recibir no filtra: si se le mandan
/// todos los slots, cada Wiimote se mueve con todos los móviles). Cemu usa UN
/// socket para todos sus mandos y pide cada pad por separado, uno tras otro
/// (y en cada respuesta vuelve a pedir ESE pad): las peticiones se ACUMULAN,
/// cada slot con su propia caducidad. Si una petición sustituyera a la
/// anterior, solo llegaría el último pad pedido y el otro mando saldría
/// desconectado en el juego (dos Mandos Wii en Cemu: solo uno funcionaba).
#[derive(Clone, Copy)]
pub struct Client {
    /// Última petición que incluía cada slot.
    pub slot_seen: [Option<Instant>; MAX_PLAYERS],
}

impl Client {
    pub fn new() -> Self {
        Client { slot_seen: [None; MAX_PLAYERS] }
    }

    /// Registra ahora los slots de `mask` (bit i = slot i), sin olvidar los demás.
    pub fn register(&mut self, mask: u8, now: Instant) {
        for (i, seen) in self.slot_seen.iter_mut().enumerate() {
            if mask & (1 << i) != 0 {
                *seen = Some(now);
            }
        }
    }

    /// Quiere ese slot: lo pidió hace menos de `ttl`.
    pub fn wants(&self, slot: usize, ttl: Duration, now: Instant) -> bool {
        self.slot_seen.get(slot).copied().flatten().is_some_and(|t| now.duration_since(t) < ttl)
    }

    /// Sigue vivo: alguna petición hace menos de `ttl`.
    pub fn alive(&self, ttl: Duration, now: Instant) -> bool {
        self.slot_seen.iter().flatten().any(|t| now.duration_since(*t) < ttl)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

pub type Clients = Arc<Mutex<HashMap<SocketAddr, Client>>>;

/// El PadData se construye y envía INLINE desde el hilo de telemetría
/// (cero colas: la misma latencia que el puntero). Cada jugador va en su
/// slot DSU (0..3) con su propia MAC y su propio pulso de recentrado, y solo
/// a los clientes suscritos a ese slot.
/// El hilo del servidor solo atiende las peticiones de Dolphin (1/s).
pub struct Dsu {
    socket: UdpSocket,
    clients: Clients,
    last: SlotSamples,
    /// Compartido con el hilo del servidor, que tambien manda PadData
    /// (el latido de los slots callados): el contador tiene que seguir
    /// creciendo o el emulador descarta el paquete.
    counter: Arc<AtomicU32>,
    pulse: Mutex<[Pulse; MAX_PLAYERS]>,
}

impl Dsu {
    /// Envía la muestra del slot a todos los clientes DSU registrados.
    pub fn push(&self, slot: u8, sample: &MotionSample) {
        let slot_idx = slot as usize;
        if slot_idx >= MAX_PLAYERS {
            return;
        }

        let touch = {
            let mut pulses = self.pulse.lock_tolerant();
            let p = &mut pulses[slot_idx];
            if p.last_recenter != Some(sample.recenter_count) {
                if p.last_recenter.is_some() {
                    p.until = Instant::now() + RECENTER_PULSE;
                }
                p.last_recenter = Some(sample.recenter_count);
            }
            Instant::now() < p.until
        };

        self.last.lock_tolerant()[slot_idx] = Some((*sample, Instant::now()));

        let clients = self.clients.lock_tolerant();
        let now = Instant::now();
        if !clients.values().any(|c| c.wants(slot_idx, CLIENT_TTL, now)) {
            return;
        }
        let counter = self.counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let packet = server::pad_data_packet(slot, sample, touch, counter);
        for (addr, c) in clients.iter() {
            if c.wants(slot_idx, CLIENT_TTL, now) {
                let _ = self.socket.send_to(&packet, addr);
            }
        }
    }
}

/// Puerto del servidor DSU (26760). `PEPOMOTE_DSU_PORT` lo cambia, solo para
/// los e2e en el propio equipo: un receptor de prueba junto al real.
pub fn port() -> u16 {
    std::env::var("PEPOMOTE_DSU_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(server::DSU_PORT)
}

pub fn start(shared: SharedState) -> Option<Arc<Dsu>> {
    let port = port();
    // Otro servidor DSU (DS4Windows, BetterJoy, un PepoMote colgado) en el
    // mismo puerto: se le cierra y se recupera el puerto
    let socket = match crate::ports::bind_udp(&shared, "127.0.0.1", port, "DSU") {
        Ok(s) => s,
        Err(e) => {
            // Aquí no se insiste (esto corre en el hilo principal, antes de
            // la ventana): la espera de `bind_udp` cubre el puerto que suelta
            // un receptor recién cerrado; si sigue ocupado, se dice (también
            // en el log) y el DSU queda apagado hasta reabrir PepoMote
            let e = format!("{e} {}", crate::tr!("port.reopen"));
            crate::log_line!("{e}");
            shared.lock_tolerant().last_error = Some(e);
            return None;
        }
    };
    let recv_socket = match socket.try_clone() {
        Ok(s) => s,
        Err(e) => {
            shared.lock_tolerant().last_error = Some(format!("DSU: {e}"));
            return None;
        }
    };

    let idle = Pulse {
        last_recenter: None,
        until: Instant::now(),
    };
    let dsu = Arc::new(Dsu {
        socket,
        clients: Arc::new(Mutex::new(HashMap::new())),
        last: Arc::new(Mutex::new([None; MAX_PLAYERS])),
        counter: Arc::new(AtomicU32::new(0)),
        pulse: Mutex::new([idle; MAX_PLAYERS]),
    });

    {
        let clients = dsu.clients.clone();
        let last = dsu.last.clone();
        let counter = dsu.counter.clone();
        std::thread::Builder::new()
            .name("pmp-dsu".into())
            .spawn(move || server::run(shared, recv_socket, clients, last, counter))
            .expect("hilo dsu");
    }
    Some(dsu)
}
