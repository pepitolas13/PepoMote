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
    last_t_us: Option<u64>,
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
            // flicks); la estabilidad en reposo la da la congelación
            filter: Filter2D::new(1.0, 0.08),
            frozen: false,
            last_emitted: None,
            offset: (0.0, 0.0),
            first_t_us: None,
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
            // Fallback h1: integración relativa del gyro (ejes del dispositivo)
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
        let (yaw_w, pitch_w) = q.world_angles();

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
            if yaw.abs() > lim || pitch.abs() * aspect_w_over_h > lim {
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
                // Liberar SIN salto: el offset absorbe la diferencia exacta y
                // el movimiento continúa desde el punto congelado.
                self.frozen = false;
                self.offset = (prev_yaw - yaw_f, prev_pitch - pitch_f);
            } else {
                return self.emit(prev_yaw, prev_pitch, sens_deg, aspect_w_over_h, abs_mode, screen_w_px, prev_yaw, prev_pitch);
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

    fn run_to(e: &mut PointerEngine, quat: [f32; 4], rec: u8, n: u64, t0: u64) -> (f32, f32) {
        let mut out = (0.5, 0.5);
        for i in 0..n {
            if let PointerOutput::Abs { nx, ny } = e.apply(
                &packet(quat, rec, t0 + i * 5_000, FLAG_QUAT_VALID),
                35.0,
                16.0 / 9.0,
                true,
                1920.0,
            ) {
                out = (nx, ny);
            }
        }
        out
    }

    #[test]
    fn recentrado_centra_y_yaw_derecha_mueve_derecha() {
        let mut e = PointerEngine::new();
        let out = e.apply(
            &packet(arr(qrot_z(0.0)), 0, 0, FLAG_QUAT_VALID),
            35.0,
            16.0 / 9.0,
            true,
            1920.0,
        );
        assert_eq!(out, PointerOutput::Abs { nx: 0.5, ny: 0.5 });

        let (nx, _) = run_to(&mut e, arr(qrot_z(-10.0)), 0, 200, 5_000);
        let expected = 0.5 + 10.0 / 35.0;
        assert!((nx - expected).abs() < 0.01, "nx={nx} esperado={expected}");
    }

    #[test]
    fn pitch_arriba_sube_el_cursor() {
        let mut e = PointerEngine::new();
        e.apply(&packet(arr(qrot_x(0.0)), 0, 0, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        let (_, ny) = run_to(&mut e, arr(qrot_x(5.0)), 0, 200, 5_000);
        let expected = 0.5 - (5.0 / 35.0) * (16.0 / 9.0);
        assert!((ny - expected).abs() < 0.01, "ny={ny} esperado={expected}");
    }

    #[test]
    fn el_roll_no_afecta_al_cursor() {
        // Mismo apuntado (yaw -10°) con y sin 70° de roll sobre el eje de
        // apuntado: el cursor debe acabar EXACTAMENTE en el mismo sitio.
        let mut e1 = PointerEngine::new();
        e1.apply(&packet(arr(qrot_z(0.0)), 0, 0, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        let (nx1, ny1) = run_to(&mut e1, arr(qrot_z(-10.0)), 0, 200, 5_000);

        let mut e2 = PointerEngine::new();
        // Recentra YA rolado 40°...
        e2.apply(
            &packet(arr(qrot_y(40.0)), 0, 0, FLAG_QUAT_VALID),
            35.0,
            16.0 / 9.0,
            true,
            1920.0,
        );
        // ...y apunta con 70° de roll: q = giro mundial ∘ roll local (el roll
        // sobre device-Y no cambia el eje de apuntado)
        let q = qrot_z(-10.0).mul(qrot_y(70.0));
        let (nx2, ny2) = run_to(&mut e2, arr(q), 0, 200, 5_000);

        assert!(
            (nx1 - nx2).abs() < 1e-3 && (ny1 - ny2).abs() < 1e-3,
            "sin roll ({nx1},{ny1}) vs con roll ({nx2},{ny2})"
        );
    }

    #[test]
    fn recentrar_rolado_mantiene_los_ejes_del_mundo() {
        // Recentrado con el móvil rolado 90°: mover la muñeca en horizontal
        // (yaw del mundo) debe seguir moviendo el cursor SOLO en horizontal.
        let mut e = PointerEngine::new();
        e.apply(
            &packet(arr(qrot_y(90.0)), 0, 0, FLAG_QUAT_VALID),
            35.0,
            16.0 / 9.0,
            true,
            1920.0,
        );
        let q = qrot_z(-10.0).mul(qrot_y(90.0));
        let (nx, ny) = run_to(&mut e, arr(q), 0, 200, 5_000);
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
        // el sensor se asienta: orientación real pitch -40°
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
        let q = qrot_x(-40.0).mul(qrot_y(0.0)); // base
        let _ = q;
        let (nx, _) = run_to(&mut e, arr(qrot_z(-5.0).mul(qrot_x(-40.0))), 0, 300, t + 5_000);
        let expected = 0.5 + 5.0 / 35.0;
        assert!((nx - expected).abs() < 0.02, "nx={nx} esperado={expected}");
    }

    #[test]
    fn descongelar_sin_salto() {
        // Parar (congela) y reanudar despacio: el cursor debe fluir SIN
        // teletransporte. Antes del puente, al superar el escape de 0.35°
        // saltaba ~0.01 en nx de un sample al siguiente.
        let mut e = PointerEngine::new();
        e.apply(&packet(arr(qrot_z(0.0)), 0, 0, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        // ir a -5° y quedarse quieto hasta congelar
        let mut t = 0u64;
        for _ in 0..400 {
            t += 5_000;
            e.apply(&packet(arr(qrot_z(-5.0)), 0, t, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        }
        // rampa lenta a 3°/s: recoger salidas y medir el salto máximo
        let mut last_nx: Option<f32> = None;
        let mut max_jump = 0.0f32;
        for i in 0..300 {
            t += 5_000;
            let deg = -5.0 - 3.0 * (i as f32 * 0.005);
            if let PointerOutput::Abs { nx, .. } =
                e.apply(&packet(arr(qrot_z(deg)), 0, t, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0)
            {
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
        e.apply(&packet(arr(qrot_z(0.0)), 0, 0, FLAG_QUAT_VALID), 35.0, 16.0 / 9.0, true, 1920.0);
        run_to(&mut e, arr(qrot_z(-20.0)), 0, 50, 5_000);
        let out = e.apply(
            &packet(arr(qrot_z(-20.0)), 1, 300_000, FLAG_QUAT_VALID),
            35.0,
            16.0 / 9.0,
            true,
            1920.0,
        );
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

    #[test]
    fn wrap180_funciona() {
        assert!((wrap180(190.0) + 170.0).abs() < 1e-4);
        assert!((wrap180(-190.0) - 170.0).abs() < 1e-4);
        assert!((wrap180(10.0) - 10.0).abs() < 1e-4);
    }
}
