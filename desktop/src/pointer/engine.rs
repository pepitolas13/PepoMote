//! Motor de puntero h1: gyro crudo → deltas relativos de ratón.
//! h2 lo sustituye por proyección absoluta del quaternion + one-euro.

/// Ejes Android con el móvil "de varita" (pantalla arriba, borde superior a la TV):
/// girar en el plano horizontal = rotación alrededor de Z (arriba) → X del cursor;
/// muñeca arriba/abajo = rotación alrededor de X (derecha) → Y del cursor.
/// Signos verificables en h1; corrección SOLO aquí.
const SIGN_X: f32 = -1.0; // gz positivo (giro a la izquierda) → cursor a la izquierda
const SIGN_Y: f32 = -1.0; // gx positivo (muñeca arriba) → cursor arriba

/// Sensibilidad: grados de giro para cruzar el ancho de la pantalla.
const DEG_TO_CROSS: f32 = 35.0;

pub struct PointerEngine {
    px_per_rad: f32,
    last_t_us: Option<u64>,
    acc_x: f32,
    acc_y: f32,
}

impl PointerEngine {
    pub fn new() -> Self {
        let screen_w = screen_width_px();
        Self {
            px_per_rad: screen_w / DEG_TO_CROSS.to_radians(),
            last_t_us: None,
            acc_x: 0.0,
            acc_y: 0.0,
        }
    }

    /// Devuelve deltas enteros de píxel; acumula la parte fraccional.
    pub fn apply(&mut self, gyro: [f32; 3], t_us: u64) -> (i32, i32) {
        let Some(last) = self.last_t_us else {
            self.last_t_us = Some(t_us);
            return (0, 0);
        };
        let dt = (t_us.saturating_sub(last)) as f32 / 1e6;
        self.last_t_us = Some(t_us);
        // dt disparatado (reconexión, wrap): ignora la muestra
        if !(1e-5..0.25).contains(&dt) {
            return (0, 0);
        }

        self.acc_x += SIGN_X * gyro[2] * dt * self.px_per_rad;
        self.acc_y += SIGN_Y * gyro[0] * dt * self.px_per_rad;

        let dx = self.acc_x as i32;
        let dy = self.acc_y as i32;
        self.acc_x -= dx as f32;
        self.acc_y -= dy as f32;
        (dx, dy)
    }
}

#[cfg(windows)]
fn screen_width_px() -> f32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    if w > 0 {
        w as f32
    } else {
        1920.0
    }
}

#[cfg(not(windows))]
fn screen_width_px() -> f32 {
    1920.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acumula_fracciones_sin_perderlas() {
        let mut e = PointerEngine {
            px_per_rad: 1000.0,
            last_t_us: None,
            acc_x: 0.0,
            acc_y: 0.0,
        };
        // Primera muestra solo fija t
        assert_eq!(e.apply([0.0, 0.0, 0.1], 0), (0, 0));
        // 10 ms a 0.1 rad/s con signo X=-1 → -1 px exacto
        let (dx, _) = e.apply([0.0, 0.0, 0.1], 10_000);
        assert_eq!(dx, -1);
        // 100 pasos de 1 ms a 0.1 rad/s → -10 px acumulados en total
        let mut total = 0;
        for i in 1..=100u64 {
            let (dx, _) = e.apply([0.0, 0.0, 0.1], 10_000 + i * 1_000);
            total += dx;
        }
        assert_eq!(total, -10);
    }

    #[test]
    fn dt_disparatado_se_ignora() {
        let mut e = PointerEngine {
            px_per_rad: 1000.0,
            last_t_us: None,
            acc_x: 0.0,
            acc_y: 0.0,
        };
        e.apply([0.0, 0.0, 1.0], 0);
        // 2 segundos de salto: reconexión → nada
        assert_eq!(e.apply([0.0, 0.0, 1.0], 2_000_000), (0, 0));
    }
}
