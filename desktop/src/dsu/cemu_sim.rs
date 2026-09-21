//! Simulación FIEL de la cadena de movimiento de Cemu, para poder ver —sin
//! móvil, sin Cemu y sin el usuario— qué orientación recibiría un juego de
//! Wii U con los bytes que PepoMote pone en el cable DSU.
//!
//! Cemu no mapea el movimiento eje por eje como Dolphin (12 bindings en
//! `dolphin.rs:43-58`): mete accel y gyro en UNA fusión Mahony propia y el
//! juego solo ve el resultado (`VPADController.cpp` → `status.acc`,
//! `status.gyroChange`, `status.gyroOrientation`). Por eso un marco que
//! Dolphin da por bueno puede salir inerte en Cemu, y por eso hace falta
//! esto: `e2e_cemu.py` manda el giroscopio a CERO y no mira nunca los bytes
//! 76-99, así que hoy nada demuestra que el giro llegue a Cemu.
//!
//! Portado línea a línea de Cemu (rama main):
//!   src/input/api/DSU/DSUControllerProvider.cpp  `integrate_motion`
//!   src/input/motion/Mahony.h                    `MahonySensorFusion`
//!   src/input/motion/MotionHandler.h             `WiiUMotionHandler`
//!   src/input/motion/MotionSample.h              `getVPAD*`
//!   src/util/math/quaternion.h                   `Assign/GetVectorZ/*/+=`
//!
//! Solo pruebas: no entra en el binario.

use super::server::pad_data_packet;
use super::{DsuProfile, MotionSample};

const DEG_TO_RAD: f32 = 0.0174533; // la constante literal de Cemu
const TAU: f32 = std::f32::consts::PI * 2.0;
const G: f32 = 9.80665;
const HZ: u32 = 250;

// ---------------------------------------------------------------------------
// util/math/quaternion.h (campos en orden w, x, y, z)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Quat {
    w: f32,
    x: f32,
    y: f32,
    z: f32,
}

impl Quat {
    fn mul(self, r: Quat) -> Quat {
        Quat {
            w: self.w * r.w - self.x * r.x - self.y * r.y - self.z * r.z,
            x: self.w * r.x + self.x * r.w + self.y * r.z - self.z * r.y,
            y: self.w * r.y - self.x * r.z + self.y * r.w + self.z * r.x,
            z: self.w * r.z + self.x * r.y - self.y * r.x + self.z * r.w,
        }
    }

    fn add(self, r: Quat) -> Quat {
        Quat { w: self.w + r.w, x: self.x + r.x, y: self.y + r.y, z: self.z + r.z }
    }

    fn normalize_xyzw(&mut self) {
        let k = 1.0 / (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        self.w *= k;
        self.x *= k;
        self.y *= k;
        self.z *= k;
    }

    /// `GetVectorZ()`: es la TRASPUESTA (el eje Z del mundo en coordenadas del
    /// cuerpo), que es justo con lo que se compara el acelerómetro.
    fn vector_z(self) -> [f32; 3] {
        [
            2.0 * (self.x * self.z - self.w * self.y),
            2.0 * (self.y * self.z + self.w * self.x),
            2.0 * (self.w * self.w + self.z * self.z) - 1.0,
        ]
    }
}

// ---------------------------------------------------------------------------
// input/motion/Mahony.h
// ---------------------------------------------------------------------------

struct Mahony {
    q: Quat,
    roll: f32,
    pitch: f32,
    yaw: f32,
    roll_wind: i32,
    pitch_wind: i32,
    yaw_wind: i32,
    bias: [f32; 3],
    sum: [f64; 3],
    count: u64,
}

impl Mahony {
    fn new() -> Self {
        // "assume default forward pose": su GetVectorZ() vale exactamente
        // (0, 1, 0), o sea el móvil PLANO con el accel de PepoMote.
        let h = 0.5f32.sqrt();
        Mahony {
            q: Quat { w: h, x: h, y: 0.0, z: 0.0 },
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
            roll_wind: 0,
            pitch_wind: 0,
            yaw_wind: 0,
            bias: [0.0; 3],
            sum: [0.0; 3],
            count: 0,
        }
    }

