//! Motor de puntero h3: apuntado absoluto ANCLADO AL MUNDO; el gyro manda y
//! el rotation vector solo ancla, en silencio.
//!
//! El quaternion (GAME_ROTATION_VECTOR) lleva el dispositivo al marco del
//! mundo, cuyo eje Z es la gravedad. El eje de apuntado del móvil (device +Y)
//! se proyecta al mundo y se descompone ahí: pitch = elevación sobre el
//! horizonte real, yaw = ángulo en el plano horizontal real. El ROLL del
//! dispositivo no aparece en ninguna de las dos coordenadas → rotar el móvil
//! mientras apuntas (Wii Sports) no afecta al cursor, ni rolado al recentrar.
//!
//! Quién mueve el cursor: el GYRO, integrado en ese marco del mundo. Es la
//! verdad de cómo se mueve la mano, sin retraso y sin la inclinación falsa
//! que el acelerómetro mete en el rotation vector durante un gesto. El quat
//! solo ANCLA el estado interno cuando el móvil está quieto (cursor
//! congelado), y lo hace en silencio: ninguna corrección mueve el cursor.
//! Ver el bloque «El gyro manda, el quat ancla».
//!
//! Fallback: integración relativa del gyro para móviles sin rotation vector
//! (flags bit0 = 0); ese camino sí es sensible al roll (ejes del dispositivo).

use super::one_euro::Filter2D;
use crate::net::codec::{InputPacket, FLAG_QUAT_VALID};

/// Congelación: clavado y en silencio SOLO con la mano quieta de verdad, y
/// continuo en cuanto se mueve. Con una banda de histéresis ancha (antes
/// 0,6-1,8°/s) el movimiento lento y suave caía dentro y el cursor iba a
/// trompicones (medido: 79 congelaciones en 4 min de barridos a ~1°/s).
/// Ahora: entra por debajo de `FREEZE_ENTER_DEG_S` mantenido `FREEZE_DWELL_US`
/// (nada de parpadeos en el umbral), y sale en cuanto el gyro supera
/// `FREEZE_EXIT_DEG_S` o acumula un movimiento sostenido (escape), que se
/// recupera al salir en vez de perderse.
const FREEZE_ENTER_DEG_S: f32 = 0.25;
const FREEZE_EXIT_DEG_S: f32 = 0.5;
const FREEZE_DWELL_US: u64 = 300_000;
/// Permanencia más corta cuando es el quat quien dice que el móvil está
/// quieto (el gyro solo trae sesgo): no hay movimiento real que proteger.
const FREEZE_DWELL_QUAT_US: u64 = 100_000;
/// Salida por velocidad: el gyro por encima de `FREEZE_EXIT_DEG_S` con el
/// quat confirmando (por encima de esto), o el gyro por encima de la
/// tolerancia al sesgo (eso ya no puede ser sesgo, diga lo que diga el quat).
const FREEZE_EXIT_QUAT_DEG_S: f32 = 0.3;
/// Hasta esta velocidad del gyro (°/s) el quat puede desmentirlo («eso es
/// sesgo, el móvil está quieto»); por encima, si el gyro dice que se mueve,
/// se mueve, diga lo que diga el quat (que puede haberse quedado colgado).
const FREEZE_BIAS_TOLERANCE_DEG_S: f32 = 3.0;
/// El sesgo del gyro solo se aprende con el móvil quieto DE VERDAD: llevando
/// congelado al menos esto (µs) y con el quat sin haberse movido más de
/// `BIAS_LEARN_QUAT_DEG` desde que se congeló. (Un quat «pegajoso» durante un
/// movimiento lento parece quieto un instante; en 400 ms ya no.)
const BIAS_LEARN_AFTER_US: u64 = 400_000;
const BIAS_LEARN_QUAT_DEG: f32 = 0.1;
/// Escape por movimiento sostenido: acumulación con fuga (τ = 1 s) del giro
/// del gyro y del quat desde que se congeló; el ruido no la llena, un
/// movimiento lento sí. Hace falta que gyro Y quat lo vean (ni el sesgo del
/// gyro ni una corrección del quat descongelan).
const FREEZE_ESCAPE_DEG: f32 = 0.08;
const FREEZE_ESCAPE_QUAT_DEG: f32 = 0.04;
const FREEZE_LEAK_TAU_S: f32 = 1.0;

/// Signos del fallback relativo (h1). Corrección SOLO aquí.
const SIGN_X: f32 = -1.0;
const SIGN_Y: f32 = -1.0;

// --- El gyro manda, el quat ancla (en silencio) ---
// El GAME_ROTATION_VECTOR del móvil es rocoso en REPOSO (anclado a la
// gravedad, sin deriva), pero en un gesto RÁPIDO su acelerómetro mide
// gravedad + aceleración del gesto y mete una inclinación FALSA que tarda
// hasta segundos en sanar; además llega con algo de retraso respecto al
// gyro. Si el cursor siguiera al quat durante o justo después de un gesto,
// se iría a un sitio que la mano no ha mandado (tirón en otra dirección al
// parar) y la velocidad iría a trompicones (cada corrección es un frenazo o
// un acelerón). El gyro es lo contrario: fiel al gesto, pero su sesgo
// integrado deriva en reposo. Por eso:
//  - En movimiento NO se corrige nada: la trayectoria es 1:1 con el gyro
//    (menos su sesgo, que se estima en reposo).
//  - Congelado (móvil quieto): el estado interno converge al quat, en
//    SILENCIO (congelado no se emite nada); al descongelar, el puente
//    absorbe la diferencia y el cursor arranca de donde estaba, sin salto.
//  - El puente (lo que ve el usuario menos el apuntado absoluto) se disuelve
//    SOLO como una fracción del movimiento que ordena la mano, en la
//    dirección de ese movimiento: se nota, como mucho, como un 12 % de
//    ganancia de más o de menos durante un instante, nunca como un
//    movimiento por su cuenta ni lateral.
/// λ (1/s) con el que el estado converge al quat mientras está congelado.
const ANCHOR_LAMBDA: f32 = 8.0;
/// Fracción del movimiento ordenado que puede ir a disolver el puente.
const BRIDGE_DISSOLVE_FRACTION: f32 = 0.12;
/// Apuntado absoluto: si el cursor real se aleja más que esto (fracción de
/// pantalla) de donde lo dejamos, es que el SO lo recortó en un borde o el
/// ratón lo movió: se sigue desde donde está de verdad.
const HINT_TOL: f32 = 0.01;
/// …y si se aleja más que esto (fracción de pantalla), no es un borde sino
/// el ratón real: ese desplazamiento es permanente (`shift`). Lo de los
/// bordes va al puente y se disuelve con el movimiento: el apuntado absoluto
/// vuelve a su sitio. Si no, con un monitor encima el recorte de abajo (el de
/// arriba no recorta: el cursor pasa al otro monitor) hace de trinquete y el
/// cursor acaba arriba del todo haciendo círculos.
const HINT_MOUSE_JUMP: f32 = 0.08;

