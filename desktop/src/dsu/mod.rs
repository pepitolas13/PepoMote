mod mapping;
mod server;

use crate::state::SharedState;
use std::sync::mpsc::{sync_channel, SyncSender};

/// Muestra de movimiento que la telemetría entrega al servidor DSU.
#[derive(Clone, Copy)]
pub struct MotionSample {
    pub t_us: u64,
    pub accel_ms2: [f32; 3],
    pub gyro_rads: [f32; 3],
    pub buttons: u32,
    pub battery_pct: u8,
    pub recenter_count: u8,
}

/// Arranca el servidor DSU en su hilo y devuelve el canal de muestras.
/// El canal es acotado y la telemetría usa try_send: si DSU se atasca,
/// se tiran muestras — jamás se bloquea el hot path del puntero.
pub fn start(shared: SharedState) -> SyncSender<MotionSample> {
    let (tx, rx) = sync_channel::<MotionSample>(256);
    std::thread::Builder::new()
        .name("pmp-dsu".into())
        .spawn(move || server::run(shared, rx))
        .expect("hilo dsu");
    tx
}