    fn update_gyro_bias(&mut self, g: [f32; 3]) {
        if g[0].abs() >= 0.35 || g[1].abs() >= 0.35 || g[2].abs() >= 0.35 {
            return;
        }
        for i in 0..3 {
            self.sum[i] += g[i] as f64;
        }
        self.count += 1;
        if self.count >= 200 {
            for i in 0..3 {
                self.bias[i] = (self.sum[i] / self.count as f64) as f32;
            }
        }
    }

    fn update_imu(&mut self, mut dt: f32, g: [f32; 3], a: [f32; 3]) {
        let mut av = a;
        let mut gv = g;
        if dt > 0.2 {
            dt = 0.2; // "dont let stutter mess up the internal state"
        }
        self.update_gyro_bias(g);
        for i in 0..3 {
            gv[i] -= self.bias[i];
        }
        // zona muerta: tolera el sesgo residual, sobre todo en yaw
        for v in gv.iter_mut() {
            if v.abs() < 0.015 {
                *v = 0.0;
            }
        }
        if av[0].abs() > 0.000001 || av[1].abs() > 0.000001 || av[2].abs() > 0.000001 {
            let len = (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt();
            if len != 0.0 {
                for v in av.iter_mut() {
                    *v /= len;
                }
            }
            let z = self.q.vector_z();
            let grav = [z[0] * 0.5, z[1] * 0.5, z[2] * 0.5];
            let err = [
                grav[1] * av[2] - grav[2] * av[1],
                grav[2] * av[0] - grav[0] * av[2],
                grav[0] * av[1] - grav[1] * av[0],
            ];
            for i in 0..3 {
                gv[i] -= err[i];
            }
        }
        let s = 0.5 * dt;
        for v in gv.iter_mut() {
            *v *= s;
        }
        let d = self.q.mul(Quat { w: 0.0, x: gv[0], y: gv[1], z: gv[2] });
        self.q = self.q.add(d);
        self.q.normalize_xyzw();
        self.update_orientation_angles();
    }

    fn calc_orientation(&mut self) {
        let q = self.q;
        let sinr_cosp = 2.0 * (q.z * q.w + q.x * q.y);
        let cosr_cosp = 1.0 - 2.0 * (q.w * q.w + q.x * q.x);
        self.roll = sinr_cosp.atan2(cosr_cosp);
        let sinp = 2.0 * (q.z * q.x - q.y * q.w);
        self.pitch = if sinp.abs() >= 1.0 {
            (std::f32::consts::PI / 2.0).copysign(sinp)
        } else {
            sinp.asin()
        };
        let siny_cosp = 2.0 * (q.z * q.y + q.w * q.x);
        let cosy_cosp = 1.0 - 2.0 * (q.x * q.x + q.y * q.y);
        self.yaw = siny_cosp.atan2(cosy_cosp);
    }

    fn update_orientation_angles(&mut self) {
        fn winding(prev: f32, new: f32) -> i32 {
            if new > prev {
                if new - prev > std::f32::consts::PI {
                    return -1;
                }
            } else if new < prev && prev - new > std::f32::consts::PI {
                return 1;
            }
            0
        }
        let (pr, pp, py) = (self.roll, self.pitch, self.yaw);
        self.calc_orientation();
        self.roll_wind += winding(pr, self.roll);
        self.pitch_wind += winding(pp, self.pitch);
        self.yaw_wind += winding(py, self.yaw);
    }

    fn roll_rad(&self) -> f32 {
        self.roll + self.roll_wind as f32 * TAU
    }
    fn pitch_rad(&self) -> f32 {
        self.pitch + self.pitch_wind as f32 * TAU
    }
    fn yaw_rad(&self) -> f32 {
        self.yaw + self.yaw_wind as f32 * TAU
    }
}

// ---------------------------------------------------------------------------
// input/motion/MotionHandler.h + MotionSample.h + el provider DSU
// ---------------------------------------------------------------------------

/// Lo que un juego de Wii U lee del VPAD. Orientación y giro en VUELTAS
/// (la unidad del VPAD: 1.0 = una vuelta entera al eje).
#[derive(Clone, Copy, Debug, Default)]
struct Vpad {
    orientation: [f32; 3], // [yaw, pitch, roll] con el offset de Cemu
    gyro_change: [f32; 3],
    acc: [f32; 3],
}

struct Cemu {
    last_ts: u64,
    imu: Mahony,
    gyro: [f32; 3],
    acc: [f32; 3],
    prev_acc: [f32; 3],
    orientation: [f32; 3],
    /// Paquetes que Cemu descartó por el filtro del timestamp.
    dropped: u32,
}

impl Cemu {
    fn new() -> Self {
        Cemu {
            last_ts: 0,
            imu: Mahony::new(),
            gyro: [0.0; 3],
            acc: [0.0; 3],
            prev_acc: [0.0; 3],
            orientation: [0.0; 3],
            dropped: 0,
        }
    }

