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

/// Todo lo que el sistema expone que huela a sensor de movimiento: IIO,
/// dispositivos `input` con nombre de gyro/accel, y si iio-sensor-proxy está
/// corriendo (en modo buffer bloquea las lecturas _raw del dispositivo).
pub fn inventory() -> String {
    let mut out = String::from("IIO (/sys/bus/iio/devices):\n");
    out.push_str(&iio::inventory(Path::new(iio::SYSFS)));
    let mut inputs: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/sys/class/input") {
        for e in rd.flatten() {
            let p = e.path();
            if !p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("input")) {
                continue;
            }
            if let Ok(name) = std::fs::read_to_string(p.join("name")) {
                let n = name.trim();
                let l = n.to_lowercase();
                if l.contains("gyro") || l.contains("accel") || l.contains("imu") || l.contains("sensor") {
                    inputs.push(format!("{}: {n}", p.file_name().unwrap().to_string_lossy()));
                }
            }
        }
    }
    out.push_str("input (/sys/class/input): ");
    if inputs.is_empty() {
        out.push_str("ninguno con nombre de sensor\n");
    } else {
        out.push('\n');
        for i in inputs {
            out.push_str("  ");
            out.push_str(&i);
            out.push('\n');
        }
    }
    let proxy = std::fs::read_dir("/proc")
        .map(|rd| {
            rd.flatten().any(|e| {
                std::fs::read_to_string(e.path().join("comm"))
                    .map(|c| c.trim().starts_with("iio-sensor-prox"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    out.push_str(if proxy { "iio-sensor-proxy: corriendo\n" } else { "iio-sensor-proxy: no\n" });
    out
}

/// Reloj monotónico en µs para `t_sensor_us` (el receptor solo usa deltas).
pub fn now_us() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_micros() as u64
}
