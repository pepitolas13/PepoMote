//! Puntero IR del perfil Wii (Dolphin ≥ 2407): de la salida absoluta del
//! motor de puntero (nx, ny) a los dos puntos de la barra sensora que el
//! perfil recompone en el grupo `IRPassthrough` (dolphin.rs `ir_passthrough`,
//! protocol/DSU.md «Puntero IR»). Es la misma cámara que Dolphin sintetiza
//! para su puntero IMU (WiimoteEmu/Camera.cpp, comprobada numéricamente
//! contra sus matrices): 42° × 31,5° de campo sobre 1023 × 767, LEDs a
//! 0,2 m, barra 10 cm por encima del centro de la TV (SYSCONF «arriba», el
//! valor por defecto), imagen en espejo (apuntar a la derecha = X baja,
//! arriba = Y baja) y el par inclinado con el roll (lado derecho abajo = el
//! punto de X alta baja). Así el cursor del juego cae donde caía con el
//! puntero IMU de Dolphin, y «acercar» solo separa el par y agranda los
//! puntos: el punto medio se queda donde está. (Una barra real se vería más
//! arriba al acercarse, pero el juego no compensa esa altura con la
//! distancia: probado en Wii Sports Resort, el cursor se hundía hasta el
//! borde inferior mientras se mantenía «acercar».)

use super::WiiIr;
use std::time::{Duration, Instant};

/// Grados con los que se llama al motor de puntero (el campo horizontal de
/// la cámara) y relación de aspecto (la de la cámara, 4:3): solo por
/// comodidad, la salida se vuelve a pasar a grados (`IrPointer::track`).
pub const SENS_DEG: f32 = 42.0;
pub const ASPECT: f32 = 4.0 / 3.0;
const FOV_X_DEG: f32 = 42.0;
const FOV_Y_DEG: f32 = FOV_X_DEG * 3.0 / 4.0;
/// Separación del par por nivel de «acercar», la MISMA tabla que el perfil
/// (dolphin.rs `IR_SEP`): LEDs a 0,2 m con 42° de campo a 2,0 / 1,3 / 0,85 /
/// 0,55 m.
const LEVEL_SEP: [f32; 4] = [0.13, 0.20, 0.31, 0.47];
/// Barra sensora 10 cm sobre el centro de la TV (Dolphin: IR/Vertical
/// Offset) vista desde los 2 m del puntero IMU de Dolphin: el punto medio
/// queda ese poco por debajo del centro de la cámara, a cualquier nivel.
const BAR_HEIGHT_M: f32 = 0.10;
const NEUTRAL_DISTANCE_M: f32 = 2.0;
/// La separación va en fracción del ancho (1023); su componente vertical se
/// pasa a fracción del alto (767): el mismo ×1,3333 que lleva el perfil.
const Y_PER_X: f32 = 1023.0 / 767.0;
/// Fondo de escala de x/y en el cable (15 bits).
const RANGE: f32 = 32767.0;
/// Paso de la rampa de «acercar»: 0 → 3 en unos 360 ms al mantener, y lo
/// mismo de vuelta al soltar.
const NEAR_STEP: Duration = Duration::from_millis(120);

/// Rampa de niveles del bit «acercar» (PMP bit 30): sube un nivel por paso
/// mientras se mantiene y baja igual al soltar, así el juego ve al mando
/// acercarse y alejarse en vez de saltar.
pub struct NearRamp {
    level: u8,
    last_step: Instant,
}

impl Default for NearRamp {
    fn default() -> Self {
        Self::new()
    }
}

impl NearRamp {
    pub fn new() -> Self {
        Self { level: 0, last_step: Instant::now() }
    }

    pub fn update(&mut self, held: bool, now: Instant) -> u8 {
        let target = if held { 3 } else { 0 };
        if self.level == target {
            self.last_step = now;
        } else if now.duration_since(self.last_step) >= NEAR_STEP {
            self.level = if target > self.level { self.level + 1 } else { self.level - 1 };
            self.last_step = now;
        }
        self.level
    }
}