    /// `DSUControllerProvider::integrate_motion`, tal cual.
    fn on_pad_data(&mut self, ts: u64, accel_g: [f32; 3], gyro_deg: [f32; 3]) {
        if ts <= self.last_ts {
            let dif = self.last_ts - ts;
            if dif >= 10_000_000 {
                self.last_ts = 0;
            }
            self.dropped += 1;
            return;
        }
        let elapsed = ts - self.last_ts;
        self.last_ts = ts;
        let dt = (elapsed as f64 / 1_000_000.0) as f32;
        let g = [gyro_deg[0] * DEG_TO_RAD, gyro_deg[1] * DEG_TO_RAD, gyro_deg[2] * DEG_TO_RAD];
        let a = [accel_g[0], -accel_g[1], -accel_g[2]];
        self.process(dt, g, a);
    }

    /// La OTRA fuente de movimiento de Cemu: un mando nativo por SDL
    /// (`SDLControllerProvider.cpp`). Es la piedra de toque: su convención
    /// está DOCUMENTADA en SDL_sensor.h (+X derecha, +Y arriba, +Z hacia el
    /// jugador; en reposo la gravedad sale +Y; giro por la regla de la mano
    /// derecha), así que lo que Cemu espera se deduce sin adivinar nada.
    ///   acc  = -data/9.81  y luego (x, -y, -z) al llamar
    ///   gyro = (x, -y, -z), en rad/s, tal cual
    fn on_sdl_sensors(&mut self, ts: u64, sdl_accel_ms2: [f32; 3], sdl_gyro_rads: [f32; 3]) {
        if ts <= self.last_ts {
            self.dropped += 1;
            return;
        }
        let elapsed = ts - self.last_ts;
        self.last_ts = ts;
        let dt = (elapsed as f64 / 1_000_000.0) as f32;
        let acc = [-sdl_accel_ms2[0] / 9.81, -sdl_accel_ms2[1] / 9.81, -sdl_accel_ms2[2] / 9.81];
        let a = [acc[0], -acc[1], -acc[2]];
        let g = [sdl_gyro_rads[0], -sdl_gyro_rads[1], -sdl_gyro_rads[2]];
        self.process(dt, g, a);
    }

    /// `WiiUMotionHandler::processMotionSample`, común a las dos vías.
    fn process(&mut self, dt: f32, g: [f32; 3], a: [f32; 3]) {
        self.gyro = g;
        self.prev_acc = self.acc;
        self.acc = a;
        self.imu.update_imu(dt, g, a);
        self.orientation[0] = -self.imu.yaw_rad() / TAU - 0.50;
        self.orientation[1] = -self.imu.pitch_rad() / TAU - 0.50;
        self.orientation[2] = self.imu.roll_rad() / TAU;
    }

    fn vpad(&self) -> Vpad {
        let mut bias_free = [0.0f32; 3];
        for i in 0..3 {
            bias_free[i] = self.gyro[i] - self.imu.bias[i];
        }
        Vpad {
            orientation: self.orientation,
            // MotionSample::getVPADGyroChange
            gyro_change: [-bias_free[0] / TAU, -bias_free[1] / TAU, bias_free[2] / TAU],
            // MotionSample::getVPADAccelerometer
            acc: [-self.acc[0], -self.acc[1], self.acc[2]],
        }
    }
}

// ---------------------------------------------------------------------------
// El móvil: genera accel y gyro de Android para un giro real
// ---------------------------------------------------------------------------

/// Postura de referencia de `mapping.rs`: móvil plano, pantalla arriba, borde
/// superior hacia la TV. Ejes Android X = derecha, Y = hacia la TV, Z = arriba.
struct Phone {
    q: Quat, // actitud cuerpo→mundo (mundo: X derecha, Y a la TV, Z arriba)
}

impl Phone {
    fn new() -> Self {
        Phone { q: Quat { w: 1.0, x: 0.0, y: 0.0, z: 0.0 } }
    }

