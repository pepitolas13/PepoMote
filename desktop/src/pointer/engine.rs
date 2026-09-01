//! Motor de puntero h2: proyección angular absoluta del quaternion
//! (GAME_ROTATION_VECTOR) contra una referencia de recentrado, con filtro
//! one-euro y deadzone con histéresis. Fallback: integración relativa del
//! gyro para móviles sin rotation vector (flags bit0 = 0).

use super::one_euro::OneEuro;
use crate::net::codec::{InputPacket, FLAG_QUAT_VALID};

/// Deadzone angular con histéresis: por debajo de este ángulo respecto a la
/// última posición emitida, el cursor no se mueve.
const DEADZONE_DEG: f32 = 0.08;

/// Signos del fallback relativo (h1). Corrección SOLO aquí.
const SIGN_X: f32 = -1.0;
const SIGN_Y: f32 = -1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerOutput {
    /// Coordenadas normalizadas 0..1 sobre la pantalla primaria.
    Abs { nx: f32, ny: f32 },
    /// Deltas de píxel (fallback sin quaternion o modo relativo).
    Rel { dx: i32, dy: i32 },
    None,
}

#[derive(Clone, Copy)]
struct Quat {
    w: f32,
    x: f32,
    y: f32,
    z: f32,
}

impl Quat {
    fn from_packet(p: &InputPacket) -> Self {
        let q = Self {
            w: p.quat[0],
            x: p.quat[1],
            y: p.quat[2],
            z: p.quat[3],
        };
        q.normalized()
    }

