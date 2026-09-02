//! Bias del giroscopio estimado en reposo. Ni los IMU por IIO ni el SSC de
//! Qualcomm entregan el gyro calibrado (Android lo hace en su HAL: por eso
//! allí el puntero no deriva y aquí sí). Un gyro MEMS en reposo marca unas
//! décimas de grado por segundo que, integradas, mueven el puntero solo y
//! siempre hacia el mismo lado. Aquí: cuando el móvil está quieto medio
//! segundo (gyro y |accel| sin apenas varianza) la media del gyro ES el bias;
//! se adopta de golpe la primera vez y luego se refina despacio.

pub struct GyroBias {
    bias: [f32; 3],
    settled: bool,
    win_start: Option<u64>,
    n: u32,
    sum_g: [f64; 3],
    sum_g2: [f64; 3],
    sum_a: f64,
    sum_a2: f64,
}

/// Ventana de evaluación.
const WINDOW_US: u64 = 500_000;
/// Desviación típica máxima del gyro (rad/s) para considerar reposo (~1,7°/s).
const MAX_STD_GYRO: f64 = 0.03;
/// Desviación típica máxima de |accel| (m/s²) para considerar reposo.
const MAX_STD_ACC: f64 = 0.35;
/// Un "bias" mayor que esto (~7°/s) es movimiento, no bias.
const MAX_BIAS: f64 = 0.12;
/// Refinado tras la primera adopción (por ventana de 0,5 s: constante ~3 s).
const ALPHA_SETTLED: f32 = 0.15;

impl Default for GyroBias {
    fn default() -> Self {
        Self::new()
    }
}

impl GyroBias {
    pub fn new() -> Self {
        Self {
            bias: [0.0; 3],
            settled: false,
            win_start: None,
            n: 0,
            sum_g: [0.0; 3],
            sum_g2: [0.0; 3],
            sum_a: 0.0,
            sum_a2: 0.0,
        }
    }

    pub fn bias(&self) -> [f32; 3] {
        self.bias
    }

    pub fn settled(&self) -> bool {
        self.settled
    }

    /// Alimenta una muestra cruda y devuelve el gyro corregido.
    pub fn correct(&mut self, t_us: u64, gyro: [f32; 3], accel: [f32; 3]) -> [f32; 3] {
        self.accumulate(t_us, gyro, accel);
        [gyro[0] - self.bias[0], gyro[1] - self.bias[1], gyro[2] - self.bias[2]]
    }

    fn accumulate(&mut self, t_us: u64, gyro: [f32; 3], accel: [f32; 3]) {
        let start = *self.win_start.get_or_insert(t_us);
        if t_us < start {
            self.reset_window();
            return;
        }
        self.n += 1;
        for i in 0..3 {
            let g = gyro[i] as f64;
            self.sum_g[i] += g;
            self.sum_g2[i] += g * g;
        }
        let a = ((accel[0] * accel[0] + accel[1] * accel[1] + accel[2] * accel[2]) as f64).sqrt();
        self.sum_a += a;
        self.sum_a2 += a * a;
        if t_us - start >= WINDOW_US {
            self.evaluate();
            self.reset_window();
        }
    }

    fn reset_window(&mut self) {
        self.win_start = None;
        self.n = 0;
        self.sum_g = [0.0; 3];
        self.sum_g2 = [0.0; 3];
        self.sum_a = 0.0;
        self.sum_a2 = 0.0;
    }

    fn evaluate(&mut self) {
        if self.n < 10 {
            return;
        }
        let k = self.n as f64;
        let mut mean = [0f32; 3];
        let mut still = true;
        for i in 0..3 {
            let m = self.sum_g[i] / k;
            let var = (self.sum_g2[i] / k - m * m).max(0.0);
            if var.sqrt() > MAX_STD_GYRO || m.abs() > MAX_BIAS {
                still = false;
            }
            mean[i] = m as f32;
        }
        let ma = self.sum_a / k;
        let va = (self.sum_a2 / k - ma * ma).max(0.0);
        if va.sqrt() > MAX_STD_ACC {
            still = false;
        }
        if !still {
            return;
        }
        let alpha = if self.settled { ALPHA_SETTLED } else { 1.0 };
        for i in 0..3 {
            self.bias[i] += alpha * (mean[i] - self.bias[i]);
        }
        self.settled = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ruido determinista en [-1, 1).
    struct Noise(u64);
    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
        }
    }

    fn feed(b: &mut GyroBias, secs: f32, gyro: [f32; 3], noise_g: f32, accel_noise: f32, seed: u64) -> [f32; 3] {
        let mut nz = Noise(seed);
        let n = (secs * 200.0) as u64;
        let mut last = [0.0; 3];
        for i in 0..n {
            let g = [
                gyro[0] + nz.next() * noise_g,
                gyro[1] + nz.next() * noise_g,
                gyro[2] + nz.next() * noise_g,
            ];
            let a = [nz.next() * accel_noise, nz.next() * accel_noise, 9.81 + nz.next() * accel_noise];
            last = b.correct(i * 5000, g, a);
        }
        last
    }

    #[test]
    fn adopta_el_bias_en_reposo_y_lo_resta() {
        let mut b = GyroBias::new();
        let out = feed(&mut b, 1.5, [0.02, -0.01, 0.035], 0.005, 0.05, 1);
        assert!(b.settled());
        let bias = b.bias();
        assert!((bias[0] - 0.02).abs() < 0.003, "{bias:?}");
        assert!((bias[1] + 0.01).abs() < 0.003, "{bias:?}");
        assert!((bias[2] - 0.035).abs() < 0.003, "{bias:?}");
        assert!(out[2].abs() < 0.01, "{out:?}");
    }

    #[test]
    fn en_movimiento_no_toca_el_bias() {
        let mut b = GyroBias::new();
        feed(&mut b, 2.0, [1.0, 0.0, 0.0], 0.005, 0.05, 2); // girando a 57°/s
        assert!(!b.settled());
        assert_eq!(b.bias(), [0.0; 3]);
        // gyro casi quieto pero el móvil vibrando (accel inestable): tampoco
        feed(&mut b, 2.0, [0.02, 0.0, 0.0], 0.005, 1.5, 3);
        assert!(!b.settled());
    }

    #[test]
    fn un_giro_lento_grande_no_es_bias() {
        let mut b = GyroBias::new();
        feed(&mut b, 2.0, [0.0, 0.0, 0.3], 0.001, 0.02, 4); // 17°/s muy suave
        assert_eq!(b.bias(), [0.0; 3]);
    }

    #[test]
    fn tras_asentarse_refina_despacio() {
        let mut b = GyroBias::new();
        feed(&mut b, 1.0, [0.02, 0.0, 0.0], 0.002, 0.02, 5);
        assert!((b.bias()[0] - 0.02).abs() < 0.002);
        // el bias cambia (temperatura): converge, pero no de golpe
        feed(&mut b, 0.6, [0.04, 0.0, 0.0], 0.002, 0.02, 6);
        let after_one = b.bias()[0];
        assert!(after_one > 0.021 && after_one < 0.03, "{after_one}");
        feed(&mut b, 6.0, [0.04, 0.0, 0.0], 0.002, 0.02, 7);
        assert!((b.bias()[0] - 0.04).abs() < 0.003, "{}", b.bias()[0]);
    }
}