    /// Gravedad medida (la reacción, +1 g «hacia arriba») en ejes del móvil.
    fn accel(&self) -> [f32; 3] {
        let q = self.q;
        [
            2.0 * (q.x * q.z + q.w * q.y) * G,
            2.0 * (q.y * q.z - q.w * q.x) * G,
            (1.0 - 2.0 * (q.x * q.x + q.y * q.y)) * G,
        ]
    }

    fn advance(&mut self, w: [f32; 3], dt: f32) {
        let d = self.q.mul(Quat { w: 0.0, x: w[0] * 0.5 * dt, y: w[1] * 0.5 * dt, z: w[2] * 0.5 * dt });
        self.q = self.q.add(d);
        self.q.normalize_xyzw();
    }
}

/// Un tramo del guion: velocidad angular (rad/s, ejes del móvil) y duración.
struct Leg {
    w: [f32; 3],
    secs: f32,
}

/// Por dónde entra el movimiento en Cemu.
#[derive(Clone, Copy, PartialEq)]
enum Via {
    /// PepoMote: móvil → `pad_data_packet` (bytes de verdad) → DSU.
    PepoMote,
    /// Un mando nativo (DS4/DS5/Switch Pro) por SDL, sin pasar por el DSU.
    MandoNativo,
}

/// Pasa el guion por TODA la cadena real y devuelve el VPAD final.
fn run_via(legs: &[Leg], via: Via) -> (Vpad, Cemu) {
    let mut phone = Phone::new();
    let mut cemu = Cemu::new();
    let dt = 1.0 / HZ as f32;
    // Uptime realista del móvil (20 h en µs): ejercita el timestamp absoluto
    let mut t_us: u64 = 72_000_000_000;
    let mut counter = 0u32;
    for leg in legs {
        let steps = (leg.secs * HZ as f32).round() as u32;
        for _ in 0..steps {
            let sample = MotionSample {
                t_us,
                accel_ms2: phone.accel(),
                gyro_rads: leg.w,
                buttons: 0,
                battery_pct: 100,
                recenter_count: 0,
                stick_x: 0,
                stick_y: 0,
                stick_rx: 0,
                stick_ry: 0,
                touch: None,
                wii_ir: None,
                profile: DsuProfile::WiiU,
            };
            match via {
                Via::PepoMote => {
                    let p = pad_data_packet(0, &sample, false, counter);
                    let f = |o: usize| f32::from_le_bytes(p[o..o + 4].try_into().unwrap());
                    let ts = u64::from_le_bytes(p[68..76].try_into().unwrap());
                    cemu.on_pad_data(ts, [f(76), f(80), f(84)], [f(88), f(92), f(96)]);
                }
                Via::MandoNativo => {
                    // Mismo movimiento físico, en los ejes que documenta SDL:
                    // x = derecha = X del móvil, y = arriba = Z del móvil,
                    // z = hacia el jugador = -Y del móvil.
                    let a = sample.accel_ms2;
                    let g = sample.gyro_rads;
                    cemu.on_sdl_sensors(sample.t_us, [a[0], a[2], -a[1]], [g[0], g[2], -g[1]]);
                }
            }
            phone.advance(leg.w, dt);
            t_us += (1_000_000 / HZ) as u64;
            counter = counter.wrapping_add(1);
        }
    }
    (cemu.vpad(), cemu)
}

/// Quieto el tiempo suficiente para que la fusión se asiente y el estimador de
/// sesgo de Cemu arranque (necesita 200 muestras).
fn run(legs: &[Leg]) -> (Vpad, Cemu) {
    run_via(legs, Via::PepoMote)
}

fn settle() -> Leg {
    Leg { w: [0.0, 0.0, 0.0], secs: 2.0 }
}

fn rev(delta: f32) -> f32 {
    // vueltas → grados, para leerlo a ojo
    delta * 360.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El móvil quieto: la orientación no puede irse sola ni salir NaN.
    #[test]
    fn quieto_no_deriva_ni_explota() {
        let (v, cemu) = run(&[settle(), settle()]);
        assert_eq!(cemu.dropped, 0, "Cemu no debe descartar ningún paquete");
        for (i, o) in v.orientation.iter().enumerate() {
            assert!(o.is_finite(), "orientación {i} no finita: {o}");
        }
        println!("QUIETO  orientación = {:?}", v.orientation);
    }

    /// El experimento: 90° de yaw, 45° de pitch y 45° de roll, uno por uno.
    /// Imprime lo que vería el juego. Con `-- --nocapture`.
    #[test]
    fn giros_reales_llegan_al_vpad() {
        // 90 °/s = 1.5708 rad/s: por encima del umbral de sesgo (0.35) y de la
        // zona muerta (0.015), o sea un giro de mano normal
        let w90 = 90.0f32.to_radians();
        let base = run(&[settle()]).0.orientation;

        let casos: [(&str, [f32; 3], f32); 3] = [
            ("YAW   90° (girar en horizontal)", [0.0, 0.0, w90], 1.0),
            ("PITCH 45° (levantar el morro)", [w90, 0.0, 0.0], 0.5),
            ("ROLL  45° (inclinar de lado)", [0.0, w90, 0.0], 0.5),
        ];

        println!("\n  base (quieto) = [yaw {:.3} pitch {:.3} roll {:.3}] vueltas", base[0], base[1], base[2]);
        for (nombre, w, secs) in casos {
            let (v, _) = run(&[settle(), Leg { w, secs }]);
            let d = [v.orientation[0] - base[0], v.orientation[1] - base[1], v.orientation[2] - base[2]];
            println!(
                "  {nombre}\n     Δ VPAD  yaw {:+7.1}°  pitch {:+7.1}°  roll {:+7.1}°   |  gyroChange {:+.4} {:+.4} {:+.4}",
                rev(d[0]), rev(d[1]), rev(d[2]),
                v.gyro_change[0], v.gyro_change[1], v.gyro_change[2]
            );
            let movido = d.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
            assert!(
                movido > 0.02,
                "{nombre}: el VPAD apenas se movió ({:.1}°). Cemu no está viendo el giro.",
                rev(movido)
            );
        }
    }

    /// LA PRUEBA DE FONDO: el mismo movimiento físico entrando por el cable de
    /// PepoMote y por un mando nativo de SDL tiene que dar A UN JUEGO DE WII U
    /// exactamente lo mismo. Si coincide, el marco de ejes de PepoMote es el
    /// que Cemu espera, y cualquier «el giro no va» es de configuración o del
    /// juego, no del cable.
    #[test]
    fn pepomote_es_indistinguible_de_un_mando_nativo() {
        let w = 90.0f32.to_radians();
        let guiones: [(&str, [f32; 3]); 4] = [
            ("quieto", [0.0, 0.0, 0.0]),
            ("yaw", [0.0, 0.0, w]),
            ("pitch", [w, 0.0, 0.0]),
            ("roll", [0.0, w, 0.0]),
        ];
        println!();
        for (nombre, mov) in guiones {
            let legs = [settle(), Leg { w: mov, secs: 0.5 }];
            let mio = run_via(&legs, Via::PepoMote).0;
            let nativo = run_via(&legs, Via::MandoNativo).0;
            println!(
                "  {nombre:7} PepoMote [{:+.4} {:+.4} {:+.4}]  mando nativo [{:+.4} {:+.4} {:+.4}]",
                mio.orientation[0], mio.orientation[1], mio.orientation[2],
                nativo.orientation[0], nativo.orientation[1], nativo.orientation[2]
            );
            for i in 0..3 {
                let d = (mio.orientation[i] - nativo.orientation[i]).abs();
                assert!(
                    d < 0.002,
                    "{nombre}: el eje {i} de la orientación difiere de un mando nativo en {:.4} vueltas ({:.1}°)",
                    d, rev(d)
                );
                let dg = (mio.gyro_change[i] - nativo.gyro_change[i]).abs();
                assert!(dg < 0.002, "{nombre}: gyroChange eje {i} difiere en {dg:.4} vueltas/s");
                let da = (mio.acc[i] - nativo.acc[i]).abs();
                assert!(da < 0.002, "{nombre}: acelerómetro eje {i} difiere en {da:.4} g");
            }
        }
    }
}
