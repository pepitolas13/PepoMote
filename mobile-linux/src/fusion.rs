//! Fusión de orientación de 6 ejes (gyro + accel): filtro de Madgwick (IMU).
//! Produce el mismo quaternion que GAME_ROTATION_VECTOR en Android: rotación
//! del marco del dispositivo al del mundo, con Z = arriba (gravedad) y yaw
//! arbitrario pero estable. Es lo que el motor de puntero del receptor espera
//! (apuntado anclado al mundo, inmune al roll). Ejes del dispositivo, como en
//! Android e IIO: X = derecha, Y = arriba de la pantalla (borde superior),
//! Z = perpendicular saliendo de la pantalla.
//!
//! Referencia: S. Madgwick, "An efficient orientation filter for inertial and
//! inertial/magnetic sensor arrays", 2010 (implementación IMU sin magnetómetro).

pub struct Madgwick {
    /// w, x, y, z
    q: [f32; 4],
    beta: f32,
    initialized: bool,
}

impl Madgwick {
    /// `beta`: ganancia de corrección por gravedad (más = converge antes,
    /// pero más sensible a las aceleraciones lineales). 0.08-0.15 va bien.
    pub fn new(beta: f32) -> Self {
        Self {
            q: [1.0, 0.0, 0.0, 0.0],
            beta,
            initialized: false,
        }
    }

    pub fn quat(&self) -> [f32; 4] {
        self.q
    }

    /// `gyro` rad/s, `accel` m/s² (con gravedad), `dt` segundos.
    pub fn update(&mut self, gyro: [f32; 3], accel: [f32; 3], dt: f32) {
        if !self.initialized {
            // Arranque alineado con la gravedad: sin los segundos de
            // "asentamiento" que tiene el rotation vector de Android
            if let Some(q) = tilt_from_accel(accel) {
                self.q = q;
                self.initialized = true;
            }
        }
        let [gx, gy, gz] = gyro;
        let [q0, q1, q2, q3] = self.q;

        // Derivada del quaternion por el giroscopio: q̇ = ½ q ⊗ ω
        let mut q_dot0 = 0.5 * (-q1 * gx - q2 * gy - q3 * gz);
        let mut q_dot1 = 0.5 * (q0 * gx + q2 * gz - q3 * gy);
        let mut q_dot2 = 0.5 * (q0 * gy - q1 * gz + q3 * gx);
        let mut q_dot3 = 0.5 * (q0 * gz + q1 * gy - q2 * gx);

        // Corrección por gradiente hacia la gravedad medida
        let norm = (accel[0] * accel[0] + accel[1] * accel[1] + accel[2] * accel[2]).sqrt();
        if norm > 1e-3 {
            let ax = accel[0] / norm;
            let ay = accel[1] / norm;
            let az = accel[2] / norm;

            let _2q0 = 2.0 * q0;
            let _2q1 = 2.0 * q1;
            let _2q2 = 2.0 * q2;
            let _2q3 = 2.0 * q3;
            let _4q0 = 4.0 * q0;
            let _4q1 = 4.0 * q1;
            let _4q2 = 4.0 * q2;
            let _8q1 = 8.0 * q1;
            let _8q2 = 8.0 * q2;
            let q0q0 = q0 * q0;
            let q1q1 = q1 * q1;
            let q2q2 = q2 * q2;
            let q3q3 = q3 * q3;

            let mut s0 = _4q0 * q2q2 + _2q2 * ax + _4q0 * q1q1 - _2q1 * ay;
            let mut s1 = _4q1 * q3q3 - _2q3 * ax + 4.0 * q0q0 * q1 - _2q0 * ay - _4q1
                + _8q1 * q1q1
                + _8q1 * q2q2
                + _4q1 * az;
            let mut s2 = 4.0 * q0q0 * q2 + _2q0 * ax + _4q2 * q3q3 - _2q3 * ay - _4q2
                + _8q2 * q1q1
                + _8q2 * q2q2
                + _4q2 * az;
            let mut s3 = 4.0 * q1q1 * q3 - _2q1 * ax + 4.0 * q2q2 * q3 - _2q2 * ay;
            let s_norm = (s0 * s0 + s1 * s1 + s2 * s2 + s3 * s3).sqrt();
            if s_norm > 1e-9 {
                s0 /= s_norm;
                s1 /= s_norm;
                s2 /= s_norm;
                s3 /= s_norm;
                q_dot0 -= self.beta * s0;
                q_dot1 -= self.beta * s1;
                q_dot2 -= self.beta * s2;
                q_dot3 -= self.beta * s3;
            }
        }

        let mut q = [
            q0 + q_dot0 * dt,
            q1 + q_dot1 * dt,
            q2 + q_dot2 * dt,
            q3 + q_dot3 * dt,
        ];
        let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        if n > 1e-9 {
            for v in &mut q {
                *v /= n;
            }
            self.q = q;
        }
    }
}

