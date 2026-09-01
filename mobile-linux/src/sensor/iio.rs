//! Sensores por IIO (sysfs), el estándar del kernel para el IMU de los
//! móviles Linux: PinePhone (mpu6050), Librem 5 (lsm9ds1), OnePlus 6 (bmi160),
//! Fairphone, SHIFT… Lectura por sondeo de `in_anglvel_*_raw` y
//! `in_accel_*_raw` con su escala, offset y la matriz de montaje del
//! dispositivo (la que corrige cómo está soldado el chip). Sin buffers ni
//! triggers: los `_raw` son legibles por cualquier usuario y no hace falta
//! configurar nada. La frecuencia de muestreo se sube si el sistema lo
//! permite (regla udev de packaging/linux-mobile); si no, la del driver.
//!
//! Unidades IIO: anglvel en rad/s, accel en m/s² — las de Android, sin conversión.

use super::{now_us, Sample, Source};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const SYSFS: &str = "/sys/bus/iio/devices";
/// Tope del protocolo (PROTOCOL.md §4.1).
const MAX_RATE_HZ: f32 = 250.0;
const MIN_RATE_HZ: f32 = 20.0;

type Matrix = [[f32; 3]; 3];
const IDENTITY: Matrix = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

struct Channel {
    file: File,
    scale: f32,
    offset: f32,
}

pub struct Imu {
    pub name: String,
    gyro: [Channel; 3],
    accel: [Channel; 3],
    gyro_mount: Matrix,
    accel_mount: Matrix,
    pub rate_hz: f32,
}

fn read_attr(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name))
        .ok()
        .map(|s| s.trim().to_owned())
}

fn read_f32(dir: &Path, name: &str) -> Option<f32> {
    read_attr(dir, name)?.parse().ok()
}

fn open_channel(dir: &Path, kind: &str, axis: char) -> Option<Channel> {
    let file = File::open(dir.join(format!("in_{kind}_{axis}_raw"))).ok()?;
    // escala/offset por eje o compartidos, según el driver
    let scale = read_f32(dir, &format!("in_{kind}_{axis}_scale"))
        .or_else(|| read_f32(dir, &format!("in_{kind}_scale")))
        .unwrap_or(1.0);
    let offset = read_f32(dir, &format!("in_{kind}_{axis}_offset"))
        .or_else(|| read_f32(dir, &format!("in_{kind}_offset")))
        .unwrap_or(0.0);
    Some(Channel {
        file,
        scale,
        offset,
    })
}

/// "1, 0, 0; 0, 1, 0; 0, 0, 1" → matriz 3×3.
pub fn parse_mount_matrix(s: &str) -> Option<Matrix> {
    let rows: Vec<Vec<f32>> = s
        .split(';')
        .map(|r| r.split(',').filter_map(|v| v.trim().parse().ok()).collect())
        .collect();
    if rows.len() != 3 || rows.iter().any(|r| r.len() != 3) {
        return None;
    }
    let mut m = IDENTITY;
    for (i, r) in rows.iter().enumerate() {
        m[i] = [r[0], r[1], r[2]];
    }
    Some(m)
}

fn mount_matrix(dir: &Path, kind: &str) -> Matrix {
    read_attr(dir, &format!("in_{kind}_mount_matrix"))
        .or_else(|| read_attr(dir, "mount_matrix"))
        .and_then(|s| parse_mount_matrix(&s))
        .unwrap_or(IDENTITY)
}

fn apply(m: &Matrix, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// De la lista de frecuencias del driver ("50 100 200 500" o el rango
/// "[10 1 1000]") elige la mayor que no pase del tope del protocolo.
pub fn pick_rate(available: &str) -> f32 {
    let nums: Vec<f32> = available
        .replace(['[', ']'], " ")
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect();
    if nums.is_empty() {
        return 200.0;
    }
    if available.contains('[') && nums.len() == 3 {
        let (min, step, max) = (nums[0], nums[1].max(1e-3), nums[2]);
        let mut best = min;
        let mut v = min;
        while v <= max && v <= MAX_RATE_HZ {
            best = v;
            v += step;
        }
        return best;
    }
    let ok = nums.iter().copied().filter(|v| *v <= MAX_RATE_HZ);
    ok.fold(f32::NAN, f32::max)
        .max(f32::MIN) // NaN → MIN si no había ninguna válida
        .max(if nums.iter().all(|v| *v > MAX_RATE_HZ) {
            nums.iter().copied().fold(f32::INFINITY, f32::min)
        } else {
            f32::MIN
        })
}

fn format_rate(v: f32) -> String {
    if (v - v.round()).abs() < 1e-3 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v}")
    }
}

