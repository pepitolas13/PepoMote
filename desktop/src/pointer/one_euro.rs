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
    prev: Option<(f32, f32)>,
}

impl Filter2D {
    pub fn new(mincutoff: f32, beta: f32) -> Self {
        Self {
            mincutoff,
            beta,
            dcutoff: 1.0,
            lp_x: LowPass::new(),
            lp_y: LowPass::new(),
            lp_dx: LowPass::new(),
            lp_dy: LowPass::new(),
            prev: None,
        }
    }

    pub fn reset(&mut self) {
        self.lp_x = LowPass::new();
        self.lp_y = LowPass::new();
        self.lp_dx = LowPass::new();
        self.lp_dy = LowPass::new();
        self.prev = None;
    }

    /// Devuelve (x, y) filtrados y la velocidad combinada suavizada
    /// (mismas unidades de entrada por segundo).
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
