mod mapping;
mod server;

use crate::net::MAX_PLAYERS;
use crate::state::SharedState;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Muestra de movimiento que la telemetría empuja al DSU.
#[derive(Clone, Copy)]
pub struct MotionSample {
    pub t_us: u64,
    pub accel_ms2: [f32; 3],
    pub gyro_rads: [f32; 3],
    pub buttons: u32,
    pub battery_pct: u8,
    pub recenter_count: u8,
}

/// Duración del pulso del botón Touch al recentrar (IMUPointer/Recenter).
const RECENTER_PULSE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy)]
struct Pulse {
    last_recenter: Option<u8>,
    until: Instant,
}

pub type SlotSamples = Arc<Mutex<[Option<(MotionSample, Instant)>; MAX_PLAYERS]>>;

/// Un cliente DSU registrado y QUÉ slots pidió (bitmask, bit i = slot i).
/// Dolphin abre un socket por mando y se suscribe solo a su slot; al recibir
/// no filtra: se queda con el último PadData que entre por ese socket. Si se
/// le mandan todos los slots, cada Wiimote se mueve con todos los móviles.
#[derive(Clone, Copy)]
pub struct Client {
    pub last_seen: Instant,
    pub slots: u8,
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
    counter: AtomicU32,
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
            let mut pulses = self.pulse.lock().unwrap();
            let p = &mut pulses[slot_idx];
            if p.last_recenter != Some(sample.recenter_count) {
                if p.last_recenter.is_some() {
                    p.until = Instant::now() + RECENTER_PULSE;
                }
                p.last_recenter = Some(sample.recenter_count);
            }
            Instant::now() < p.until
        };

        self.last.lock().unwrap()[slot_idx] = Some((*sample, Instant::now()));

        let clients = self.clients.lock().unwrap();
        let slot_bit = 1u8 << slot;
        if !clients.values().any(|c| c.slots & slot_bit != 0) {
            return;
        }
        let counter = self.counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let packet = server::pad_data_packet(slot, sample, touch, counter);
        for (addr, c) in clients.iter() {
            if c.slots & slot_bit != 0 {
                let _ = self.socket.send_to(&packet, addr);
            }
        }
    }
}

pub fn start(shared: SharedState) -> Option<Arc<Dsu>> {
    let socket = match UdpSocket::bind(("127.0.0.1", server::DSU_PORT)) {
        Ok(s) => s,
        Err(e) => {
            shared.lock().unwrap().last_error =
                Some(format!("DSU: no puedo escuchar en {}: {e}", server::DSU_PORT));
            return None;
        }
    };
    let recv_socket = match socket.try_clone() {
        Ok(s) => s,
        Err(e) => {
            shared.lock().unwrap().last_error = Some(format!("DSU: {e}"));
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
        counter: AtomicU32::new(0),
        pulse: Mutex::new([idle; MAX_PLAYERS]),
    });

    {
        let clients = dsu.clients.clone();
        let last = dsu.last.clone();
        std::thread::Builder::new()
            .name("pmp-dsu".into())
            .spawn(move || server::run(shared, recv_socket, clients, last))
            .expect("hilo dsu");
    }
    Some(dsu)
}