/// Sube la frecuencia de muestreo si se puede y devuelve la efectiva.
fn configure_rate(dir: &Path, kind: &str) -> Option<f32> {
    let available = read_attr(dir, &format!("in_{kind}_sampling_frequency_available"))
        .or_else(|| read_attr(dir, "sampling_frequency_available"));
    let wanted = available.as_deref().map(pick_rate).unwrap_or(200.0);
    for name in [format!("in_{kind}_sampling_frequency"), "sampling_frequency".to_owned()] {
        let path = dir.join(&name);
        if path.exists() {
            let _ = std::fs::write(&path, format_rate(wanted)); // sin permiso: se queda la del driver
            return read_f32(dir, &name);
        }
    }
    None
}

impl Imu {
    /// Busca gyro y acelerómetro en `base` (el mismo dispositivo o dos).
    pub fn find(base: &Path) -> Result<Self, String> {
        let entries = std::fs::read_dir(base)
            .map_err(|e| format!("No hay sensores IIO en {}: {e}", base.display()))?;
        let mut gyro_dir: Option<PathBuf> = None;
        let mut accel_dir: Option<PathBuf> = None;
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs {
            if gyro_dir.is_none() && d.join("in_anglvel_x_raw").exists() {
                gyro_dir = Some(d.clone());
            }
            if accel_dir.is_none() && d.join("in_accel_x_raw").exists() {
                accel_dir = Some(d);
            }
        }
        let gyro_dir = gyro_dir.ok_or(
            "No encuentro giroscopio (in_anglvel_*) en /sys/bus/iio/devices: ¿este móvil tiene gyro y el driver cargado?",
        )?;
        let accel_dir = accel_dir.ok_or("No encuentro acelerómetro (in_accel_*) en /sys/bus/iio/devices")?;

        let ch = |dir: &Path, kind: &str| -> Result<[Channel; 3], String> {
            Ok([
                open_channel(dir, kind, 'x').ok_or(format!("in_{kind}_x_raw no legible"))?,
                open_channel(dir, kind, 'y').ok_or(format!("in_{kind}_y_raw no legible"))?,
                open_channel(dir, kind, 'z').ok_or(format!("in_{kind}_z_raw no legible"))?,
            ])
        };
        let gyro = ch(&gyro_dir, "anglvel")?;
        let accel = ch(&accel_dir, "accel")?;

        let rate = configure_rate(&gyro_dir, "anglvel");
        if accel_dir != gyro_dir {
            let _ = configure_rate(&accel_dir, "accel");
        }
        let rate_hz = rate.unwrap_or(100.0).clamp(MIN_RATE_HZ, MAX_RATE_HZ);

        Ok(Imu {
            name: read_attr(&gyro_dir, "name").unwrap_or_else(|| "iio".into()),
            gyro_mount: mount_matrix(&gyro_dir, "anglvel"),
            accel_mount: mount_matrix(&accel_dir, "accel"),
            gyro,
            accel,
            rate_hz,
        })
    }

    fn read3(ch: &mut [Channel; 3], m: &Matrix) -> Option<[f32; 3]> {
        let mut v = [0f32; 3];
        let mut s = String::with_capacity(16);
        for (i, c) in ch.iter_mut().enumerate() {
            s.clear();
            c.file.seek(SeekFrom::Start(0)).ok()?;
            c.file.read_to_string(&mut s).ok()?;
            let raw: f32 = s.trim().parse::<i64>().ok()? as f32;
            v[i] = (raw + c.offset) * c.scale;
        }
        Some(apply(m, v))
    }

    /// Una lectura completa (para tests y diagnóstico).
    pub fn read(&mut self) -> Option<Sample> {
        let gyro = Self::read3(&mut self.gyro, &self.gyro_mount)?;
        let accel = Self::read3(&mut self.accel, &self.accel_mount)?;
        Some(Sample {
            t_us: now_us(),
            gyro,
            accel,
        })
    }
}

