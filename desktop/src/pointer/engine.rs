//! Motor de puntero h2: apuntado absoluto ANCLADO AL MUNDO.
//!
//! El quaternion (GAME_ROTATION_VECTOR) lleva el dispositivo al marco del
//! mundo, cuyo eje Z es la gravedad. El eje de apuntado del móvil (device +Y)
//! se proyecta al mundo y se descompone ahí: pitch = elevación sobre el
//! horizonte real, yaw = ángulo en el plano horizontal real. El ROLL del
//! dispositivo no aparece en ninguna de las dos coordenadas → rotar el móvil
//! mientras apuntas (Wii Sports) no afecta al cursor, ni rolado al recentrar.
//!
//! Fallback: integración relativa del gyro para móviles sin rotation vector
//! (flags bit0 = 0); ese camino sí es sensible al roll (ejes del dispositivo).

use super::one_euro::Filter2D;
use crate::net::codec::{InputPacket, FLAG_QUAT_VALID};

/// Congelación por velocidad con histéresis: clavado en reposo, continuo
/// (sin cuantizar la trayectoria) en cuanto hay intención de movimiento.
const FREEZE_ENTER_DEG_S: f32 = 0.6;
const FREEZE_EXIT_DEG_S: f32 = 1.8;
/// Escape por posición: aunque la velocidad medida sea baja (deriva muy lenta,
/// entrada en escalón), si lo filtrado se aleja esto del punto congelado, se libera.
const FREEZE_ESCAPE_DEG: f32 = 0.35;

/// Signos del fallback relativo (h1). Corrección SOLO aquí.
const SIGN_X: f32 = -1.0;
const SIGN_Y: f32 = -1.0;

// --- Fusión complementaria ADAPTATIVA del receptor ---
// El GAME_ROTATION_VECTOR del móvil es rocoso en REPOSO (anclado a la
// gravedad, sin deriva), pero en un gesto RÁPIDO su acelerómetro mide
// gravedad + aceleración del gesto y mete una inclinación FALSA: el cursor
// "sube solo" y pierde recorrido. El gyro es al revés: fiel en el gesto,
// pero su ruido/sesgo integrado deriva en reposo.
//
// La fusión pesa cada fuente según cuánto se mueve el móvil:
//  - Reposo/lento: sigue FUERTE al quat (λ alto) → cero deriva del gyro; el
//    cursor se queda clavado y el congelado lo suelta al ratón real.
//  - Gesto rápido: apenas corrige (λ bajo) → manda el gyro, sin tilt espurio.
// La velocidad de giro (‖gyro‖) elige el punto entre ambos, de forma
// continua. Un acelerómetro contaminado sin apenas giro (traslación pura)
// también baja la confianza en el quat.
/// λ de corrección hacia el quat en REPOSO (1/s): τ≈1/λ. Alto = mata la
/// deriva del gyro deprisa y clava el cursor.
const FUSION_LAMBDA_REST: f32 = 12.0;
/// λ de corrección en pleno gesto: casi no corrige, respeta el gyro.
const FUSION_LAMBDA_MOVE: f32 = 0.3;
/// ‖gyro‖ (rad/s) al que la confianza en el quat cae a la mitad (~20°/s):
/// por debajo domina el quat (reposo), por encima el gyro (gesto).
const FUSION_TRUST_RATE: f32 = 0.35;
/// ‖gyro‖ sobre esto = gesto violento: forzar λ mínimo un tiempo (rad/s).
const ANOMALY_RATE_RADS: f32 = 3.0;
/// |‖accel‖ − g| sobre esto = acelerómetro contaminado (m/s²).
const ANOMALY_ACCEL_MS2: f32 = 3.0;
/// Tras un gesto violento, mantener λ mínimo esto: deja que el quat sane su
/// inclinación falsa (su acelerómetro se recalma) antes de volver a seguirlo.
const ANOMALY_HOLD_S: f32 = 0.2;
const G_MS2: f32 = 9.81;
/// h² mínimo del eje de apuntado: por debajo (|pitch| ≳ 81°) el yaw es
/// indefinido (polo) y se congela para que no dé latigazos.
const POLE_H2: f32 = 0.022;
/// Zona muerta del gyro en el fallback relativo (rad/s ≈ 1.7°/s): mata el
/// sesgo típico de los MEMS baratos sin tragarse el giro intencional.
const GYRO_DEADZONE_RADS: f32 = 0.03;