/// Roll (rad, positivo = el lado derecho del móvil hacia abajo) del quaternion
/// PMP (w, x, y, z; dispositivo → mundo, Z arriba): el eje X del dispositivo
/// (su lado derecho, con el borde superior apuntando a la TV) en el mundo.
pub fn roll_rad(q: [f32; 4]) -> f32 {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n < 1e-6 {
        return 0.0;
    }
    let (w, x, y, z) = (q[0] / n, q[1] / n, q[2] / n, q[3] / n);
    // columna X de la matriz de rotación: a dónde apunta el lado derecho
    let rx = 1.0 - 2.0 * (y * y + z * z);
    let ry = 2.0 * (x * y + w * z);
    let rz = 2.0 * (x * z - w * y);
    (-rz).atan2((rx * rx + ry * ry).sqrt())
}

/// Byte de Left X (128..255 = −90°..+90°): el perfil lo lee como `Left X+`
/// (0..1) y hace (v − 0,5)·π.
fn roll_byte(roll: f32) -> u8 {
    let v = (roll / std::f32::consts::FRAC_PI_2).clamp(-1.0, 1.0);
    128 + (63.5 * (1.0 + v)).round() as u8
}

/// Los dos puntos para el apuntado (yaw, pitch en grados respecto al
/// recentrado; positivo = derecha / arriba) y el nivel pedido, o el mayor
/// nivel por debajo con el que ninguno se salga de la cámara (Dolphin
/// recorta al borde en vez de ocultar el punto). `roll`: `None` = Left X es
/// el stick del Nunchuk propio y el par va horizontal. `None` de vuelta =
/// fuera de la cámara (centinela).
pub fn encode(yaw_deg: f32, pitch_deg: f32, level: u8, roll: Option<f32>) -> Option<WiiIr> {
    if !yaw_deg.is_finite() || !pitch_deg.is_finite() {
        return None;
    }
    let yaw = yaw_deg.to_radians();
    let pitch = pitch_deg.to_radians();
    let half_x = (FOV_X_DEG.to_radians() / 2.0).tan();
    let half_y = (FOV_Y_DEG.to_radians() / 2.0).tan();
    // más allá de ±90° la tangente se da la vuelta: eso es «detrás»
    if yaw.abs() >= std::f32::consts::FRAC_PI_2 || pitch.abs() >= std::f32::consts::FRAC_PI_2 {
        return None;
    }
    let mid_x = 0.5 - yaw.tan() / (2.0 * half_x);
    let mid_y = 0.5 - pitch.tan() / (2.0 * half_y) + (BAR_HEIGHT_M / NEUTRAL_DISTANCE_M) / (2.0 * half_y);
    let (sin_r, cos_r) = roll.unwrap_or(0.0).sin_cos();
    let inside = |x: f32, y: f32| (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y);
    for l in (0..=level.min(3)).rev() {
        let sep = LEVEL_SEP[l as usize];
        let dx = sep / 2.0 * cos_r;
        let dy = sep / 2.0 * sin_r * Y_PER_X;
        if inside(mid_x - dx, mid_y - dy) && inside(mid_x + dx, mid_y + dy) {
            return Some(WiiIr {
                x: (mid_x * RANGE).round() as u16,
                y: (mid_y * RANGE).round() as u16,
                level: l,
                roll: roll.map(roll_byte),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(ir: &WiiIr) -> (f32, f32) {
        (ir.x as f32 / RANGE * 1023.0, ir.y as f32 / RANGE * 767.0)
    }

    /// Los mismos números que salen de las matrices de Dolphin (Camera.cpp)
    /// con su puntero a 2 m y la barra 10 cm arriba: neutro en (511, 452),
    /// 10° a la derecha en x ≈ 271, 10° arriba en y ≈ 213.
    #[test]
    fn misma_camara_que_dolphin() {
        let neutral = encode(0.0, 0.0, 0, Some(0.0)).unwrap();
        let (x, y) = px(&neutral);
        assert!((x - 511.5).abs() < 1.0, "x={x}");
        assert!((y - 452.0).abs() < 2.0, "y={y}");
        assert_eq!(neutral.level, 0);
        let right = encode(10.0, 0.0, 0, Some(0.0)).unwrap();
        let (x, _) = px(&right);
        assert!((x - 276.0).abs() < 6.0, "10° a la derecha: X baja (espejo), x={x}");
        let up = encode(0.0, 10.0, 0, Some(0.0)).unwrap();
        let (_, y) = px(&up);
        assert!((y - 213.0).abs() < 3.0, "10° arriba: Y baja, y={y}");
        let left = encode(-10.0, 0.0, 0, Some(0.0)).unwrap();
        assert!(px(&left).0 > 740.0);
    }

    #[test]
    fn acercar_sube_el_nivel_sin_mover_el_punto_medio() {
        let far = encode(0.0, 0.0, 0, Some(0.0)).unwrap();
        let near = encode(0.0, 0.0, 3, Some(0.0)).unwrap();
        assert_eq!(near.level, 3);
        assert_eq!((near.x, near.y), (far.x, far.y), "acercarse solo separa el par");
        let (_, y) = px(&near);
        assert!((y - 452.0).abs() < 2.0, "y={y}");
    }

    #[test]
    fn cerca_del_borde_baja_el_nivel_y_fuera_de_la_camara_no_hay_puntos() {
        // apuntando a la derecha del todo (16,8°) el par de 0,47 no cabe: se queda en 1
        let edge = encode(16.8, 0.0, 3, Some(0.0)).unwrap();
        assert_eq!(edge.level, 1, "{edge:?}");
        assert!(encode(29.4, 0.0, 0, Some(0.0)).is_none());
        assert!(encode(0.0, 28.35, 0, Some(0.0)).is_none());
        assert!(encode(f32::NAN, 0.0, 0, None).is_none());
        // muy lejos (más de 90°): detrás, nunca «se da la vuelta»
        assert!(encode(126.0, 0.0, 0, None).is_none());
    }

    #[test]
    fn roll_horario_positivo_y_su_byte() {
        // giro de +30° sobre el eje de apuntado (Y del dispositivo): el lado
        // derecho baja → roll positivo (horario visto desde detrás)
        let (s, c) = (15f32.to_radians()).sin_cos();
        let r = roll_rad([c, 0.0, s, 0.0]);
        assert!((r.to_degrees() - 30.0).abs() < 0.01, "{}", r.to_degrees());
        assert!((roll_rad([1.0, 0.0, 0.0, 0.0])).abs() < 1e-6);
        assert_eq!(roll_rad([0.0; 4]), 0.0, "quaternion nulo: sin roll");
        assert_eq!(roll_byte(0.0), 192);
        assert_eq!(roll_byte(std::f32::consts::FRAC_PI_2), 255);
        assert_eq!(roll_byte(-std::f32::consts::FRAC_PI_2), 128);
        assert_eq!(roll_byte(3.0), 255, "más de 90° se recorta");
        // el perfil deshace el byte: (v − 0,5)·π con v = (byte − 128)/127
        let v = (roll_byte(45f32.to_radians()) as f32 - 128.0) / 127.0;
        assert!((((v - 0.5) * std::f32::consts::PI).to_degrees() - 45.0).abs() < 1.0);
        // sin quaternion utilizable: roll 0 explícito; con Nunchuk propio: None
        assert_eq!(encode(0.0, 0.0, 0, Some(0.0)).unwrap().roll, Some(192));
        assert_eq!(encode(0.0, 0.0, 0, None).unwrap().roll, None);
    }

    #[test]
    fn la_rampa_sube_por_pasos_y_baja_igual() {
        let t0 = Instant::now();
        let mut r = NearRamp { level: 0, last_step: t0 };
        assert_eq!(r.update(true, t0), 0, "el primer paso tarda un intervalo");
        assert_eq!(r.update(true, t0 + Duration::from_millis(119)), 0);
        assert_eq!(r.update(true, t0 + Duration::from_millis(120)), 1);
        assert_eq!(r.update(true, t0 + Duration::from_millis(200)), 1);
        assert_eq!(r.update(true, t0 + Duration::from_millis(240)), 2);
        assert_eq!(r.update(true, t0 + Duration::from_millis(360)), 3);
        assert_eq!(r.update(true, t0 + Duration::from_millis(900)), 3, "tope en 3");
        assert_eq!(r.update(false, t0 + Duration::from_millis(1000)), 3, "al soltar baja por pasos");
        assert_eq!(r.update(false, t0 + Duration::from_millis(1020)), 2);
        assert_eq!(r.update(false, t0 + Duration::from_millis(1140)), 1);
        assert_eq!(r.update(false, t0 + Duration::from_millis(1260)), 0);
        assert_eq!(r.update(false, t0 + Duration::from_millis(5000)), 0);
    }
}