impl Source for Imu {
    fn run(mut self: Box<Self>, tx: Sender<Sample>, stop: Arc<AtomicBool>) {
        let period = Duration::from_secs_f32(1.0 / self.rate_hz);
        let mut next = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            next += period;
            match self.read() {
                Some(s) => {
                    if tx.send(s).is_err() {
                        break;
                    }
                }
                None => std::thread::sleep(period),
            }
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now;
            }
        }
    }

    fn describe(&self) -> String {
        format!("IIO {} · {:.0} Hz", self.name, self.rate_hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un /sys/bus/iio/devices de mentira con un IMU tipo mpu6050. (El
    /// nombre real es "iio:device0"; los dos puntos no valen en Windows y
    /// `find` no depende del nombre.)
    fn fake_sysfs(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("pepomote-iio-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dev = base.join("iio-device0");
        std::fs::create_dir_all(&dev).unwrap();
        let w = |n: &str, v: &str| std::fs::write(dev.join(n), v).unwrap();
        w("name", "mpu6050\n");
        w("in_anglvel_x_raw", "1000\n");
        w("in_anglvel_y_raw", "0\n");
        w("in_anglvel_z_raw", "-500\n");
        w("in_anglvel_scale", "0.001064724\n"); // rad/s por LSB (±250°/s)
        w("in_accel_x_raw", "0\n");
        w("in_accel_y_raw", "0\n");
        w("in_accel_z_raw", "16384\n"); // 1 g a ±2 g
        w("in_accel_scale", "0.000598\n");
        w("sampling_frequency_available", "10 20 50 100 200 500\n");
        w("sampling_frequency", "50\n");
        // chip girado 90° sobre Z: x' = y, y' = -x
        w("mount_matrix", "0, 1, 0; -1, 0, 0; 0, 0, 1\n");
        base
    }

    #[test]
    fn encuentra_escala_matriz_y_frecuencia() {
        let base = fake_sysfs("find");
        let mut imu = Imu::find(&base).unwrap();
        assert_eq!(imu.name, "mpu6050");
        assert!((imu.rate_hz - 200.0).abs() < 1e-3, "sube a la mayor ≤ 250: {}", imu.rate_hz);
        assert_eq!(read_attr(&base.join("iio-device0"), "sampling_frequency").unwrap(), "200");

        let s = imu.read().unwrap();
        // gyro crudo (1000, 0, -500)·scale → (1.0647, 0, -0.5324), rotado: (0, -1.0647, -0.5324)
        assert!((s.gyro[0]).abs() < 1e-4, "{:?}", s.gyro);
        assert!((s.gyro[1] + 1.064_724).abs() < 1e-4, "{:?}", s.gyro);
        assert!((s.gyro[2] + 0.532_362).abs() < 1e-4, "{:?}", s.gyro);
        // accel: 16384 · 0.000598 = 9.797 m/s² en Z (plano sobre la mesa)
        assert!((s.accel[2] - 9.797).abs() < 0.01, "{:?}", s.accel);
        assert!(imu.describe().contains("mpu6050"));
    }

    #[test]
    fn sin_gyro_da_error_claro() {
        let base = fake_sysfs("nogyro");
        for a in ['x', 'y', 'z'] {
            std::fs::remove_file(base.join("iio-device0").join(format!("in_anglvel_{a}_raw"))).unwrap();
        }
        let err = match Imu::find(&base) {
            Err(e) => e,
            Ok(_) => panic!("sin gyro debería fallar"),
        };
        assert!(err.contains("giroscopio"), "{err}");
    }

    #[test]
    fn matriz_de_montaje() {
        let m = parse_mount_matrix("1, 0, 0; 0, 1, 0; 0, 0, 1").unwrap();
        assert_eq!(m, IDENTITY);
        assert!(parse_mount_matrix("1, 0; 0, 1").is_none());
        let m = parse_mount_matrix("0, -1, 0; 1, 0, 0; 0, 0, 1").unwrap();
        assert_eq!(apply(&m, [1.0, 2.0, 3.0]), [-2.0, 1.0, 3.0]);
    }

    #[test]
    fn eleccion_de_frecuencia() {
        assert_eq!(pick_rate("10 20 50 100 200 500"), 200.0);
        assert_eq!(pick_rate("12.5 26 52 104 208 416"), 208.0);
        assert_eq!(pick_rate("[10 10 1000]"), 250.0);
        assert_eq!(pick_rate("500 1000"), 500.0, "si todas pasan del tope, la menor");
        assert_eq!(pick_rate(""), 200.0);
    }
}