    fn normalized(self) -> Self {
        let n = (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if n < 1e-6 {
            return Self { w: 1.0, x: 0.0, y: 0.0, z: 0.0 };
        }
        Self { w: self.w / n, x: self.x / n, y: self.y / n, z: self.z / n }
    }

    fn conj(self) -> Self {
        Self { w: self.w, x: -self.x, y: -self.y, z: -self.z }
    }

    fn mul(self, o: Self) -> Self {
        Self {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }

    /// Rota el eje de apuntado del dispositivo (0,1,0) por este quaternion.
    /// (Columna Y de la matriz de rotación.)
    fn rotate_pointing_axis(self) -> [f32; 3] {
        let (w, x, y, z) = (self.w, self.x, self.y, self.z);
        [
            2.0 * (x * y - w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + w * x),
        ]
    }
}

pub struct PointerEngine {
    q_ref: Option<Quat>,
    last_recenter: Option<u8>,
    f_yaw: OneEuro,
    f_pitch: OneEuro,
    last_emitted: Option<(f32, f32)>, // grados (yaw, pitch)
    last_t_us: Option<u64>,
    // fallback relativo
    acc_x: f32,
    acc_y: f32,
}

impl PointerEngine {
    pub fn new() -> Self {
        Self {
            q_ref: None,
            last_recenter: None,
            f_yaw: OneEuro::new(1.0, 0.02),
            f_pitch: OneEuro::new(1.0, 0.02),
            last_emitted: None,
            last_t_us: None,
            acc_x: 0.0,
            acc_y: 0.0,
        }
    }

    /// `sens_deg`: grados de giro para cruzar el ancho de pantalla.
    /// `aspect_w_over_h`: relación de aspecto de la pantalla destino.
    /// `abs_mode`: false = forzar salida relativa (juegos).
    pub fn apply(
        &mut self,
        p: &InputPacket,
        sens_deg: f32,
        aspect_w_over_h: f32,
        abs_mode: bool,
        screen_w_px: f32,
    ) -> PointerOutput {
        let dt = self.compute_dt(p.t_sensor_us);

        // Recentrado: flanco del contador (o primera muestra)
        let recentered = self.last_recenter != Some(p.recenter_count);
        self.last_recenter = Some(p.recenter_count);

        if p.flags & FLAG_QUAT_VALID == 0 {
            // Fallback h1: integración relativa del gyro
            let Some(dt) = dt else { return PointerOutput::None };
            let px_per_rad = screen_w_px / sens_deg.to_radians();
            self.acc_x += SIGN_X * p.gyro[2] * dt * px_per_rad;
            self.acc_y += SIGN_Y * p.gyro[0] * dt * px_per_rad;
            let dx = self.acc_x as i32;
            let dy = self.acc_y as i32;
            self.acc_x -= dx as f32;
            self.acc_y -= dy as f32;
            return if dx != 0 || dy != 0 {
                PointerOutput::Rel { dx, dy }
            } else {
                PointerOutput::None
            };
        }

        let q = Quat::from_packet(p);
        if recentered || self.q_ref.is_none() {
            self.q_ref = Some(q);
            self.f_yaw.reset();
            self.f_pitch.reset();
            self.last_emitted = Some((0.0, 0.0));
            return if abs_mode {
                PointerOutput::Abs { nx: 0.5, ny: 0.5 }
            } else {
                PointerOutput::None
            };
        }

        let q_rel = self.q_ref.unwrap().conj().mul(q);
        let v = q_rel.rotate_pointing_axis();
        let yaw = v[0].atan2(v[1]).to_degrees(); // + = derecha
        let pitch = v[2].clamp(-1.0, 1.0).asin().to_degrees(); // + = arriba

        let dt = dt.unwrap_or(0.005);
        let yaw_f = self.f_yaw.filter(yaw, dt);
        let pitch_f = self.f_pitch.filter(pitch, dt);

        // Deadzone con histéresis sobre la última posición emitida
        let (ly, lp) = self.last_emitted.unwrap_or((yaw_f, pitch_f));
        let dist = ((yaw_f - ly).powi(2) + (pitch_f - lp).powi(2)).sqrt();
        let (out_yaw, out_pitch) = if dist < DEADZONE_DEG {
            (ly, lp)
        } else {
            self.last_emitted = Some((yaw_f, pitch_f));
            (yaw_f, pitch_f)
        };

        if abs_mode {
            let nx = (0.5 + out_yaw / sens_deg).clamp(0.0, 1.0);
            let ny = (0.5 - (out_pitch / sens_deg) * aspect_w_over_h).clamp(0.0, 1.0);
            PointerOutput::Abs { nx, ny }
        } else {
            // Modo relativo con orientación absoluta: delta contra lo último emitido
            let px_per_deg = screen_w_px / sens_deg;
            self.acc_x += (out_yaw - ly) * px_per_deg;
            self.acc_y -= (out_pitch - lp) * px_per_deg;
            let dx = self.acc_x as i32;
            let dy = self.acc_y as i32;
            self.acc_x -= dx as f32;
            self.acc_y -= dy as f32;
            if dx != 0 || dy != 0 {
                PointerOutput::Rel { dx, dy }
            } else {
                PointerOutput::None
            }
        }
    }

    fn compute_dt(&mut self, t_us: u64) -> Option<f32> {
        let last = self.last_t_us.replace(t_us)?;
        let dt = t_us.saturating_sub(last) as f32 / 1e6;
        if (1e-5..0.25).contains(&dt) {
            Some(dt)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::codec::FLAG_QUAT_VALID;

    fn packet(quat: [f32; 4], recenter: u8, t_us: u64, flags: u8) -> InputPacket {
        InputPacket {
            flags,
            session_id: 1,
            seq: 1,
            t_sensor_us: t_us,
            quat,
            gyro: [0.0; 3],
            accel: [0.0; 3],
            buttons: 0,
            recenter_count: recenter,
            battery_pct: 100,
            touch_scroll_dy: 0,
        }
    }

    fn rot_z(deg: f32) -> [f32; 4] {
        let h = deg.to_radians() / 2.0;
        [h.cos(), 0.0, 0.0, h.sin()]
    }

    fn rot_x(deg: f32) -> [f32; 4] {
        let h = deg.to_radians() / 2.0;
        [h.cos(), h.sin(), 0.0, 0.0]
    }

    #[test]
    fn recentrado_centra_y_yaw_derecha_mueve_derecha() {
        let mut e = PointerEngine::new();
        // Primera muestra = recentrado al centro
        let out = e.apply(&packet(rot_z(0.0), 0, 0, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        assert_eq!(out, PointerOutput::Abs { nx: 0.5, ny: 0.5 });

        // Girar -10° alrededor de Z (a la derecha) → yaw +10° → nx = 0.5 + 10/35
        let mut nx_last = 0.5;
        for i in 1..200 {
            if let PointerOutput::Abs { nx, .. } =
                e.apply(&packet(rot_z(-10.0), 0, i * 5_000, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0)
            {
                nx_last = nx;
            }
        }
        let expected = 0.5 + 10.0 / 35.0;
        assert!((nx_last - expected).abs() < 0.01, "nx={nx_last} esperado={expected}");
    }

    #[test]
    fn pitch_arriba_sube_el_cursor() {
        let mut e = PointerEngine::new();
        e.apply(&packet(rot_x(0.0), 0, 0, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        let mut ny_last = 0.5;
        for i in 1..200 {
            if let PointerOutput::Abs { ny, .. } =
                e.apply(&packet(rot_x(5.0), 0, i * 5_000, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0)
            {
                ny_last = ny;
            }
        }
        let expected = 0.5 - (5.0 / 35.0) * (16.0 / 9.0);
        assert!((ny_last - expected).abs() < 0.01, "ny={ny_last} esperado={expected}");
    }

    #[test]
    fn nuevo_recentrado_vuelve_al_centro() {
        let mut e = PointerEngine::new();
        e.apply(&packet(rot_z(0.0), 0, 0, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        for i in 1..50 {
            e.apply(&packet(rot_z(-20.0), 0, i * 5_000, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        }
        // Recentrar apuntando a -20°: esa orientación pasa a ser el centro
        let out = e.apply(&packet(rot_z(-20.0), 1, 260_000, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        assert_eq!(out, PointerOutput::Abs { nx: 0.5, ny: 0.5 });
    }

    #[test]
    fn sin_quat_cae_a_relativo() {
        let mut e = PointerEngine::new();
        let mut p = packet([1.0, 0.0, 0.0, 0.0], 0, 0, 0);
        p.gyro = [0.0, 0.0, 1.0];
        e.apply(&p, 35.0, 16.0 / 9.0, true, 1920.0);
        p.t_sensor_us = 10_000;
        let out = e.apply(&p, 35.0, 16.0 / 9.0, true, 1920.0);
        match out {
            PointerOutput::Rel { dx, .. } => assert!(dx < 0, "dx={dx}"),
            other => panic!("esperaba Rel, fue {other:?}"),
        }
    }
}