/// Zona muerta SUAVE: 0 dentro de ±dz, y fuera resta dz (sin escalón brusco,
/// así el arranque del movimiento no da un tirón).
fn soft_deadzone(v: f32, dz: f32) -> f32 {
    if v > dz {
        v - dz
    } else if v < -dz {
        v + dz
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerOutput {
    /// Coordenadas normalizadas sobre la pantalla primaria (pueden salirse:
    /// cada inyector recorta a su espacio real).
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
        Self {
            w: p.quat[0],
            x: p.quat[1],
            y: p.quat[2],
            z: p.quat[3],
        }
        .normalized()
    }

    fn normalized(self) -> Self {
        let n = (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if n < 1e-6 {
            return Self { w: 1.0, x: 0.0, y: 0.0, z: 0.0 };
        }
        Self { w: self.w / n, x: self.x / n, y: self.y / n, z: self.z / n }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn mul(self, o: Self) -> Self {
        Self {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }

    /// Rota un vector del marco del dispositivo al mundo: R(q)·v.
    fn rotate(self, v: [f32; 3]) -> [f32; 3] {
        let u = [self.x, self.y, self.z];
        let t = [
            2.0 * (u[1] * v[2] - u[2] * v[1]),
            2.0 * (u[2] * v[0] - u[0] * v[2]),
            2.0 * (u[0] * v[1] - u[1] * v[0]),
        ];
        [
            v[0] + self.w * t[0] + u[1] * t[2] - u[2] * t[1],
            v[1] + self.w * t[1] + u[2] * t[0] - u[0] * t[2],
            v[2] + self.w * t[2] + u[0] * t[1] - u[1] * t[0],
        ]
    }

    /// Eje de apuntado del dispositivo (0,1,0) expresado en el MUNDO.
    /// (Columna Y de la matriz de rotación.)
    fn pointing_dir_world(self) -> [f32; 3] {
        let (w, x, y, z) = (self.w, self.x, self.y, self.z);
        [
            2.0 * (x * y - w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + w * x),
        ]
    }

    /// (yaw, pitch) en grados, en el marco del mundo: yaw = ángulo en el plano
    /// horizontal (arbitrario pero consistente sin magnetómetro), pitch =
    /// elevación sobre el horizonte de la gravedad. El roll no interviene.
    fn world_angles(self) -> (f32, f32) {
        let d = self.pointing_dir_world();
        let yaw = d[0].atan2(d[1]).to_degrees();
        let horiz = (d[0] * d[0] + d[1] * d[1]).sqrt();
        let pitch = d[2].atan2(horiz).to_degrees();
        (yaw, pitch)
    }
}

/// Envuelve una diferencia de ángulos a (-180, 180].
fn wrap180(deg: f32) -> f32 {
    let x = (deg + 180.0).rem_euclid(360.0);
    x - 180.0
}

pub struct PointerEngine {
    /// (yaw, pitch) del mundo capturados en el recentrado.
    ref_angles: Option<(f32, f32)>,
    last_recenter: Option<u8>,
    filter: Filter2D,
    frozen: bool,
    last_emitted: Option<(f32, f32)>, // grados (yaw, pitch) relativos
    /// Puente anti-salto del descongelado: se fija a (held − filtrado) al
    /// liberar (salto cero) y se disuelve exponencialmente con el movimiento.
    offset: (f32, f32),
    /// t de la primera muestra: ventana de asentamiento del rotation vector.
    first_t_us: Option<u64>,
    /// Posición real del cursor (normalizada), si el SO la sabe: al
    /// descongelar, el puntero continúa desde donde el RATÓN lo dejó.
    cursor_hint: Option<(f32, f32)>,
    last_t_us: Option<u64>,
    /// (yaw, pitch) fusionados del receptor: gyro a corto plazo, quat en calma.
    fused: Option<(f32, f32)>,
    /// Segundos que quedan de ventana de anomalía (gesto violento).
    anomaly_hold: f32,
    // fallback relativo
    acc_x: f32,
    acc_y: f32,
}

impl PointerEngine {
    pub fn new() -> Self {
        Self {
            ref_angles: None,
            last_recenter: None,
            // beta = cuánto se abre el filtro con la velocidad (menos lag en
            // flicks); la estabilidad en reposo la da la congelación. 0.2:
            // a 100°/s el cutoff sube a ~21 Hz — el flick no pierde recorrido
            filter: Filter2D::new(1.0, 0.2),
            frozen: false,
            last_emitted: None,
            offset: (0.0, 0.0),
            first_t_us: None,
            cursor_hint: None,
            last_t_us: None,
            fused: None,
            anomaly_hold: 0.0,
            acc_x: 0.0,
            acc_y: 0.0,
        }
    }

    /// Fusión complementaria: integra el gyro proyectado al mundo (verdad a
    /// corto plazo) y converge hacia el quat solo en calma. Ver constantes
    /// FUSION_*/ANOMALY_* arriba.
    fn fuse(&mut self, p: &InputPacket, q: Quat, qyaw: f32, qpitch: f32, dt: Option<f32>) -> (f32, f32) {
        // Primera muestra o hueco grande (suspensión, pérdida): el quat es
        // la mejor verdad disponible.
        let Some(dt) = dt else {
            self.fused = Some((qyaw, qpitch));
            self.anomaly_hold = 0.0;
            return (qyaw, qpitch);
        };
        let (mut fy, mut fp) = self.fused.unwrap_or((qyaw, qpitch));

        let g = p.gyro;
        let a = p.accel;
        let gyro_mag = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
        let acc_mag = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        // acc_mag ≈ 0 = emisor sin acelerómetro: ese gate no aplica
        let accel_bad = acc_mag > 0.5 && (acc_mag - G_MS2).abs() > ANOMALY_ACCEL_MS2;
        if gyro_mag > ANOMALY_RATE_RADS || accel_bad {
            self.anomaly_hold = ANOMALY_HOLD_S;
        } else {
            self.anomaly_hold = (self.anomaly_hold - dt).max(0.0);
        }

        // d(yaw)/dt y d(pitch)/dt desde el gyro en el mundo: ḋ = ω × d
        let d = q.pointing_dir_world();
        let w = q.rotate(g);
        let dd = [
            w[1] * d[2] - w[2] * d[1],
            w[2] * d[0] - w[0] * d[2],
            w[0] * d[1] - w[1] * d[0],
        ];
        let h2 = d[0] * d[0] + d[1] * d[1];
        let pole = h2 < POLE_H2;
        let h2c = h2.max(1e-4);
        let h = h2c.sqrt();
        let dyaw = (d[1] * dd[0] - d[0] * dd[1]) / h2c;
        let dh = (d[0] * dd[0] + d[1] * dd[1]) / h;
        let dpitch = h * dd[2] - d[2] * dh; // denominador h²+d2² = ‖d‖² = 1

        if !pole {
            fy += dyaw.to_degrees() * dt;
        }
        fp += dpitch.to_degrees() * dt;

        // Confianza en el quat: 1 quieto, →0 en gesto (o accel contaminado).
        // Elige λ entre reposo (mata deriva) y movimiento (respeta el gyro).
        let trust = if self.anomaly_hold > 0.0 {
            0.0
        } else {
            let r = gyro_mag / FUSION_TRUST_RATE;
            1.0 / (1.0 + r * r)
        };
        let lambda = FUSION_LAMBDA_MOVE + (FUSION_LAMBDA_REST - FUSION_LAMBDA_MOVE) * trust;
        let l = 1.0 - (-dt * lambda).exp();
        if !pole {
            fy += wrap180(qyaw - fy) * l;
        }
        fp += (qpitch - fp) * l;

        self.fused = Some((fy, fp));
        (fy, fp)
    }

    /// La telemetría informa de la posición real del cursor antes de cada
    /// apply (barato); solo se usa en la transición de descongelado.
    pub fn set_cursor_hint(&mut self, hint: Option<(f32, f32)>) {
        self.cursor_hint = hint;
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
            // Fallback h1: integración relativa del gyro (ejes del dispositivo).
            // Sin quat no hay referencia absoluta que cancele el sesgo del
            // gyro; una zona muerta suave lo mata en reposo (el móvil quieto
            // no arrastra el cursor) sin comerse el movimiento intencional.
            let Some(dt) = dt else { return PointerOutput::None };
            let px_per_rad = screen_w_px / sens_deg.to_radians();
            let gz = soft_deadzone(p.gyro[2], GYRO_DEADZONE_RADS);
            let gx = soft_deadzone(p.gyro[0], GYRO_DEADZONE_RADS);
            self.acc_x += SIGN_X * gz * dt * px_per_rad;
            self.acc_y += SIGN_Y * gx * dt * px_per_rad;
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
        let (qyaw, qpitch) = q.world_angles();
        let (yaw_w, pitch_w) = self.fuse(p, q, qyaw, qpitch, dt);

        if recentered || self.ref_angles.is_none() {
            self.ref_angles = Some((yaw_w, pitch_w));
            self.filter.reset();
            self.frozen = false;
            self.offset = (0.0, 0.0);
            self.last_emitted = Some((0.0, 0.0));
            return if abs_mode {
                PointerOutput::Abs { nx: 0.5, ny: 0.5 }
            } else {
                PointerOutput::None
            };
        }

        let (yaw_ref, pitch_ref) = self.ref_angles.unwrap();
        let yaw = wrap180(yaw_w - yaw_ref); // + = derecha
        let pitch = pitch_w - pitch_ref; // + = arriba

        // Asentamiento del rotation vector: el sensor arranca en identidad y
        // "salta" a la orientación real al engancharse a la gravedad. Si en
        // los primeros 2.5 s la desviación excede lo físicamente razonable,
        // la referencia era falsa: re-anclar (cursor quieto en el centro).
        let first_t = *self.first_t_us.get_or_insert(p.t_sensor_us);
        if p.t_sensor_us.saturating_sub(first_t) < 2_500_000 {
            let lim = sens_deg * 0.9;
            // Se mira el QUAT crudo, no el fusionado: el salto de
            // asentamiento aparece en el quat al instante (sin gyro).
            let qdev_yaw = wrap180(qyaw - yaw_ref);
            let qdev_pitch = qpitch - pitch_ref;
            if qdev_yaw.abs() > lim || qdev_pitch.abs() * aspect_w_over_h > lim {
                // El salto de asentamiento es una corrección del móvil, no un
                // giro: re-anclar DIRECTO al quat (la fusión saltaría lenta).
                self.ref_angles = Some((qyaw, qpitch));
                self.fused = Some((qyaw, qpitch));
                self.filter.reset();
                self.frozen = false;
                self.offset = (0.0, 0.0);
                self.last_emitted = Some((0.0, 0.0));
                return if abs_mode {
                    PointerOutput::Abs { nx: 0.5, ny: 0.5 }
                } else {
                    PointerOutput::None
                };
            }
        }

        let dt = dt.unwrap_or(0.005);
        let (yaw_f, pitch_f, speed) = self.filter.filter(yaw, pitch, dt);

        let (prev_yaw, prev_pitch) = self.last_emitted.unwrap_or((yaw_f, pitch_f));

        if self.frozen {
            // ¿Cuánto se ha alejado la orientación real del punto congelado?
            let dev_y = yaw_f + self.offset.0 - prev_yaw;
            let dev_p = pitch_f + self.offset.1 - prev_pitch;
            let dev = (dev_y * dev_y + dev_p * dev_p).sqrt();
            if speed > FREEZE_EXIT_DEG_S || dev > FREEZE_ESCAPE_DEG {
                // Liberar SIN salto y desde donde esté el cursor DE VERDAD:
                // si el ratón real lo movió mientras estábamos congelados, el
                // puntero continúa desde ahí (convivencia con el mouse).
                self.frozen = false;
                let (anchor_yaw, anchor_pitch) = match (abs_mode, self.cursor_hint) {
                    (true, Some((cx, cy))) => (
                        (cx - 0.5) * sens_deg,
                        (0.5 - cy) * sens_deg / aspect_w_over_h,
                    ),
                    _ => (prev_yaw, prev_pitch),
                };
                self.offset = (anchor_yaw - yaw_f, anchor_pitch - pitch_f);
                self.last_emitted = Some((anchor_yaw, anchor_pitch));
            } else {
                // Congelado = SILENCIO: ni un paquete de inyección. El ratón
                // real queda libre mientras el móvil esté quieto.
                return PointerOutput::None;
            }
        }

        // Libre: el puente se disuelve dentro del propio movimiento (más
        // deprisa cuanto más rápido te mueves — imperceptible).
        let k = (-dt * (4.0 + speed * 0.5)).exp();
        self.offset.0 *= k;
        self.offset.1 *= k;

        let out_yaw = yaw_f + self.offset.0;
        let out_pitch = pitch_f + self.offset.1;

        if speed < FREEZE_ENTER_DEG_S {
            self.frozen = true;
        }
        self.last_emitted = Some((out_yaw, out_pitch));
        self.emit(out_yaw, out_pitch, sens_deg, aspect_w_over_h, abs_mode, screen_w_px, prev_yaw, prev_pitch)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        out_yaw: f32,
        out_pitch: f32,
        sens_deg: f32,
        aspect_w_over_h: f32,
        abs_mode: bool,
        screen_w_px: f32,
        prev_yaw: f32,
        prev_pitch: f32,
    ) -> PointerOutput {

        if abs_mode {
            // Sin recorte a la pantalla primaria: con varios monitores el cursor
            // debe poder salir. Cada inyector recorta a su espacio real; aquí
            // solo un clamp de cordura.
            let nx = (0.5 + out_yaw / sens_deg).clamp(-2.0, 3.0);
            let ny = (0.5 - (out_pitch / sens_deg) * aspect_w_over_h).clamp(-2.0, 3.0);
            PointerOutput::Abs { nx, ny }
        } else {
            // Modo relativo con orientación absoluta: delta contra lo último emitido
            let px_per_deg = screen_w_px / sens_deg;
            self.acc_x += (out_yaw - prev_yaw) * px_per_deg;
            self.acc_y -= (out_pitch - prev_pitch) * px_per_deg;
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

    const DT_US: u64 = 5_000;

    fn packet(quat: [f32; 4], recenter: u8, t_us: u64, flags: u8) -> InputPacket {
        InputPacket {
            flags,
            session_id: 1,
            seq: 1,
            t_sensor_us: t_us,
            quat,
            gyro: [0.0; 3],
            accel: [0.0, 0.0, G_MS2],
            buttons: 0,
            recenter_count: recenter,
            battery_pct: 100,
            touch_scroll_dy: 0,
            stick_x: 0,
            stick_y: 0,
            stick_rx: 0,
            stick_ry: 0,
            touch_x: 0,
            touch_y: 0,
        }
    }

    fn qrot_z(deg: f32) -> Quat {
        let h = deg.to_radians() / 2.0;
        Quat { w: h.cos(), x: 0.0, y: 0.0, z: h.sin() }
    }

    fn qrot_x(deg: f32) -> Quat {
        let h = deg.to_radians() / 2.0;
        Quat { w: h.cos(), x: h.sin(), y: 0.0, z: 0.0 }
    }

    fn qrot_y(deg: f32) -> Quat {
        let h = deg.to_radians() / 2.0;
        Quat { w: h.cos(), x: 0.0, y: h.sin(), z: 0.0 }
    }

    fn arr(q: Quat) -> [f32; 4] {
        [q.w, q.x, q.y, q.z]
    }

    fn conj(q: Quat) -> Quat {
        Quat { w: q.w, x: -q.x, y: -q.y, z: -q.z }
    }

    /// Eje y ángulo (rad, camino corto) de la rotación prev→cur en el marco
    /// del DISPOSITIVO: lo que un gyro real mediría durante ese giro.
    fn delta_axis_angle(prev: Quat, cur: Quat) -> ([f32; 3], f32) {
        let d = conj(prev).mul(cur).normalized();
        let s = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
        if s < 1e-9 {
            return ([0.0; 3], 0.0);
        }
        let mut ang = 2.0 * s.atan2(d.w);
        if ang > std::f32::consts::PI {
            ang -= 2.0 * std::f32::consts::PI;
        }
        ([d.x / s, d.y / s, d.z / s], ang)
    }

    fn qrot_axis(axis: [f32; 3], ang: f32) -> Quat {
        let h = ang / 2.0;
        let s = h.sin();
        Quat { w: h.cos(), x: axis[0] * s, y: axis[1] * s, z: axis[2] * s }
    }

    /// Móvil simulado FÍSICAMENTE COHERENTE: cada paquete lleva el gyro que
    /// corresponde al giro real entre la muestra anterior y esta.
    struct Phone {
        q: Quat,
        t: u64,
    }

    impl Phone {
        fn new() -> Self {
            Self { q: Quat { w: 1.0, x: 0.0, y: 0.0, z: 0.0 }, t: 0 }
        }

        /// Paquete que salta directo a `target` (una sola muestra).
        fn make(&mut self, target: Quat, rec: u8) -> InputPacket {
            let (axis, ang) = delta_axis_angle(self.q, target);
            let dt = DT_US as f32 / 1e6;
            let k = ang / dt;
            self.q = target;
            self.t += DT_US;
            let mut p = packet(arr(target), rec, self.t, FLAG_QUAT_VALID);
            p.gyro = [axis[0] * k, axis[1] * k, axis[2] * k];
            p
        }

        /// Gira suave hasta `target` en `steps` muestras. Devuelve la última
        /// salida Abs vista (o el centro si no hubo ninguna).
        fn turn(&mut self, e: &mut PointerEngine, target: Quat, steps: u32) -> (f32, f32) {
            let (axis, ang) = delta_axis_angle(self.q, target);
            let q0 = self.q;
            let mut out = (0.5, 0.5);
            for i in 1..=steps {
                let qi = q0.mul(qrot_axis(axis, ang * i as f32 / steps as f32));
                let p = self.make(qi, 0);
                if let PointerOutput::Abs { nx, ny } = ap(e, &p) {
                    out = (nx, ny);
                }
            }
            out
        }

        /// Mantiene la orientación actual `steps` muestras.
        fn hold(&mut self, e: &mut PointerEngine, steps: u32) -> (f32, f32) {
            let mut out = (0.5, 0.5);
            for _ in 0..steps {
                let q = self.q;
                let p = self.make(q, 0);
                if let PointerOutput::Abs { nx, ny } = ap(e, &p) {
                    out = (nx, ny);
                }
            }
            out
        }
    }

    fn ap(e: &mut PointerEngine, p: &InputPacket) -> PointerOutput {
        e.apply(p, 35.0, 16.0 / 9.0, true, 1920.0)
    }

    #[test]
    fn recentrado_centra_y_yaw_derecha_mueve_derecha() {
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        assert_eq!(ap(&mut e, &p), PointerOutput::Abs { nx: 0.5, ny: 0.5 });

        ph.turn(&mut e, qrot_z(-10.0), 40);
        let (nx, _) = ph.hold(&mut e, 200);
        let expected = 0.5 + 10.0 / 35.0;
        assert!((nx - expected).abs() < 0.01, "nx={nx} esperado={expected}");
    }

    #[test]
    fn pitch_arriba_sube_el_cursor() {
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_x(0.0), 0);
        ap(&mut e, &p);
        ph.turn(&mut e, qrot_x(5.0), 40);
        let (_, ny) = ph.hold(&mut e, 200);
        let expected = 0.5 - (5.0 / 35.0) * (16.0 / 9.0);
        assert!((ny - expected).abs() < 0.01, "ny={ny} esperado={expected}");
    }

    #[test]
    fn el_roll_no_afecta_al_cursor() {
        // Mismo apuntado (yaw -10°) con y sin 70° de roll sobre el eje de
        // apuntado: el cursor debe acabar en el mismo sitio.
        let mut e1 = PointerEngine::new();
        let mut ph1 = Phone::new();
        let p = ph1.make(qrot_z(0.0), 0);
        ap(&mut e1, &p);
        ph1.turn(&mut e1, qrot_z(-10.0), 40);
        let (nx1, ny1) = ph1.hold(&mut e1, 400);

        let mut e2 = PointerEngine::new();
        let mut ph2 = Phone::new();
        // Recentra YA rolado 40°...
        let p = ph2.make(qrot_y(40.0), 0);
        ap(&mut e2, &p);
        // ...y apunta con 70° de roll: q = giro mundial ∘ roll local (el roll
        // sobre device-Y no cambia el eje de apuntado)
        ph2.turn(&mut e2, qrot_z(-10.0).mul(qrot_y(70.0)), 160);
        let (nx2, ny2) = ph2.hold(&mut e2, 400);

        assert!(
            (nx1 - nx2).abs() < 2e-3 && (ny1 - ny2).abs() < 2e-3,
            "sin roll ({nx1},{ny1}) vs con roll ({nx2},{ny2})"
        );
    }

    #[test]
    fn recentrar_rolado_mantiene_los_ejes_del_mundo() {
        // Recentrado con el móvil rolado 90°: mover la muñeca en horizontal
        // (yaw del mundo) debe seguir moviendo el cursor SOLO en horizontal.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_y(90.0), 0);
        ap(&mut e, &p);
        ph.turn(&mut e, qrot_z(-10.0).mul(qrot_y(90.0)), 40);
        let (nx, ny) = ph.hold(&mut e, 300);
        let expected = 0.5 + 10.0 / 35.0;
        assert!((nx - expected).abs() < 0.01, "nx={nx} esperado={expected}");
        assert!((ny - 0.5).abs() < 0.01, "ny={ny} debería seguir centrado");
    }

    #[test]
    fn asentamiento_del_sensor_no_manda_el_cursor_a_la_esquina() {
        // El rotation vector arranca en identidad y a los ~500 ms "salta" a
        // la orientación real (aquí pitch -40°). Sin el guardián, la
        // referencia falsa clavaba el cursor abajo; con él, se re-ancla y el
        // cursor queda en el centro, y el apuntado posterior funciona.
        let mut e = PointerEngine::new();
        // primeras muestras: identidad (sensor sin asentar)
        let mut t = 0u64;
        for _ in 0..100 {
            t += 5_000;
            e.apply(&packet(arr(qrot_z(0.0)), 0, t, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        }
        // el sensor se asienta: orientación real pitch -40°. Es una
        // CORRECCIÓN del móvil (sin gyro): el gyro va a cero, como en la vida
        // real — por eso aquí se usan paquetes crudos, no el Phone coherente.
        let mut out = (0.0, 0.0);
        for _ in 0..100 {
            t += 5_000;
            if let PointerOutput::Abs { nx, ny } =
                e.apply(&packet(arr(qrot_x(-40.0)), 0, t, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0)
            {
                out = (nx, ny);
            }
        }
        assert!(
            (out.0 - 0.5).abs() < 0.02 && (out.1 - 0.5).abs() < 0.02,
            "el cursor debería quedar centrado tras re-anclar, está en {out:?}"
        );
        // y el apuntado relativo a la nueva referencia funciona
        let mut ph = Phone { q: qrot_x(-40.0), t };
        ph.turn(&mut e, qrot_z(-5.0).mul(qrot_x(-40.0)), 40);
        let (nx, _) = ph.hold(&mut e, 300);
        let expected = 0.5 + 5.0 / 35.0;
        assert!((nx - expected).abs() < 0.02, "nx={nx} esperado={expected}");
    }

    #[test]
    fn raton_real_libre_congelado_y_continuidad_al_retomar() {
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);
        ph.hold(&mut e, 400);
        // Congelado: SILENCIO (el ratón real queda libre)
        for _ in 0..50 {
            let q = ph.q;
            let p = ph.make(q, 0);
            assert_eq!(ap(&mut e, &p), PointerOutput::None, "congelado debe callar");
        }
        // El ratón real dejó el cursor en (0.3, 0.7); el móvil retoma
        e.set_cursor_hint(Some((0.3, 0.7)));
        let mut first_abs: Option<(f32, f32)> = None;
        for i in 1..100 {
            let deg = -0.02 * i as f32 * 4.0; // rampa que escapa del freeze
            let p = ph.make(qrot_z(deg), 0);
            if let PointerOutput::Abs { nx, ny } = ap(&mut e, &p) {
                first_abs = Some((nx, ny));
                break;
            }
        }
        let (nx, ny) = first_abs.expect("debería descongelar");
        assert!(
            (nx - 0.3).abs() < 0.03 && (ny - 0.7).abs() < 0.03,
            "debe continuar desde el cursor real (0.3,0.7), fue ({nx},{ny})"
        );
    }

    #[test]
    fn descongelar_sin_salto() {
        // Parar (congela) y reanudar despacio: el cursor debe fluir SIN
        // teletransporte. Antes del puente, al superar el escape de 0.35°
        // saltaba ~0.01 en nx de un sample al siguiente.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);
        // ir a -5° y quedarse quieto hasta congelar
        ph.turn(&mut e, qrot_z(-5.0), 40);
        ph.hold(&mut e, 400);
        // rampa lenta a 3°/s: recoger salidas y medir el salto máximo
        let mut last_nx: Option<f32> = None;
        let mut max_jump = 0.0f32;
        for i in 0..300 {
            let deg = -5.0 - 3.0 * (i as f32 * 0.005);
            let p = ph.make(qrot_z(deg), 0);
            if let PointerOutput::Abs { nx, .. } = ap(&mut e, &p) {
                if let Some(prev) = last_nx {
                    max_jump = max_jump.max((nx - prev).abs());
                }
                last_nx = Some(nx);
            }
        }
        assert!(
            max_jump < 0.003,
            "salto máximo por sample = {max_jump} (teletransporte)"
        );
    }

    #[test]
    fn nuevo_recentrado_vuelve_al_centro() {
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);
        ph.turn(&mut e, qrot_z(-20.0), 40);
        ph.hold(&mut e, 50);
        let p = ph.make(qrot_z(-20.0), 1);
        assert_eq!(ap(&mut e, &p), PointerOutput::Abs { nx: 0.5, ny: 0.5 });
    }

    #[test]
    fn latigazo_con_tilt_espurio_ni_sube_ni_pierde_recorrido() {
        // El caso real reportado: gesto rápido → el GAME_ROTATION_VECTOR del
        // móvil mete un pitch falso (acelerómetro contaminado) y el cursor
        // "subía solo" y perdía recorrido. El gyro del paquete lleva la
        // verdad (solo yaw): el receptor debe rechazar el tilt espurio y
        // completar todo el recorrido.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);
        ph.hold(&mut e, 100);

        let dt = DT_US as f32 / 1e6;
        let mut max_ny_dev = 0.0f32;
        let mut last = (0.5, 0.5);
        let mut track = |out: PointerOutput, max_ny_dev: &mut f32, last: &mut (f32, f32)| {
            if let PointerOutput::Abs { nx, ny } = out {
                *max_ny_dev = max_ny_dev.max((ny - 0.5).abs());
                *last = (nx, ny);
            }
        };

        // Latigazo: yaw 0→-15° en 20 muestras (150°/s). El quat enviado
        // arrastra ADEMÁS un pitch espurio de hasta 6°; el acelerómetro
        // delata el gesto (‖a‖ = 14 m/s²).
        let mut prev_true = qrot_z(0.0);
        for i in 1..=20 {
            let f = i as f32 / 20.0;
            let true_q = qrot_z(-15.0 * f);
            let sent_q = qrot_z(-15.0 * f).mul(qrot_x(6.0 * f));
            let (axis, ang) = delta_axis_angle(prev_true, true_q);
            prev_true = true_q;
            ph.q = sent_q;
            ph.t += DT_US;
            let mut p = packet(arr(sent_q), 0, ph.t, FLAG_QUAT_VALID);
            let k = ang / dt;
            p.gyro = [axis[0] * k, axis[1] * k, axis[2] * k];
            p.accel = [0.0, 0.0, 14.0];
            track(ap(&mut e, &p), &mut max_ny_dev, &mut last);
        }
        // La fusión del móvil "sana" su pitch espurio en 60 muestras (calma)
        for i in 1..=60 {
            let spur = 6.0 * (1.0 - i as f32 / 60.0);
            let sent_q = qrot_z(-15.0).mul(qrot_x(spur));
            ph.q = sent_q;
            ph.t += DT_US;
            let p = packet(arr(sent_q), 0, ph.t, FLAG_QUAT_VALID);
            track(ap(&mut e, &p), &mut max_ny_dev, &mut last);
        }
        // reposo
        for _ in 0..200 {
            ph.t += DT_US;
            let p = packet(arr(qrot_z(-15.0)), 0, ph.t, FLAG_QUAT_VALID);
            track(ap(&mut e, &p), &mut max_ny_dev, &mut last);
        }

        // Sin la fusión del receptor la excursión vertical sería
        // 6/35·16/9 ≈ 0.30 de pantalla; exigimos rechazar ≥ 80%
        assert!(max_ny_dev < 0.06, "el cursor subió {max_ny_dev} (tilt espurio no rechazado)");
        let expected = 0.5 + 15.0 / 35.0;
        assert!(
            (last.0 - expected).abs() < 0.03,
            "recorrido incompleto: nx={} esperado={expected}",
            last.0
        );
        // Y tras asentarse, el cursor queda en la vertical correcta (sin
        // residuo pegado): el tilt espurio ya no deja poso.
        assert!(
            (last.1 - 0.5).abs() < 0.02,
            "residuo vertical tras el latigazo: ny={}",
            last.1
        );
    }

    #[test]
    fn quat_a_50hz_con_gyro_a_200hz_sale_suave_y_completo() {
        // Móviles que capan el rotation vector a 50 Hz: el quat llega en
        // escalera (se repite 4 muestras) pero el gyro va fino a 200 Hz.
        // El fusionado debe seguir al gyro (suave), no a la escalera.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);
        ph.hold(&mut e, 100);

        let dt = DT_US as f32 / 1e6;
        let mut prev_true = qrot_z(0.0);
        let mut stale = qrot_z(0.0);
        let mut last_nx: Option<f32> = None;
        let mut max_jump = 0.0f32;
        let mut final_nx = 0.5f32;
        for i in 1..=100 {
            let true_q = qrot_z(-20.0 * i as f32 / 100.0); // 40°/s
            if i % 4 == 0 {
                stale = true_q;
            }
            let (axis, ang) = delta_axis_angle(prev_true, true_q);
            prev_true = true_q;
            ph.t += DT_US;
            let mut p = packet(arr(stale), 0, ph.t, FLAG_QUAT_VALID);
            let k = ang / dt;
            p.gyro = [axis[0] * k, axis[1] * k, axis[2] * k];
            if let PointerOutput::Abs { nx, .. } = ap(&mut e, &p) {
                if let Some(prev) = last_nx {
                    max_jump = max_jump.max((nx - prev).abs());
                }
                last_nx = Some(nx);
                final_nx = nx;
            }
        }
        // paso ideal por muestra: 0.2°/35 ≈ 0.0057; la escalera cruda daría
        // saltos de 4× (~0.023)
        assert!(max_jump < 0.009, "escalera visible: salto máx {max_jump}");
        // recorrido completo tras asentar
        ph.q = qrot_z(-20.0);
        let (nx, _) = ph.hold(&mut e, 200);
        let _ = final_nx;
        let expected = 0.5 + 20.0 / 35.0;
        assert!((nx - expected).abs() < 0.02, "nx={nx} esperado={expected}");
    }

    #[test]
    fn cerca_del_polo_el_yaw_no_da_latigazos() {
        // Apuntando casi vertical (|pitch| ≳ 81°) el yaw del eje de apuntado
        // es indefinido: girar rápido ahí no debe barrer el cursor en X.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        ph.q = qrot_x(84.0);
        let q0 = ph.q;
        let p = ph.make(q0, 0);
        ap(&mut e, &p);
        for i in 1..=200 {
            let q = qrot_z(120.0 * i as f32 / 200.0).mul(qrot_x(84.0));
            let p = ph.make(q, 0);
            if let PointerOutput::Abs { nx, .. } = ap(&mut e, &p) {
                assert!((nx - 0.5).abs() < 0.03, "latigazo de yaw en el polo: nx={nx}");
            }
        }
    }

    #[test]
    fn en_reposo_con_gyro_sesgado_el_cursor_no_deriva() {
        // El caso reportado: móvil QUIETO pero el gyro trae sesgo + ruido
        // (todo MEMS lo tiene). La fusión no debe integrar esa deriva: el
        // cursor tiene que quedarse clavado y acabar CONGELADO (soltando el
        // ratón real). El quat, en reposo, es estable: manda él.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);

        // sesgo de gyro de 1.5°/s en yaw + ruido pseudoaleatorio; el quat
        // permanece en la orientación real (0,0) porque el móvil está quieto
        let bias = 1.5_f32.to_radians();
        let mut seed = 12345u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / (1 << 24) as f32 - 0.5 // ±0.5
        };
        let mut max_dev = 0.0f32;
        let mut frozen_tail = 0;
        for i in 0..600 {
            ph.t += DT_US;
            let mut p = packet(arr(qrot_z(0.0)), 0, ph.t, FLAG_QUAT_VALID);
            // gyro = sesgo + ruido (en ejes del dispositivo, aquí Z=yaw)
            let noise = 2.0_f32.to_radians();
            p.gyro = [rnd() * noise, rnd() * noise, bias + rnd() * noise];
            p.accel = [0.0, 0.0, G_MS2];
            match ap(&mut e, &p) {
                PointerOutput::Abs { nx, ny } => {
                    max_dev = max_dev.max(((nx - 0.5).hypot(ny - 0.5)).abs());
                    frozen_tail = 0;
                }
                PointerOutput::None => frozen_tail += 1,
                _ => {}
            }
            let _ = i;
        }
        // el cursor no se va (deriva < ~1.5% de pantalla en todo el minuto)
        assert!(max_dev < 0.015, "el cursor derivó {max_dev} en reposo");
        // y acaba congelado: las últimas muestras no inyectan nada
        assert!(frozen_tail > 20, "no llegó a congelar en reposo ({frozen_tail})");
    }

    #[test]
    fn flick_no_pierde_recorrido() {
        // Flick de 25° en 100 ms: nada de infra-recorrido — a los 60 ms de
        // parar, el cursor tiene que estar clavado en el destino.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);
        ph.hold(&mut e, 100);
        ph.turn(&mut e, qrot_z(-25.0), 20); // 250°/s
        let (nx, _) = ph.hold(&mut e, 12);
        let expected = 0.5 + 25.0 / 35.0;
        assert!(
            (nx - expected).abs() < 0.036,
            "infra-recorrido tras flick: nx={nx} esperado={expected}"
        );
    }

    #[test]
    fn fallback_sin_quat_no_deriva_en_reposo() {
        // Móvil viejo sin rotation vector, quieto: el gyro trae sesgo bajo
        // (< zona muerta). El cursor NO debe moverse.
        let mut e = PointerEngine::new();
        let mut p = packet([0.0; 4], 0, 0, 0); // sin FLAG_QUAT_VALID
        p.gyro = [0.015, 0.0, 0.02]; // ~1°/s de sesgo, por debajo de la zona muerta
        e.apply(&p, 34.0, 16.0 / 9.0, true, 1920.0);
        let mut moved = false;
        for i in 1..400 {
            p.t_sensor_us = i * 5_000;
            if !matches!(e.apply(&p, 34.0, 16.0 / 9.0, true, 1920.0), PointerOutput::None) {
                moved = true;
            }
        }
        assert!(!moved, "el sesgo del gyro movió el cursor en reposo (fallback)");
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

    #[test]
    fn wrap180_funciona() {
        assert!((wrap180(190.0) + 170.0).abs() < 1e-4);
        assert!((wrap180(-190.0) - 170.0).abs() < 1e-4);
        assert!((wrap180(10.0) - 10.0).abs() < 1e-4);
    }
}