/// Quaternion (dispositivo→mundo) que lleva la gravedad medida a +Z del
/// mundo; yaw = 0. None si el acelerómetro no da nada útil.
fn tilt_from_accel(accel: [f32; 3]) -> Option<[f32; 4]> {
    let n = (accel[0] * accel[0] + accel[1] * accel[1] + accel[2] * accel[2]).sqrt();
    if n < 1e-3 {
        return None;
    }
    let u = [accel[0] / n, accel[1] / n, accel[2] / n]; // "arriba" en el dispositivo
    // rotación mínima que lleva u a (0,0,1): eje = u × z, ángulo = acos(u·z)
    let dot = u[2].clamp(-1.0, 1.0);
    if dot > 0.999_999 {
        return Some([1.0, 0.0, 0.0, 0.0]);
    }
    if dot < -0.999_999 {
        return Some([0.0, 1.0, 0.0, 0.0]); // boca abajo: media vuelta sobre X
    }
    let axis = [u[1], -u[0], 0.0]; // u × (0,0,1)
    let an = (axis[0] * axis[0] + axis[1] * axis[1]).sqrt();
    let half = dot.acos() / 2.0;
    let s = half.sin() / an;
    Some([half.cos(), axis[0] * s, axis[1] * s, 0.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: f32 = 9.80665;

    /// Eje de apuntado (device +Y) en el mundo: columna Y de R(q). Copia de
    /// la fórmula del motor de puntero del receptor: si esto cuadra, cuadra.
    fn pointing_dir(q: [f32; 4]) -> [f32; 3] {
        let [w, x, y, z] = q;
        [
            2.0 * (x * y - w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + w * x),
        ]
    }

    fn pitch_deg(q: [f32; 4]) -> f32 {
        let d = pointing_dir(q);
        d[2].atan2((d[0] * d[0] + d[1] * d[1]).sqrt()).to_degrees()
    }

    fn yaw_deg(q: [f32; 4]) -> f32 {
        let d = pointing_dir(q);
        d[0].atan2(d[1]).to_degrees()
    }

    #[test]
    fn plano_sobre_la_mesa_es_identidad() {
        let mut f = Madgwick::new(0.1);
        for _ in 0..200 {
            f.update([0.0; 3], [0.0, 0.0, G], 0.005);
        }
        assert!(pitch_deg(f.quat()).abs() < 0.5);
    }

    #[test]
    fn morro_arriba_30_grados_da_pitch_30() {
        // Móvil con el borde superior 30° arriba: android a = (0, g·sin30, g·cos30)
        let mut f = Madgwick::new(0.1);
        let a = [0.0, G * 0.5, G * 0.866_025];
        for _ in 0..400 {
            f.update([0.0; 3], a, 0.005);
        }
        let p = pitch_deg(f.quat());
        assert!((p - 30.0).abs() < 1.0, "pitch={p}");
    }

    #[test]
    fn el_arranque_ya_esta_alineado() {
        // Primera muestra: sin ventana de asentamiento
        let mut f = Madgwick::new(0.1);
        f.update([0.0; 3], [0.0, G * 0.5, G * 0.866_025], 0.005);
        let p = pitch_deg(f.quat());
        assert!((p - 30.0).abs() < 1.0, "pitch tras 1 muestra={p}");
    }

    #[test]
    fn gyro_z_positivo_gira_a_la_izquierda() {
        // Girar sobre Z (regla de la mano derecha, Z arriba) = hacia la
        // izquierda visto desde arriba: el eje de apuntado va hacia -X
        let mut f = Madgwick::new(0.1);
        f.update([0.0; 3], [0.0, 0.0, G], 0.005);
        for _ in 0..100 {
            f.update([0.0, 0.0, 1.0], [0.0, 0.0, G], 0.005); // 1 rad/s · 0.5 s
        }
        let y = yaw_deg(f.quat());
        let expected = -(0.5f32).to_degrees();
        assert!((y - expected).abs() < 1.5, "yaw={y} esperado={expected}");
        assert!(pitch_deg(f.quat()).abs() < 0.5);
    }

    #[test]
    fn el_quaternion_se_mantiene_unitario() {
        let mut f = Madgwick::new(0.1);
        for i in 0..2000 {
            let t = i as f32 * 0.005;
            f.update([t.sin(), (2.0 * t).cos(), 0.3], [0.1, G * 0.4, G * 0.9], 0.005);
        }
        let q = f.quat();
        let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        assert!((n - 1.0).abs() < 1e-4);
    }
}
