//! One-euro filter (Casiez et al. 2012) — el estándar para punteros:
//! estable parado, sin lag apreciable en movimiento.

use std::f32::consts::TAU;

pub struct OneEuro {
    mincutoff: f32,
    beta: f32,
    dcutoff: f32,
    x_prev: Option<f32>,
    dx_prev: f32,
}

impl OneEuro {
    pub fn new(mincutoff: f32, beta: f32) -> Self {
        Self {
            mincutoff,
            beta,
            dcutoff: 1.0,
            x_prev: None,
            dx_prev: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.x_prev = None;
        self.dx_prev = 0.0;
    }

    fn alpha(cutoff: f32, dt: f32) -> f32 {
        let tau = 1.0 / (TAU * cutoff);
        1.0 / (1.0 + tau / dt)
    }

    pub fn filter(&mut self, x: f32, dt: f32) -> f32 {
        let Some(x_prev) = self.x_prev else {
            self.x_prev = Some(x);
            return x;
        };
        let dx = (x - x_prev) / dt;
        let a_d = Self::alpha(self.dcutoff, dt);
        let dx_hat = a_d * dx + (1.0 - a_d) * self.dx_prev;
        let cutoff = self.mincutoff + self.beta * dx_hat.abs();
        let a = Self::alpha(cutoff, dt);
        let x_hat = a * x + (1.0 - a) * x_prev;
        self.x_prev = Some(x_hat);
        self.dx_prev = dx_hat;
        x_hat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converge_a_senal_constante() {
        let mut f = OneEuro::new(1.0, 0.02);
        let mut y = 0.0;
        for _ in 0..500 {
            y = f.filter(10.0, 0.005);
        }
        assert!((y - 10.0).abs() < 0.01, "y={y}");
    }

    #[test]
    fn atenua_jitter_en_reposo() {
        let mut f = OneEuro::new(1.0, 0.02);
        for _ in 0..200 {
            f.filter(5.0, 0.005);
        }
        // jitter alterno de ±0.1 alrededor de 5: la salida apenas se mueve
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for i in 0..200 {
            let noise = if i % 2 == 0 { 0.1 } else { -0.1 };
            let y = f.filter(5.0 + noise, 0.005);
            min = min.min(y);
            max = max.max(y);
        }
        assert!(max - min < 0.05, "rango={}", max - min);
    }
}
