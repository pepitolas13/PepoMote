//! Fuentes de muestras de movimiento: IIO (el móvil de verdad) o simulada
//! (pruebas sin hardware). Cada una corre su propio bucle y empuja muestras
//! por un canal; el hilo de paquetes las fusiona y las manda.

pub mod fake;
pub mod iio;

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub t_us: u64,
    pub gyro: [f32; 3],  // rad/s, ejes del dispositivo
    pub accel: [f32; 3], // m/s², con gravedad
}

pub trait Source: Send {
    /// Bucle bloqueante: emite muestras hasta que `stop` se active o el
    /// receptor del canal desaparezca.
    fn run(self: Box<Self>, tx: Sender<Sample>, stop: Arc<AtomicBool>);
    fn describe(&self) -> String;
}

pub fn open(fake: bool) -> Result<Box<dyn Source>, String> {
    if fake {
        return Ok(Box::new(fake::Fake::new()));
    }
    iio::Imu::find(Path::new(iio::SYSFS)).map(|i| Box::new(i) as Box<dyn Source>)
}

/// Reloj monotónico en µs para `t_sensor_us` (el receptor solo usa deltas).
pub fn now_us() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_micros() as u64
}
