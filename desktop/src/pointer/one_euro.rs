//! Filtro one-euro 2D isótropo (Casiez et al. 2012, extendido a 2D).
//!
//! CLAVE: el cutoff adaptativo se calcula con la MAGNITUD de la velocidad
//! combinada y se aplica idéntico a ambos ejes. Con filtros independientes
//! por eje, un movimiento diagonal lleva velocidades distintas por eje →
//! cutoffs distintos → lags distintos → la trayectoria se curva. Con cutoff
//! compartido el lag es isótropo y las diagonales salen rectas.

use std::f32::consts::TAU;

struct LowPass {
    y: Option<f32>,
}

impl LowPass {
    fn new() -> Self {
        Self { y: None }
    }

    fn filter(&mut self, x: f32, alpha: f32) -> f32 {
        let y = match self.y {
            Some(prev) => alpha * x + (1.0 - alpha) * prev,
            None => x,
        };
        self.y = Some(y);
        y
    }
}

fn alpha(cutoff: f32, dt: f32) -> f32 {
    let tau = 1.0 / (TAU * cutoff);
    1.0 / (1.0 + tau / dt)
}

pub struct Filter2D {
    mincutoff: f32,
    beta: f32,
    dcutoff: f32,
    lp_x: LowPass,
    lp_y: LowPass,
    lp_dx: LowPass,
    lp_dy: LowPass,
    /// Velocidad externa apenas suavizada (20 Hz): abre el cutoff al
    /// instante en cuanto la mano arranca, sin el retardo de `dcutoff`.
    lp_fx: LowPass,
    lp_fy: LowPass,
    prev: Option<(f32, f32)>,
}

/// Suavizado de la velocidad que abre el filtro en `filter_with_rate` (Hz).
const RATE_CUTOFF_FAST: f32 = 20.0;

impl Filter2D {
    pub fn new(mincutoff: f32, beta: f32) -> Self {
        Self {
            mincutoff,
            beta,
            // 2 Hz (vs el 1 Hz canónico): el estimador de velocidad responde
            // al arranque de un flick en ~80 ms en vez de ~160 — menos
            // infra-recorrido — sin dejar pasar el jitter de reposo
            dcutoff: 2.0,
            lp_x: LowPass::new(),
            lp_y: LowPass::new(),
            lp_dx: LowPass::new(),
            lp_dy: LowPass::new(),
            lp_fx: LowPass::new(),
            lp_fy: LowPass::new(),
            prev: None,
        }
    }

    pub fn reset(&mut self) {
        self.lp_x = LowPass::new();
        self.lp_y = LowPass::new();
        self.lp_dx = LowPass::new();
        self.lp_dy = LowPass::new();
        self.lp_fx = LowPass::new();
        self.lp_fy = LowPass::new();
        self.prev = None;
    }

    /// Como `filter`, pero la velocidad viene de fuera (el gyro: instantánea,
    /// sin el retardo de derivar), en las mismas unidades por segundo. El
    /// cutoff se abre con esa velocidad apenas suavizada (20 Hz), para que
    /// el arranque de un gesto no se quede pegajoso; la velocidad devuelta
    /// (para decidir la congelación) lleva el suavizado tranquilo de
    /// `dcutoff`.
    pub fn filter_with_rate(&mut self, x: f32, y: f32, rate_x: f32, rate_y: f32, dt: f32) -> (f32, f32, f32) {
        self.prev = Some((x, y));
        let a_d = alpha(self.dcutoff, dt);
        let dx = self.lp_dx.filter(rate_x, a_d);
        let dy = self.lp_dy.filter(rate_y, a_d);
        let speed = (dx * dx + dy * dy).sqrt();
        let a_f = alpha(RATE_CUTOFF_FAST, dt);
        let fx = self.lp_fx.filter(rate_x, a_f);
        let fy = self.lp_fy.filter(rate_y, a_f);
        let fast = (fx * fx + fy * fy).sqrt();
        let a = alpha(self.mincutoff + self.beta * fast, dt);
        (self.lp_x.filter(x, a), self.lp_y.filter(y, a), speed)
    }

