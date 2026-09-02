//! Fuentes de muestras de movimiento: IIO (PinePhone, Librem 5, …),
//! Qualcomm SSC/SLPI (SDM845 y posteriores: OnePlus 6/6T, SHIFT6mq, Poco
//! F1, …) o simulada (pruebas sin hardware). Cada una corre su propio bucle
//! y empuja muestras por un canal; el hilo de paquetes las fusiona y las manda.

pub mod fake;
pub mod iio;
pub mod ssc;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

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
    // 1) IIO (el kernel expone el IMU); 2) Qualcomm SSC/SLPI (el IMU cuelga
    // del DSP de sensores y Linux no ve nada en IIO)
    let iio_err = match iio::Imu::find(Path::new(iio::SYSFS)) {
        Ok(i) => return Ok(Box::new(i)),
        Err(e) => e,
    };
    match ssc::Ssc::open() {
        Ok(s) => Ok(Box::new(s)),
        Err(ssc_err) => Err(format!("IIO: {iio_err}\nSSC: {ssc_err}")),
    }
}

/// Fuente con la calibración de ejes aplicada (signos por eje, ver `calib`).
pub struct Corrected {
    inner: Box<dyn Source>,
    axes: crate::calib::Axes,
}

impl Source for Corrected {
    fn run(self: Box<Self>, tx: Sender<Sample>, stop: Arc<AtomicBool>) {
        let Corrected { inner, axes } = *self;
        let (itx, irx) = std::sync::mpsc::channel();
        let inner_stop = Arc::new(AtomicBool::new(false));
        let inner_stop2 = inner_stop.clone();
        let worker = std::thread::spawn(move || inner.run(itx, inner_stop2));
        while !stop.load(Ordering::Relaxed) {
            match irx.recv_timeout(Duration::from_millis(200)) {
                Ok(s) => {
                    if tx.send(axes.apply(s)).is_err() {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        inner_stop.store(true, Ordering::Relaxed);
        let _ = worker.join();
    }

    fn describe(&self) -> String {
        format!("{} · ejes {}", self.inner.describe(), self.axes.describe())
    }
}

/// `open` más la calibración de ejes guardada (si la hay y no es identidad).
pub fn open_corrected(fake: bool) -> Result<Box<dyn Source>, String> {
    let inner = open(fake)?;
    Ok(match crate::store::load_axes() {
        Some(axes) if !axes.is_identity() => Box::new(Corrected { inner, axes }),
        _ => inner,
    })
}

/// Todo lo que el sistema expone que huela a sensor de movimiento: IIO,
/// dispositivos `input` con nombre de gyro/accel, si iio-sensor-proxy está
/// corriendo (en modo buffer bloquea las lecturas _raw del dispositivo) y el
/// resultado de sondear el SSC de Qualcomm.
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
    out.push_str(&ssc::probe());
    out.push('\n');
    out
}

/// Lee `dur` de muestras de una fuente ya abierta y resume: nº de muestras,
/// frecuencia real y medias de accel (gravedad: plano boca arriba => z ≈ +9.8)
/// y gyro (en reposo ≈ 0). Diagnóstico de `--sensors`.
pub fn sample_summary(src: Box<dyn Source>, dur: std::time::Duration) -> String {
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::{channel, RecvTimeoutError};
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let worker = std::thread::spawn(move || src.run(tx, stop2));
    let deadline = Instant::now() + dur;
    let (mut n, mut acc, mut gyr) = (0u32, [0f64; 3], [0f64; 3]);
    let (mut t_first, mut t_last) = (0u64, 0u64);
    // entrega: ¿llegan de una en una o en ráfagas (batching del DSP)?
    let (mut last_arrival, mut max_gap_ms, mut burst) = (None::<Instant>, 0f64, 0u32);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match rx.recv_timeout(left) {
            Ok(smp) => {
                if let Some(p) = last_arrival {
                    let g = p.elapsed().as_secs_f64() * 1e3;
                    max_gap_ms = max_gap_ms.max(g);
                    if g < 0.3 {
                        burst += 1;
                    }
                }
                last_arrival = Some(Instant::now());
                if n == 0 {
                    t_first = smp.t_us;
                }
                t_last = smp.t_us;
                n += 1;
                for i in 0..3 {
                    acc[i] += smp.accel[i] as f64;
                    gyr[i] += smp.gyro[i] as f64;
                }
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    stop.store(true, Ordering::Relaxed);
    drop(rx);
    let _ = worker.join();
    if n == 0 {
        return format!("muestras en {:.1} s: NINGUNA (la fuente abre pero no entrega datos)", dur.as_secs_f32());
    }
    let span_s = if n > 1 { (t_last.saturating_sub(t_first)) as f64 / 1e6 } else { 0.0 };
    let hz = if span_s > 0.0 { (n - 1) as f64 / span_s } else { 0.0 };
    let k = n as f64;
    let burst_pct = if n > 1 { burst * 100 / (n - 1) } else { 0 };
    format!(
        "muestras en {:.1} s: {n} ({hz:.0} Hz según sus timestamps) · entrega: hueco máx {max_gap_ms:.0} ms, {burst_pct}% en ráfaga · accel media [{:.2} {:.2} {:.2}] m/s² (|g|={:.2}) · gyro media [{:.3} {:.3} {:.3}] rad/s",
        dur.as_secs_f32(),
        acc[0] / k,
        acc[1] / k,
        acc[2] / k,
        ((acc[0] / k).powi(2) + (acc[1] / k).powi(2) + (acc[2] / k).powi(2)).sqrt(),
        gyr[0] / k,
        gyr[1] / k,
        gyr[2] / k
    )
}

/// Reloj monotónico en µs para `t_sensor_us` (el receptor solo usa deltas).
pub fn now_us() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumen_de_muestras_con_fuente_simulada() {
        let out = sample_summary(Box::new(fake::Fake::new()), std::time::Duration::from_millis(300));
        assert!(out.starts_with("muestras en 0.3 s: "), "{out}");
        assert!(!out.contains("NINGUNA"), "{out}");
        assert!(out.contains("Hz"), "{out}");
    }
}
