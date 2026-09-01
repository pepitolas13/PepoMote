mod mapping;
mod server;

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

struct Pulse {
    last_recenter: Option<u8>,
    until: Instant,
}

/// El PadData se construye y envía INLINE desde el hilo de telemetría
/// (cero colas, cero hilos intermedios: la misma latencia que el puntero).
/// El hilo del servidor solo atiende las peticiones de Dolphin (1/s).
pub struct Dsu {
    socket: UdpSocket,
    clients: Arc<Mutex<HashMap<SocketAddr, Instant>>>,
    last: Arc<Mutex<Option<(MotionSample, Instant)>>>,
    counter: AtomicU32,
    pulse: Mutex<Pulse>,
}

impl Dsu {
    /// Envía la muestra a todos los clientes DSU registrados. Barato cuando
    /// no hay ninguno (un lock + return).
    pub fn push(&self, sample: &MotionSample) {
        let touch = {
            let mut p = self.pulse.lock().unwrap();
            if p.last_recenter != Some(sample.recenter_count) {
                if p.last_recenter.is_some() {
                    p.until = Instant::now() + RECENTER_PULSE;
                }
                p.last_recenter = Some(sample.recenter_count);
            }
            Instant::now() < p.until
        };

        *self.last.lock().unwrap() = Some((*sample, Instant::now()));

        let clients = self.clients.lock().unwrap();
        if clients.is_empty() {
            return;
        }
        let counter = self.counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let packet = server::pad_data_packet(sample, touch, counter, &self.last.lock().unwrap());
        for addr in clients.keys() {
            let _ = self.socket.send_to(&packet, addr);
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

    let dsu = Arc::new(Dsu {
        socket,
        clients: Arc::new(Mutex::new(HashMap::new())),
        last: Arc::new(Mutex::new(None)),
        counter: AtomicU32::new(0),
        pulse: Mutex::new(Pulse {
            last_recenter: None,
            until: Instant::now(),
        }),
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
