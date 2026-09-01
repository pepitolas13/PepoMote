//! Sensor simulado (`--fake-sensors`): un móvil que oscila lentamente en yaw
//! y pitch, con la gravedad coherente. Sirve para probar red, protocolo y UI
//! en un PC sin IMU; el receptor ve moverse el cursor.

use super::{now_us, Sample, Source};
use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

const RATE_HZ: f32 = 200.0;
const G: f32 = 9.80665;

pub struct Fake;

impl Fake {
    pub fn new() -> Self {
        Self
    }
}

impl Source for Fake {
    fn run(self: Box<Self>, tx: Sender<Sample>, stop: Arc<AtomicBool>) {
        let period = Duration::from_secs_f32(1.0 / RATE_HZ);
        let start = Instant::now();
        let mut next = start;
        let mut prev = (0.0f32, 0.0f32);
        while !stop.load(Ordering::Relaxed) {
            next += period;
            let t = start.elapsed().as_secs_f32();
            // yaw ±15° cada 6 s, pitch ±5° cada 4 s
            let yaw = 15f32.to_radians() * (TAU * t / 6.0).sin();
            let pitch = 5f32.to_radians() * (TAU * t / 4.0).sin();
            let dt = 1.0 / RATE_HZ;
            let gyro = [(pitch - prev.1) / dt, 0.0, (yaw - prev.0) / dt];
            prev = (yaw, pitch);
            let accel = [0.0, G * pitch.sin(), G * pitch.cos()];
            if tx
                .send(Sample {
                    t_us: now_us(),
                    gyro,
                    accel,
                })
                .is_err()
            {
                break;
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
        format!("simulado · {RATE_HZ:.0} Hz")
    }
}