    /// Devuelve (x, y) filtrados y la velocidad combinada suavizada
    /// (mismas unidades de entrada por segundo), derivando la entrada.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn filter(&mut self, x: f32, y: f32, dt: f32) -> (f32, f32, f32) {
        let (px, py) = self.prev.unwrap_or((x, y));
        self.prev = Some((x, y));

        let a_d = alpha(self.dcutoff, dt);
        let dx = self.lp_dx.filter((x - px) / dt, a_d);
        let dy = self.lp_dy.filter((y - py) / dt, a_d);
        let speed = (dx * dx + dy * dy).sqrt();

        // Un solo cutoff para los dos ejes: lag isótropo
        let a = alpha(self.mincutoff + self.beta * speed, dt);
        (self.lp_x.filter(x, a), self.lp_y.filter(y, a), speed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converge_a_senal_constante() {
        let mut f = Filter2D::new(1.0, 0.08);
        let mut out = (0.0, 0.0, 0.0);
        for _ in 0..500 {
            out = f.filter(10.0, -4.0, 0.005);
        }
        assert!((out.0 - 10.0).abs() < 0.01 && (out.1 + 4.0).abs() < 0.01);
        assert!(out.2 < 0.1, "en reposo la velocidad debe caer, es {}", out.2);
    }

    #[test]
    fn diagonal_isotropa_rampas_iguales_salen_iguales() {
        // Rampa idéntica en ambos ejes: con cutoff compartido, la salida de
        // ambos ejes debe ser EXACTAMENTE igual en cada paso (misma alpha).
        let mut f = Filter2D::new(1.0, 0.08);
        for i in 0..300 {
            let v = i as f32 * 0.05;
            let (ox, oy, _) = f.filter(v, v, 0.005);
            assert!(
                (ox - oy).abs() < 1e-6,
                "paso {i}: ox={ox} oy={oy} — el lag no es isótropo"
            );
        }
    }

    #[test]
    fn la_velocidad_externa_abre_el_filtro_igual_que_la_derivada() {
        // Misma rampa: con la velocidad dada desde fuera (constante) el
        // filtro converge a la misma salida que derivando, y con velocidad
        // externa cero sigue a la senal con el cutoff minimo (mas lag).
        let mut a = Filter2D::new(1.0, 0.2);
        let mut b = Filter2D::new(1.0, 0.2);
        let mut c = Filter2D::new(1.0, 0.2);
        let (mut oa, mut ob, mut oc) = ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0), (0.0, 0.0, 0.0));
        for i in 0..400 {
            let v = i as f32 * 0.25; // 50 unidades/s
            oa = a.filter(v, 0.0, 0.005);
            ob = b.filter_with_rate(v, 0.0, 50.0, 0.0, 0.005);
            oc = c.filter_with_rate(v, 0.0, 0.0, 0.0, 0.005);
        }
        assert!((oa.0 - ob.0).abs() < 0.05, "derivada {} vs externa {}", oa.0, ob.0);
        assert!((oa.2 - 50.0).abs() < 1.0 && (ob.2 - 50.0).abs() < 0.01);
        assert!(oc.0 < ob.0 - 1.0, "sin velocidad externa el lag debe ser mayor: {} vs {}", oc.0, ob.0);
    }

    #[test]
    fn atenua_jitter_en_reposo() {
        let mut f = Filter2D::new(1.0, 0.08);
        for _ in 0..200 {
            f.filter(5.0, 5.0, 0.005);
        }
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for i in 0..200 {
            let noise = if i % 2 == 0 { 0.1 } else { -0.1 };
            let (ox, _, _) = f.filter(5.0 + noise, 5.0 - noise, 0.005);
            min = min.min(ox);
            max = max.max(ox);
        }
        assert!(max - min < 0.06, "rango={}", max - min);
    }
}