/// λ (1/s) del estimador de sesgo del gyro (solo aprende congelado: ahí el
/// gyro debería leer cero y lo que lee es sesgo).
const BIAS_LAMBDA: f32 = 0.7;
/// Sesgo máximo creíble (rad/s ≈ 6°/s): más que eso no es sesgo, es que el
/// móvil se mueve aunque el quat aún no lo diga.
const BIAS_MAX_RADS: f32 = 0.1;
/// Cutoff (Hz) del suavizado de las velocidades que deciden la congelación.
const RATE_CUTOFF_HZ: f32 = 2.0;
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

/// Paso-bajo de primer orden con la constante de tiempo de `cutoff` Hz.
fn lowpass(prev: Option<f32>, x: f32, cutoff: f32, dt: f32) -> f32 {
    match prev {
        Some(p) => {
            let tau = 1.0 / (std::f32::consts::TAU * cutoff);
            let a = 1.0 / (1.0 + tau / dt);
            p + (x - p) * a
        }
        None => x,
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

    /// d(yaw)/dt y d(pitch)/dt (°/s) de un giro `gyro` (rad/s, ejes del
    /// dispositivo) proyectado al mundo con este marco: ḋ = ω × d. El tercer
    /// valor es «polo»: apuntando casi vertical el yaw es indefinido.
    fn world_rates(self, gyro: [f32; 3]) -> (f32, f32, bool) {
        let d = self.pointing_dir_world();
        let w = self.rotate(gyro);
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
        (if pole { 0.0 } else { dyaw.to_degrees() }, dpitch.to_degrees(), pole)
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
    /// Último (yaw, pitch) filtrado: el paso de movimiento ordenado.
    last_filtered: Option<(f32, f32)>,
    /// Puente: lo que ve el usuario menos el apuntado absoluto filtrado. Se
    /// fija al descongelar (salto cero) y solo se disuelve con el movimiento.
    offset: (f32, f32),
    /// Desplazamiento permanente del apuntado (hasta recentrar): lo que el
    /// SO recortó en los bordes de la pantalla y lo que movió el ratón real.
    /// Como con un ratón: al volver de un borde el cursor responde al
    /// instante, y no hay recorrido invisible que deshacer.
    shift: (f32, f32),
    /// t de la primera muestra: ventana de asentamiento del rotation vector.
    first_t_us: Option<u64>,
    /// Posición real del cursor (normalizada), si el SO la sabe: al
    /// descongelar, el puntero continúa desde donde el RATÓN lo dejó.
    cursor_hint: Option<(f32, f32)>,
    last_t_us: Option<u64>,
    /// (yaw, pitch) del mundo integrados del gyro; anclados al quat en reposo.
    fused: Option<(f32, f32)>,
    /// Giro del gyro acumulado desde el recentrado, SIN anclar: lo que la
    /// mano ha girado de verdad (guardián de asentamiento).
    raw_int: (f32, f32),
    /// Congelado: giro del gyro acumulado desde que se congeló (exacto, para
    /// recuperarlo al salir) y con fuga (para el escape); ídem del quat.
    freeze_gyro: (f32, f32),
    freeze_gyro_leaky: (f32, f32),
    freeze_quat_leaky: (f32, f32),
    /// Desde cuándo la mano está por debajo del umbral de congelar (µs).
    still_since: Option<u64>,
    /// Cuándo se congeló y dónde estaba el quat entonces (aprendizaje del
    /// sesgo solo con el móvil quieto de verdad).
    frozen_at: Option<(u64, f32, f32)>,
    /// Velocidad del quat suavizada (°/s por eje): «el quat ve movimiento».
    quat_rate: Option<(f32, f32)>,
    last_quat: Option<(f32, f32)>,
    /// Sesgo estimado del gyro (rad/s, ejes del dispositivo).
    bias: [f32; 3],
    // fallback relativo
    acc_x: f32,
    acc_y: f32,
}

impl PointerEngine {
    pub fn new() -> Self {
        Self {
            ref_angles: None,
            last_recenter: None,
            // Filtro casi transparente. Con el gyro mandando, la señal ya
            // llega limpia (jitter en reposo < 1 px con y sin filtro, medido
            // en grabaciones reales); lo que sí cuesta es el retardo: con el
            // ajuste antiguo (1 Hz, β 0,2) el cursor iba 30-50 px por detrás
            // de la mano a velocidad normal y 170 px en un flick. Con 10 Hz y
            // β 1 (a 60°/s el cutoff ya es 70 Hz) se queda en 6-12 px a
            // cualquier velocidad: el cursor es la mano.
            filter: Filter2D::new(10.0, 1.0),
            frozen: false,
            last_emitted: None,
            last_filtered: None,
            offset: (0.0, 0.0),
            shift: (0.0, 0.0),
            first_t_us: None,
            cursor_hint: None,
            last_t_us: None,
            fused: None,
            raw_int: (0.0, 0.0),
            freeze_gyro: (0.0, 0.0),
            freeze_gyro_leaky: (0.0, 0.0),
            freeze_quat_leaky: (0.0, 0.0),
            still_since: None,
            frozen_at: None,
            quat_rate: None,
            last_quat: None,
            bias: [0.0; 3],
            acc_x: 0.0,
            acc_y: 0.0,
        }
    }

    /// Estado (yaw, pitch) del mundo y velocidades del gyro (°/s): el gyro
    /// (sin sesgo) integrado siempre; congelado, además converge al quat en
    /// silencio y aprende el sesgo. Ver el bloque «El gyro manda».
    fn track(&mut self, p: &InputPacket, q: Quat, qyaw: f32, qpitch: f32, dt: Option<f32>) -> (f32, f32, f32, f32) {
        // Primera muestra o hueco grande (suspensión, pérdida): el quat es
        // la mejor verdad disponible.
        let Some(dt) = dt else {
            self.fused = Some((qyaw, qpitch));
            self.quat_rate = None;
            self.last_quat = Some((qyaw, qpitch));
            return (qyaw, qpitch, 0.0, 0.0);
        };
        let (mut fy, mut fp) = self.fused.unwrap_or((qyaw, qpitch));

        // Velocidad del quat (suavizada): decide, junto con el gyro, si el
        // móvil está quieto de verdad
        let (lqy, lqp) = self.last_quat.replace((qyaw, qpitch)).unwrap_or((qyaw, qpitch));
        let (ry, rp) = (wrap180(qyaw - lqy) / dt, (qpitch - lqp) / dt);
        let prev = self.quat_rate;
        self.quat_rate = Some((
            lowpass(prev.map(|r| r.0), ry, RATE_CUTOFF_HZ, dt),
            lowpass(prev.map(|r| r.1), rp, RATE_CUTOFF_HZ, dt),
        ));

        let truly_still = self.frozen
            && self.frozen_at.is_some_and(|(t0, y0, p0)| {
                p.t_sensor_us.saturating_sub(t0) >= BIAS_LEARN_AFTER_US
                    && wrap180(qyaw - y0).hypot(qpitch - p0) < BIAS_LEARN_QUAT_DEG
            });
        if truly_still {
            // Quieto de verdad: lo que lee el gyro es sesgo. Acotado: más
            // que eso no es sesgo, es movimiento.
            let l = 1.0 - (-dt * BIAS_LAMBDA).exp();
            for i in 0..3 {
                let target = p.gyro[i].clamp(-BIAS_MAX_RADS, BIAS_MAX_RADS);
                self.bias[i] += (target - self.bias[i]) * l;
            }
        }
        let gyro = [p.gyro[0] - self.bias[0], p.gyro[1] - self.bias[1], p.gyro[2] - self.bias[2]];
        let (dyaw, dpitch, pole) = q.world_rates(gyro);
        let (gy, gp) = (dyaw * dt, dpitch * dt);
        fy += gy;
        fp += gp;
        self.raw_int.0 += gy;
        self.raw_int.1 += gp;

        if self.frozen {
            self.freeze_gyro.0 += gy;
            self.freeze_gyro.1 += gp;
            let leak = dt / FREEZE_LEAK_TAU_S;
            self.freeze_gyro_leaky.0 += gy - self.freeze_gyro_leaky.0 * leak;
            self.freeze_gyro_leaky.1 += gp - self.freeze_gyro_leaky.1 * leak;
            if let Some((qry, qrp)) = self.quat_rate {
                self.freeze_quat_leaky.0 += qry * dt - self.freeze_quat_leaky.0 * leak;
                self.freeze_quat_leaky.1 += qrp * dt - self.freeze_quat_leaky.1 * leak;
            }
            // Anclaje silencioso al quat: congelado no se emite nada, y al
            // descongelar el puente absorbe la diferencia
            let l = 1.0 - (-dt * ANCHOR_LAMBDA).exp();
            if !pole {
                fy += wrap180(qyaw - fy) * l;
            }
            fp += (qpitch - fp) * l;
        }

        self.fused = Some((fy, fp));
        (fy, fp, dyaw, dpitch)
    }

    /// El puente solo se disuelve con el movimiento que ordena la mano: como
    /// mucho `BRIDGE_DISSOLVE_FRACTION` del paso, y solo su componente en la
    /// dirección del paso (ganancia un poco mayor o menor; nunca lateral ni
    /// por su cuenta).
    fn dissolve_bridge(&mut self, step_yaw: f32, step_pitch: f32) {
        let step = step_yaw.hypot(step_pitch);
        if step < 1e-6 {
            return;
        }
        let (ux, uy) = (step_yaw / step, step_pitch / step);
        let along = self.offset.0 * ux + self.offset.1 * uy;
        let allowed = BRIDGE_DISSOLVE_FRACTION * step;
        let reduce = along.clamp(-allowed, allowed);
        self.offset.0 -= ux * reduce;
        self.offset.1 -= uy * reduce;
    }

    /// Recentrado o re-anclaje: todo a cero alrededor de (yaw, pitch).
    fn rebase(&mut self, yaw: f32, pitch: f32) {
        self.ref_angles = Some((yaw, pitch));
        self.fused = Some((yaw, pitch));
        self.filter.reset();
        self.frozen = false;
        self.offset = (0.0, 0.0);
        self.shift = (0.0, 0.0);
        self.raw_int = (0.0, 0.0);
        self.last_emitted = Some((0.0, 0.0));
        self.last_filtered = None;
        self.freeze_gyro = (0.0, 0.0);
        self.freeze_gyro_leaky = (0.0, 0.0);
        self.freeze_quat_leaky = (0.0, 0.0);
        self.still_since = None;
        self.frozen_at = None;
    }

    /// Apuntado absoluto: si el cursor real no está donde lo dejamos (el SO
    /// lo recortó en un borde, o el ratón lo movió), se sigue desde donde
    /// está DE VERDAD, sin zona muerta ni salto. Un salto grande es el ratón:
    /// desplazamiento permanente (`shift`). Lo pequeño y continuo es un
    /// borde: va al puente, que se disuelve con el movimiento, y el apuntado
    /// absoluto recupera su sitio.
    fn follow_real_cursor(&mut self, sens_deg: f32, aspect: f32) {
        let (Some((cx, cy)), Some((py, pp))) = (self.cursor_hint, self.last_emitted) else {
            return;
        };
        let hy = (cx - 0.5) * sens_deg;
        let hp = (0.5 - cy) * sens_deg / aspect;
        let (dy, dp) = (hy - py, hp - pp);
        let (ny, np) = (dy / sens_deg, dp * aspect / sens_deg); // en pantallas
        if ny.abs() > HINT_TOL || np.abs() > HINT_TOL {
            if ny.abs() > HINT_MOUSE_JUMP || np.abs() > HINT_MOUSE_JUMP {
                self.shift.0 += dy;
                self.shift.1 += dp;
            } else {
                self.offset.0 += dy;
                self.offset.1 += dp;
            }
            self.last_emitted = Some((hy, hp));
        }
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
        let (yaw_w, pitch_w, rate_yaw, rate_pitch) = self.track(p, q, qyaw, qpitch, dt);

        if recentered || self.ref_angles.is_none() {
            // Recentrar lleva el cursor al centro también en modo relativo:
            // es lo que pide el botón (los deltas siguen desde ahí)
            self.rebase(yaw_w, pitch_w);
            return PointerOutput::Abs { nx: 0.5, ny: 0.5 };
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
            // Se mira el QUAT crudo, no el integrado: el salto de
            // asentamiento aparece en el quat al instante (sin gyro).
            // …y solo si el gyro NO lo explica: un giro de verdad lo mide
            // el gyro; el salto de asentamiento del quat, no.
            let qdev_yaw = wrap180(wrap180(qyaw - yaw_ref) - self.raw_int.0);
            let qdev_pitch = qpitch - pitch_ref - self.raw_int.1;
            if qdev_yaw.abs() > lim || qdev_pitch.abs() * aspect_w_over_h > lim {
                // El salto de asentamiento es una corrección del móvil, no un
                // giro: re-anclar DIRECTO al quat.
                self.rebase(qyaw, qpitch);
                return PointerOutput::Abs { nx: 0.5, ny: 0.5 };
            }
        }

        let dt = dt.unwrap_or(0.005);
        // La velocidad que abre el filtro (y que decide congelar) sale del
        // gyro: instantánea, sin el retardo de derivar, y ajena al anclaje.
        let (yaw_f, pitch_f, speed) = self.filter.filter_with_rate(yaw, pitch, rate_yaw, rate_pitch, dt);
        let quat_speed = self.quat_rate.map_or(0.0, |(a, b)| a.hypot(b));
        let (lf_yaw, lf_pitch) = self.last_filtered.replace((yaw_f, pitch_f)).unwrap_or((yaw_f, pitch_f));

        let (prev_yaw, prev_pitch) = self.last_emitted.unwrap_or((yaw_f, pitch_f));

        if self.frozen {
            // Movimiento lento sostenido: el gyro lo acumula y el quat lo
            // confirma (ni sesgo ni corrección descongelan)
            let gdev = self.freeze_gyro_leaky.0.hypot(self.freeze_gyro_leaky.1);
            let qdev = self.freeze_quat_leaky.0.hypot(self.freeze_quat_leaky.1);
            let creeping = gdev > FREEZE_ESCAPE_DEG && qdev > FREEZE_ESCAPE_QUAT_DEG;
            let moving = speed > FREEZE_BIAS_TOLERANCE_DEG_S
                || (speed > FREEZE_EXIT_DEG_S && quat_speed > FREEZE_EXIT_QUAT_DEG_S);
            if moving || creeping {
                // Liberar recuperando lo que la mano giró mientras estaba
                // congelado (unos píxeles), medido por el QUAT (con sesgo del
                // gyro no se ha movido nada y el quat lo sabe): el puente
                // absorbe SOLO lo que el anclaje movió el estado (se disuelve
                // con el movimiento). Si el ratón real movió el cursor, lo
                // recoge `follow_real_cursor`.
                let (ry, rp) = match self.frozen_at {
                    Some((_, y0, p0)) => (wrap180(qyaw - y0), qpitch - p0),
                    None => (0.0, 0.0),
                };
                self.frozen = false;
                self.still_since = None;
                self.frozen_at = None;
                self.offset = (
                    prev_yaw + ry - yaw_f - self.shift.0,
                    prev_pitch + rp - pitch_f - self.shift.1,
                );
            } else {
                // Congelado = SILENCIO: ni un paquete de inyección. El ratón
                // real queda libre mientras el móvil esté quieto.
                return PointerOutput::None;
            }
        }

        // Libre: el puente se disuelve dentro del propio movimiento ordenado
        self.dissolve_bridge(yaw_f - lf_yaw, pitch_f - lf_pitch);
        if abs_mode {
            self.follow_real_cursor(sens_deg, aspect_w_over_h);
        }

        let out_yaw = yaw_f + self.offset.0 + self.shift.0;
        let out_pitch = pitch_f + self.offset.1 + self.shift.1;

        // Quieto: el gyro no ve movimiento, o ve tan poco que puede ser su
        // sesgo y el quat confirma que no hay nada; y mantenido un rato
        // (permanencia), que un roce con el umbral no congele
        let quat_says_still = speed < FREEZE_BIAS_TOLERANCE_DEG_S && quat_speed < FREEZE_ENTER_DEG_S;
        let still = speed < FREEZE_ENTER_DEG_S || quat_says_still;
        if still {
            let since = *self.still_since.get_or_insert(p.t_sensor_us);
            let dwell = if quat_says_still { FREEZE_DWELL_QUAT_US } else { FREEZE_DWELL_US };
            if p.t_sensor_us.saturating_sub(since) >= dwell {
                self.frozen = true;
                self.frozen_at = Some((p.t_sensor_us, qyaw, qpitch));
                self.freeze_gyro = (0.0, 0.0);
                self.freeze_gyro_leaky = (0.0, 0.0);
                self.freeze_quat_leaky = (0.0, 0.0);
            }
        } else {
            self.still_since = None;
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

    const G_MS2: f32 = 9.81;

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

    /// Como `ap`, simulando además el cursor real del SO: la última posición
    /// emitida, recortada a la pantalla (el receptor lo lee antes de cada
    /// paquete). Si `mouse` trae algo, el ratón real lo dejó ahí.
    fn ap_os(e: &mut PointerEngine, p: &InputPacket, mouse: Option<(f32, f32)>) -> PointerOutput {
        if let Some(m) = mouse {
            e.set_cursor_hint(Some(m));
        }
        let out = ap(e, p);
        if let PointerOutput::Abs { nx, ny } = out {
            e.set_cursor_hint(Some((nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0))));
        }
        out
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
        let mut first_abs: Option<(f32, f32)> = None;
        for i in 1..100 {
            let deg = -0.02 * i as f32 * 4.0; // rampa que escapa del freeze
            let p = ph.make(qrot_z(deg), 0);
            if let PointerOutput::Abs { nx, ny } = ap_os(&mut e, &p, Some((0.3, 0.7))) {
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

    /// Móvil realista: el quat llega `lag` muestras por detrás del gyro y,
    /// en un gesto brusco, arrastra un pitch falso que sana en reposo con
    /// una constante de tiempo de ~0,5 s (lo que hace un rotation vector de
    /// verdad). Devuelve las salidas emitidas (t, nx, ny).
    fn realistic_flick(yaw_deg: f32, flick_steps: u32, lag: usize, false_pitch: f32) -> (Vec<(u64, f32, f32)>, u64) {
        let mut e = PointerEngine::new();
        let mut t = 0u64;
        let mut out = Vec::new();
        let mut history: Vec<Quat> = Vec::new();
        let dt = DT_US as f32 / 1e6;
        let mut prev_true = qrot_z(0.0);
        let mut push = |e: &mut PointerEngine, true_q: Quat, sent_q: Quat, t: u64, out: &mut Vec<(u64, f32, f32)>, prev_true: &mut Quat| {
            let (axis, ang) = delta_axis_angle(*prev_true, true_q);
            *prev_true = true_q;
            let mut p = packet(arr(sent_q), 0, t, FLAG_QUAT_VALID);
            let k = ang / dt;
            p.gyro = [axis[0] * k, axis[1] * k, axis[2] * k];
            if let PointerOutput::Abs { nx, ny } = ap(e, &p) {
                out.push((t, nx, ny));
            }
        };
        // reposo inicial (recentrado + congelado)
        for _ in 0..200 {
            t += DT_US;
            history.push(qrot_z(0.0));
            push(&mut e, qrot_z(0.0), qrot_z(0.0), t, &mut out, &mut prev_true);
        }
        // flick: yaw 0 → yaw_deg en flick_steps muestras, con el quat
        // retrasado `lag` muestras y un pitch falso que crece con el gesto
        for i in 1..=flick_steps {
            t += DT_US;
            let f = i as f32 / flick_steps as f32;
            let true_q = qrot_z(yaw_deg * f);
            let sent_true = qrot_z(yaw_deg * f).mul(qrot_x(false_pitch * f));
            history.push(sent_true);
            let sent = history[history.len().saturating_sub(1 + lag)];
            push(&mut e, true_q, sent, t, &mut out, &mut prev_true);
        }
        let t_stop = t;
        // la mano se para en seco; el quat termina de llegar y su pitch
        // falso sana con τ = 0,5 s
        for i in 1..=600u32 {
            t += DT_US;
            let heal = false_pitch * (-(i as f32) * dt / 0.5).exp();
            let sent_true = qrot_z(yaw_deg).mul(qrot_x(heal));
            history.push(sent_true);
            let sent = history[history.len().saturating_sub(1 + lag)];
            push(&mut e, qrot_z(yaw_deg), sent, t, &mut out, &mut prev_true);
        }
        (out, t_stop)
    }

    #[test]
    fn tras_un_flick_el_cursor_no_se_mueve_por_su_cuenta() {
        // El caso reportado: flick brusco y, al parar, el cursor se iba por
        // donde no tocaba (el quat, con retraso y con inclinación falsa,
        // tiraba de él). Ahora: al parar la mano, para el cursor. Se permite
        // solo la cola del filtro en los primeros 40 ms.
        let (out, t_stop) = realistic_flick(-20.0, 16, 3, 5.0);
        let settled = out.iter().find(|(t, _, _)| *t >= t_stop + 40_000).expect("emite tras parar");
        let (sx, sy) = (settled.1, settled.2);
        let mut max_after = 0.0f32;
        for (t, nx, ny) in &out {
            if *t >= t_stop + 40_000 {
                max_after = max_after.max((nx - sx).hypot(ny - sy));
            }
        }
        assert!(max_after < 0.004, "tras parar, el cursor se movió {max_after} por su cuenta");
        // vertical: el pitch falso del quat nunca mueve el cursor
        let max_ny = out.iter().map(|(_, _, ny)| (ny - 0.5).abs()).fold(0.0, f32::max);
        assert!(max_ny < 0.01, "el pitch falso del quat movió el cursor en vertical: {max_ny}");
        // y el recorrido es el de la mano
        let expected = 0.5 + 20.0 / 35.0;
        assert!((sx - expected).abs() < 0.02, "recorrido: nx={sx} esperado={expected}");
    }

    #[test]
    fn barrido_uniforme_sale_uniforme_aunque_el_quat_se_retrase() {
        // A velocidad constante el cursor debe avanzar a velocidad constante:
        // ni frenazos ni acelerones por correcciones (el quat llega 3 muestras
        // tarde). Se mide el paso por muestra una vez pasado el arranque.
        let mut e = PointerEngine::new();
        let mut t = 0u64;
        let dt = DT_US as f32 / 1e6;
        let mut history: Vec<Quat> = Vec::new();
        let mut prev_true = qrot_z(0.0);
        let mut emit = |e: &mut PointerEngine, true_q: Quat, sent: Quat, t: u64, prev_true: &mut Quat| {
            let (axis, ang) = delta_axis_angle(*prev_true, true_q);
            *prev_true = true_q;
            let mut p = packet(arr(sent), 0, t, FLAG_QUAT_VALID);
            let k = ang / dt;
            p.gyro = [axis[0] * k, axis[1] * k, axis[2] * k];
            ap(e, &p)
        };
        for _ in 0..200 {
            t += DT_US;
            history.push(qrot_z(0.0));
            emit(&mut e, qrot_z(0.0), qrot_z(0.0), t, &mut prev_true);
        }
        // barrido a 60°/s durante 0,5 s
        let mut steps: Vec<f32> = Vec::new();
        let mut last_nx: Option<f32> = None;
        for i in 1..=100 {
            t += DT_US;
            let true_q = qrot_z(-0.3 * i as f32);
            history.push(true_q);
            let sent = history[history.len().saturating_sub(4)];
            if let PointerOutput::Abs { nx, .. } = emit(&mut e, true_q, sent, t, &mut prev_true) {
                if let Some(p) = last_nx {
                    if i > 30 {
                        steps.push(nx - p);
                    }
                }
                last_nx = Some(nx);
            }
        }
        let ideal = 0.3 / 35.0;
        let (min, max) = steps.iter().fold((f32::MAX, f32::MIN), |(a, b), s| (a.min(*s), b.max(*s)));
        assert!(
            min > ideal * 0.92 && max < ideal * 1.08,
            "paso por muestra irregular: {min}..{max} (ideal {ideal})"
        );
    }

    #[test]
    fn puente_grande_no_desvia_ni_mueve_por_su_cuenta() {
        // El ratón real dejó el cursor 0,3 de pantalla más abajo; el móvil
        // retoma moviéndose en HORIZONTAL. El puente no puede convertirse en
        // un movimiento vertical (antes se disolvía con el tiempo: tirón
        // hacia donde no apuntaba la mano).
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);
        ph.hold(&mut e, 400);
        let mut prev: Option<(f32, f32)> = None;
        let mut max_dy = 0.0f32;
        let mut first: Option<(f32, f32)> = None;
        let mut last = (0.5, 0.5);
        for i in 1..=200 {
            let p = ph.make(qrot_z(-0.07 * i as f32), 0); // 14°/s, se queda en pantalla
            let mouse = if i == 1 { Some((0.5, 0.8)) } else { None };
            if let PointerOutput::Abs { nx, ny } = ap_os(&mut e, &p, mouse) {
                if let Some((px, py)) = prev {
                    max_dy = max_dy.max((ny - py).abs());
                    assert!(nx >= px - 1e-4, "retrocedió en x: {px} → {nx}");
                }
                if first.is_none() {
                    first = Some((nx, ny));
                }
                prev = Some((nx, ny));
                last = (nx, ny);
            }
        }
        let (fx, fy) = first.expect("emite");
        assert!((fy - 0.8).abs() < 0.03, "debe arrancar donde dejó el ratón (y=0.8), fue {fy}");
        assert!(max_dy < 1e-3, "movimiento vertical no ordenado: {max_dy} por muestra");
        assert!((last.1 - fy).abs() < 0.01, "el cursor se fue en vertical: {} → {}", fy, last.1);
        // ganancia horizontal exacta: lo del ratón es un desplazamiento
        // permanente, no un puente que se disuelva
        let travel = last.0 - fx;
        let ideal = 14.0 / 35.0 * 0.9; // ~lo recorrido tras el arranque
        assert!(travel > ideal * 0.9 && travel < 14.0 / 35.0 * 1.02, "ganancia fuera de rango: {travel}");
    }

    #[test]
    fn volver_de_un_borde_no_tiene_zona_muerta() {
        // Flick hacia arriba que se pasa del borde superior (el SO deja el
        // cursor clavado en y=0). Al bajar la mano, el cursor tiene que bajar
        // AL INSTANTE y 1:1, como un ratón: nada de "deshacer" el recorrido
        // invisible que quedó por encima de la pantalla.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_x(0.0), 0);
        ap_os(&mut e, &p, None);
        for _ in 0..200 {
            let q = ph.q;
            let p = ph.make(q, 0);
            ap_os(&mut e, &p, None);
        }
        // flick: +30° de pitch en 20 muestras (300°/s): ny = 0.5 - 30/35·16/9 < 0
        let (axis, ang) = delta_axis_angle(ph.q, qrot_x(30.0));
        let q0 = ph.q;
        let mut min_ny = 1.0f32;
        for i in 1..=20 {
            let qi = q0.mul(qrot_axis(axis, ang * i as f32 / 20.0));
            let p = ph.make(qi, 0);
            if let PointerOutput::Abs { ny, .. } = ap_os(&mut e, &p, None) {
                min_ny = min_ny.min(ny);
            }
        }
        assert!(min_ny < 0.0, "el flick debería pasarse del borde (ny={min_ny})");
        // la mano baja 5° a 50°/s (sin pararse: no hay congelación)
        let (axis, ang) = delta_axis_angle(ph.q, qrot_x(25.0));
        let q1 = ph.q;
        let mut first_down: Option<(u32, f32)> = None;
        let mut last_ny = 0.0f32;
        for i in 1..=20 {
            let qi = q1.mul(qrot_axis(axis, ang * i as f32 / 20.0));
            let p = ph.make(qi, 0);
            if let PointerOutput::Abs { ny, .. } = ap_os(&mut e, &p, None) {
                if ny > 0.005 && first_down.is_none() {
                    first_down = Some((i, ny));
                }
                last_ny = ny;
            }
        }
        let (i, _) = first_down.expect("el cursor debe bajar del borde");
        // (un par de muestras de cola del filtro al invertir el sentido)
        assert!(i <= 8, "zona muerta: tardó {i} muestras en responder");
        // 5° de bajada = 5/35·16/9 = 0.254 de pantalla desde el borde (±cola del filtro)
        let mut ny_settled = last_ny;
        for _ in 0..30 {
            let q = ph.q;
            let p = ph.make(q, 0);
            if let PointerOutput::Abs { ny, .. } = ap_os(&mut e, &p, None) {
                ny_settled = ny;
            }
        }
        // (menos la cola del filtro que aún subía cuando la mano invirtió,
        // absorbida en el borde: ~0,7° a 600°/s)
        let expected = 5.0 / 35.0 * (16.0 / 9.0);
        assert!((ny_settled - expected).abs() < 0.05, "bajada desde el borde: ny={ny_settled} esperado={expected} (último en movimiento {last_ny})");
    }

    #[test]
    fn el_rebote_de_la_mano_se_ve_tal_cual() {
        // Fidelidad 1:1 también tras un flick: si la mano rebota 4° hacia
        // atrás, el cursor vuelve 4° (nada se esconde ni se frena).
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_x(0.0), 0);
        ap(&mut e, &p);
        ph.hold(&mut e, 200);
        ph.turn(&mut e, qrot_x(30.0), 20); // 600°/s
        ph.turn(&mut e, qrot_x(26.0), 12); // rebote de 4° en 60 ms
        let (_, ny_end) = ph.hold(&mut e, 60);
        let expected = 0.5 - 26.0 / 35.0 * (16.0 / 9.0);
        assert!((ny_end - expected).abs() < 0.03, "el cursor no siguió a la mano: ny={ny_end} esperado={expected}");
    }

    #[test]
    fn recentrar_centra_tambien_en_relativo() {
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        assert_eq!(e.apply(&p, 35.0, 16.0 / 9.0, false, 1920.0), PointerOutput::Abs { nx: 0.5, ny: 0.5 });
        for _ in 0..20 {
            let q = ph.q;
            let p = ph.make(q, 0);
            e.apply(&p, 35.0, 16.0 / 9.0, false, 1920.0);
        }
        let p = ph.make(qrot_z(-10.0), 1); // Home: nuevo recentrado
        assert_eq!(e.apply(&p, 35.0, 16.0 / 9.0, false, 1920.0), PointerOutput::Abs { nx: 0.5, ny: 0.5 });
    }

    #[test]
    fn los_bordes_no_hacen_de_trinquete() {
        // Con un monitor encima, el SO recorta el cursor abajo pero no arriba.
        // Barridos verticales que se pasan por abajo: el apuntado NO debe ir
        // subiendo vuelta tras vuelta (antes el desplazamiento era permanente
        // y el cursor acababa arriba del todo).
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_x(0.0), 0);
        ap(&mut e, &p);
        ph.hold(&mut e, 200);
        // el SO: recorta solo por abajo (ny ≤ 1), arriba deja pasar
        let os = |e: &mut PointerEngine, out: PointerOutput| {
            if let PointerOutput::Abs { nx, ny } = out {
                e.set_cursor_hint(Some((nx.clamp(0.0, 1.0), ny.min(1.0))));
            }
        };
        let mut at_start = Vec::new();
        for _cycle in 0..6 {
            // abajo 14° (se pasa del borde inferior: 14/35·16/9 = 0,71 > 0,5)
            let (axis, ang) = delta_axis_angle(ph.q, qrot_x(-14.0));
            let q0 = ph.q;
            for i in 1..=40 {
                let qi = q0.mul(qrot_axis(axis, ang * i as f32 / 40.0));
                let p = ph.make(qi, 0);
                let out = ap(&mut e, &p);
                os(&mut e, out);
            }
            // y arriba hasta +8° (sin recorte arriba)
            let (axis, ang) = delta_axis_angle(ph.q, qrot_x(8.0));
            let q0 = ph.q;
            for i in 1..=40 {
                let qi = q0.mul(qrot_axis(axis, ang * i as f32 / 40.0));
                let p = ph.make(qi, 0);
                let out = ap(&mut e, &p);
                os(&mut e, out);
            }
            // vuelta al punto de partida
            let (axis, ang) = delta_axis_angle(ph.q, qrot_x(0.0));
            let q0 = ph.q;
            let mut last = None;
            for i in 1..=20 {
                let qi = q0.mul(qrot_axis(axis, ang * i as f32 / 20.0));
                let p = ph.make(qi, 0);
                let out = ap(&mut e, &p);
                os(&mut e, out);
                if let PointerOutput::Abs { ny, .. } = out {
                    last = Some(ny);
                }
            }
            at_start.push(last.unwrap());
        }
        // apuntando al mismo sitio, el cursor debe estar (casi) en el mismo
        // sitio en cada vuelta: nada de subir 0,2 de pantalla por vuelta
        let first = at_start[0];
        let last = *at_start.last().unwrap();
        assert!((last - first).abs() < 0.08, "el apuntado derivó: ny {first:.3} → {last:.3} en 6 vueltas ({at_start:?})");
        assert!(last > 0.3, "el cursor acabó arriba: ny={last:.3} ({at_start:?})");
    }

    #[test]
    fn el_raton_mueve_el_cursor_en_pleno_apuntado() {
        // Mientras el móvil barre en horizontal, el ratón real desplaza el
        // cursor 0,2 de pantalla en vertical: el puntero sigue desde ahí,
        // sin volver atrás y con la ganancia intacta.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap_os(&mut e, &p, None);
        for _ in 0..100 {
            let q = ph.q;
            let p = ph.make(q, 0);
            ap_os(&mut e, &p, None);
        }
        let mut before: Option<(f32, f32)> = None;
        let mut last = (0.5, 0.5);
        for i in 1..=200 {
            let p = ph.make(qrot_z(-0.07 * i as f32), 0); // 14°/s, se queda en pantalla
            let mouse = if i == 100 { before.map(|(x, y)| (x, y + 0.2)) } else { None };
            if let PointerOutput::Abs { nx, ny } = ap_os(&mut e, &p, mouse) {
                if i == 99 {
                    before = Some((nx, ny));
                }
                if i > 105 {
                    assert!((ny - (before.unwrap().1 + 0.2)).abs() < 0.01, "muestra {i}: ny={ny}, debería seguir a 0,2 del ratón");
                }
                last = (nx, ny);
            }
        }
        let (bx, _) = before.unwrap();
        // 101 muestras más a 0,07° = 7,07° → 7,07/35 de pantalla, ganancia 1:1
        let travel = last.0 - bx;
        let expected = 7.07 / 35.0;
        assert!((travel - expected).abs() < 0.02, "ganancia tras el ratón: {travel} esperado {expected}");
    }

    #[test]
    fn sesgo_del_gyro_se_aprende_en_reposo_y_no_altera_la_ganancia() {
        // Gyro con 2°/s de sesgo en yaw. Tras un reposo, un barrido de 20°
        // debe recorrer 20° (no 20° ± lo que sume el sesgo) y el cursor no
        // debe derivar en reposo.
        let mut e = PointerEngine::new();
        let bias = 2.0_f32.to_radians();
        let mut t = 0u64;
        let dt = DT_US as f32 / 1e6;
        let mut prev_true = qrot_z(0.0);
        let mut send = |e: &mut PointerEngine, true_q: Quat, t: u64, prev_true: &mut Quat| {
            let (axis, ang) = delta_axis_angle(*prev_true, true_q);
            *prev_true = true_q;
            let mut p = packet(arr(true_q), 0, t, FLAG_QUAT_VALID);
            let k = ang / dt;
            p.gyro = [axis[0] * k, axis[1] * k, axis[2] * k + bias];
            ap(e, &p)
        };
        let mut moved_at_rest = 0.0f32;
        for _ in 0..600 {
            t += DT_US;
            if let PointerOutput::Abs { nx, ny } = send(&mut e, qrot_z(0.0), t, &mut prev_true) {
                moved_at_rest = moved_at_rest.max((nx - 0.5).hypot(ny - 0.5));
            }
        }
        assert!(moved_at_rest < 0.01, "derivó en reposo con sesgo: {moved_at_rest}");
        let mut last = (0.5, 0.5);
        for i in 1..=100 {
            t += DT_US;
            if let PointerOutput::Abs { nx, ny } = send(&mut e, qrot_z(-0.2 * i as f32), t, &mut prev_true) {
                last = (nx, ny);
            }
        }
        for _ in 0..30 {
            t += DT_US;
            if let PointerOutput::Abs { nx, ny } = send(&mut e, qrot_z(-20.0), t, &mut prev_true) {
                last = (nx, ny);
            }
        }
        let expected = 0.5 + 20.0 / 35.0;
        assert!((last.0 - expected).abs() < 0.025, "recorrido con sesgo: nx={} esperado={expected}", last.0);
    }

    #[test]
    fn el_quat_sanando_en_reposo_no_descongela() {
        // Congelado, el quat corrige 3° de pitch despacio (2°/s) con el gyro
        // a cero: ni se descongela ni se mueve el cursor.
        let mut e = PointerEngine::new();
        let mut t = 0u64;
        for _ in 0..300 {
            t += DT_US;
            ap(&mut e, &packet(arr(qrot_z(0.0)), 0, t, FLAG_QUAT_VALID));
        }
        for i in 1..=300 {
            t += DT_US;
            let pitch = 3.0 * (i as f32 / 300.0);
            let out = ap(&mut e, &packet(arr(qrot_x(pitch)), 0, t, FLAG_QUAT_VALID));
            assert_eq!(out, PointerOutput::None, "muestra {i}: una corrección del quat movió el cursor: {out:?}");
        }
    }

    #[test]
    fn quat_colgado_no_para_el_cursor() {
        // El rotation vector del móvil se queda congelado (fallo del sensor)
        // mientras el gyro sigue midiendo un barrido a 40°/s: el cursor debe
        // seguir al gyro y recorrer todo, no quedarse clavado.
        let mut e = PointerEngine::new();
        let mut t = 0u64;
        let dt = DT_US as f32 / 1e6;
        for _ in 0..200 {
            t += DT_US;
            ap(&mut e, &packet(arr(qrot_z(0.0)), 0, t, FLAG_QUAT_VALID));
        }
        let mut prev_true = qrot_z(0.0);
        let mut last = 0.5f32;
        for i in 1..=100 {
            t += DT_US;
            let true_q = qrot_z(-0.2 * i as f32);
            let (axis, ang) = delta_axis_angle(prev_true, true_q);
            prev_true = true_q;
            let mut p = packet(arr(qrot_z(0.0)), 0, t, FLAG_QUAT_VALID); // quat colgado
            let k = ang / dt;
            p.gyro = [axis[0] * k, axis[1] * k, axis[2] * k];
            if let PointerOutput::Abs { nx, .. } = ap(&mut e, &p) {
                last = nx;
            }
        }
        let expected = 0.5 + 20.0 / 35.0;
        assert!((last - expected).abs() < 0.03, "con el quat colgado el cursor se quedó en {last} (esperado {expected})");
    }

    #[test]
    fn un_barrido_lento_no_va_a_trompicones() {
        // Barrido continuo a 1°/s (el caso medido: 79 congelaciones en 4 min):
        // ni una sola muestra sin emitir después del arranque.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);
        ph.hold(&mut e, 200);
        let mut silent = 0;
        let mut last_nx = 0.5f32;
        for i in 1..=600 {
            let p = ph.make(qrot_z(-0.005 * i as f32), 0); // 1°/s
            match ap(&mut e, &p) {
                PointerOutput::Abs { nx, .. } => {
                    assert!(nx >= last_nx - 1e-4, "muestra {i}: retrocedió {last_nx} → {nx}");
                    last_nx = nx;
                }
                _ => {
                    if i > 120 {
                        silent += 1;
                    }
                }
            }
        }
        assert_eq!(silent, 0, "el barrido lento se congeló {silent} muestras");
        let expected = 0.5 + 3.0 / 35.0;
        assert!((last_nx - expected).abs() < 0.01, "recorrido a 1°/s: {last_nx} esperado {expected}");
    }

    #[test]
    fn congelar_exige_estar_quieto_un_rato_y_al_salir_no_se_pierde_nada() {
        // Quieto → congela (silencio) tras la permanencia; un movimiento lento
        // sostenido descongela y el cursor sale donde la mano está de verdad.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);
        let mut frozen_at = None;
        for i in 0..200 {
            let q = ph.q;
            let p = ph.make(q, 0);
            if ap(&mut e, &p) == PointerOutput::None && frozen_at.is_none() {
                frozen_at = Some(i);
            }
        }
        let f = frozen_at.expect("debe congelar en reposo");
        // quieto de verdad (el quat también lo dice): permanencia corta, ~100 ms
        assert!(f >= 15 && f <= 80, "congeló en la muestra {f} (esperado ~20-60 de permanencia)");
        // 0,4°/s: por debajo del umbral de salida por velocidad, pero
        // sostenido → escape en menos de 0,5 s y sin perder recorrido
        let mut first = None;
        for i in 1..=200 {
            let p = ph.make(qrot_z(-0.002 * i as f32), 0);
            if let PointerOutput::Abs { nx, .. } = ap(&mut e, &p) {
                first = Some((i, nx));
                break;
            }
        }
        let (i, nx) = first.expect("el movimiento lento debe descongelar");
        assert!(i <= 100, "tardó {i} muestras en descongelar");
        let travelled = 0.002 * i as f32;
        let expected = 0.5 + travelled / 35.0;
        assert!((nx - expected).abs() < 0.004, "al salir el cursor debe estar donde la mano: nx={nx} esperado={expected}");
    }

    #[test]
    fn movimiento_lento_si_descongela() {
        // 1°/s de verdad (gyro y quat de acuerdo): por debajo de la
        // histéresis de salida, pero el escape por posición lo libera.
        let mut e = PointerEngine::new();
        let mut ph = Phone::new();
        let p = ph.make(qrot_z(0.0), 0);
        ap(&mut e, &p);
        ph.hold(&mut e, 400);
        let mut moved = None;
        for i in 1..=400 {
            let p = ph.make(qrot_z(-0.005 * i as f32), 0);
            if let PointerOutput::Abs { nx, .. } = ap(&mut e, &p) {
                moved = Some((i, nx));
                break;
            }
        }
        let (i, _) = moved.expect("el movimiento lento debe descongelar");
        assert!(i < 200, "tardó {i} muestras (1 s) en descongelar");
    }
}
