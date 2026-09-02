//! Salida a ritmo fijo, interpolada. Algunas fuentes entregan las muestras a
//! ráfagas (el SSC de Qualcomm agrupa varias; un IIO en modo buffer también
//! puede): la fusión va bien porque usa los timestamps del sensor, pero si
//! cada ráfaga se manda tal cual, el puntero (orientación ABSOLUTA en el
//! receptor) salta a cada ráfaga. Aquí se guarda un histórico de estados
//! (t_sensor, quaternion, gyro, accel), se mide cuánto "envejece" cada
//! muestra hasta llegar y se reproduce el histórico con un retardo igual al
//! peor envejecimiento reciente, interpolando (slerp) entre estados según el
//! reloj de pared. Con entrega regular el retardo es ~0 y no cambia nada.

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub t_us: u64,
    pub quat: [f32; 4],
    pub gyro: [f32; 3],
    pub accel: [f32; 3],
}

pub struct Pacer {
    hist: VecDeque<State>,
    /// (reloj de pared µs, edad = pared − t_sensor) de las últimas llegadas
    ages: VecDeque<(u64, i64)>,
    offset_min: i64,
    delay_us: u64,
    pushes: u32,
}

/// Histórico y ventana de edades que se conservan.
const KEEP_US: u64 = 1_500_000;
/// Retardo máximo que se acepta (más allá, mejor un salto que tanta latencia).
const MAX_DELAY_US: u64 = 80_000;
/// Margen sobre el envejecimiento medido.
const MARGIN_US: u64 = 5_000;

impl Default for Pacer {
    fn default() -> Self {
        Self::new()
    }
}

impl Pacer {
    pub fn new() -> Self {
        Self {
            hist: VecDeque::new(),
            ages: VecDeque::new(),
            offset_min: 0,
            delay_us: 0,
            pushes: 0,
        }
    }

    pub fn delay_us(&self) -> u64 {
        self.delay_us
    }

    /// Nuevo estado fusionado, llegado en el instante de pared `wall_us`.
    pub fn push(&mut self, s: State, wall_us: u64) {
        if self.hist.back().is_some_and(|b| s.t_us <= b.t_us) {
            return; // reloj del sensor hacia atrás o repetido: fuera
        }
        self.hist.push_back(s);
        while self.hist.front().is_some_and(|f| s.t_us.saturating_sub(f.t_us) > KEEP_US) {
            self.hist.pop_front();
        }
        self.ages.push_back((wall_us, wall_us as i64 - s.t_us as i64));
        while self.ages.front().is_some_and(|f| wall_us.saturating_sub(f.0) > KEEP_US) {
            self.ages.pop_front();
        }
        self.pushes = self.pushes.wrapping_add(1);
        if self.pushes % 8 == 0 || self.ages.len() < 8 {
            self.recalc();
        }
    }

    fn recalc(&mut self) {
        if self.ages.is_empty() {
            return;
        }
        let mut v: Vec<i64> = self.ages.iter().map(|a| a.1).collect();
        v.sort_unstable();
        self.offset_min = v[0];
        let p95 = v[(v.len() - 1) * 95 / 100];
        let spread = (p95 - self.offset_min).max(0) as u64;
        self.delay_us = (spread + MARGIN_US).min(MAX_DELAY_US);
    }

    /// Estado interpolado para el instante de pared `wall_us` (reproducido
    /// con el retardo actual). None sin histórico.
    pub fn output(&self, wall_us: u64) -> Option<State> {
        let last = *self.hist.back()?;
        let target = (wall_us as i64 - self.offset_min - self.delay_us as i64).max(0) as u64;
        if target >= last.t_us {
            return Some(last);
        }
        let first = *self.hist.front()?;
        if target <= first.t_us {
            return Some(first);
        }
        // par (a, b) con a.t <= target <= b.t, buscando desde el final
        let mut b = last;
        for a in self.hist.iter().rev().skip(1) {
            if a.t_us <= target {
                let f = (target - a.t_us) as f32 / (b.t_us - a.t_us).max(1) as f32;
                return Some(State {
                    t_us: target,
                    quat: slerp(a.quat, b.quat, f),
                    gyro: lerp3(a.gyro, b.gyro, f),
                    accel: lerp3(a.accel, b.accel, f),
                });
            }
            b = *a;
        }
        Some(first)
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], f: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]
}

/// Interpolación esférica entre quaternions (w, x, y, z), camino corto.
pub fn slerp(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if dot < 0.0 {
        for v in &mut b {
            *v = -*v;
        }
        dot = -dot;
    }
    let (s0, s1) = if dot > 0.9995 {
        (1.0 - t, t)
    } else {
        let theta0 = dot.clamp(-1.0, 1.0).acos();
        let theta = theta0 * t;
        let sin0 = theta0.sin();
        (theta.cos() - dot * theta.sin() / sin0, theta.sin() / sin0)
    };
    let mut q = [
        a[0] * s0 + b[0] * s1,
        a[1] * s0 + b[1] * s1,
        a[2] * s0 + b[2] * s1,
        a[3] * s0 + b[3] * s1,
    ];
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n > 1e-9 {
        for v in &mut q {
            *v /= n;
        }
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quaternion de un giro `deg` sobre Z.
    fn qz(deg: f32) -> [f32; 4] {
        let h = deg.to_radians() / 2.0;
        [h.cos(), 0.0, 0.0, h.sin()]
    }

    fn yaw_deg(q: [f32; 4]) -> f32 {
        (2.0 * q[3].atan2(q[0])).to_degrees()
    }

    fn st(t_us: u64, yaw: f32) -> State {
        State {
            t_us,
            quat: qz(yaw),
            gyro: [0.0, 0.0, yaw],
            accel: [0.0, 0.0, 9.81],
        }
    }

    #[test]
    fn slerp_a_medio_camino() {
        let q = slerp(qz(0.0), qz(90.0), 0.5);
        assert!((yaw_deg(q) - 45.0).abs() < 0.01);
        // camino corto aunque b venga con el signo opuesto
        let mut b = qz(90.0);
        for v in &mut b {
            *v = -*v;
        }
        let q = slerp(qz(0.0), b, 0.5);
        assert!((yaw_deg(q) - 45.0).abs() < 0.01);
        // casi iguales: nlerp sin NaN
        let q = slerp(qz(0.0), qz(0.001), 0.3);
        assert!(q.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn entrega_regular_sin_retardo_apreciable() {
        let mut p = Pacer::new();
        // 200 Hz, cada muestra llega 1 ms después de generarse
        for i in 0..400u64 {
            let t = i * 5000;
            p.push(st(t, i as f32 * 0.1), t + 1000);
        }
        assert!(p.delay_us() <= 3000, "{}", p.delay_us());
        let out = p.output(399 * 5000 + 1500).unwrap();
        assert!((yaw_deg(out.quat) - 39.9).abs() < 0.2, "{}", yaw_deg(out.quat));
    }

    #[test]
    fn rafagas_salen_suaves_con_retardo_de_una_rafaga() {
        let mut p = Pacer::new();
        // 10 muestras (50 ms de sensor) llegan de golpe cada 50 ms, girando a 0,1°/muestra
        let mut i = 0u64;
        let mut last_wall = 0;
        for burst in 0..40u64 {
            let wall = (burst + 1) * 50_000;
            for _ in 0..10 {
                p.push(st(i * 5000, i as f32 * 0.1), wall);
                i += 1;
            }
            last_wall = wall;
        }
        let d = p.delay_us();
        assert!((45_000..=60_000).contains(&d), "retardo {d}");
        // Entre dos ráfagas la salida avanza de forma continua según el reloj de pared
        let mut prev = yaw_deg(p.output(last_wall).unwrap().quat);
        for k in 1..=10u64 {
            let y = yaw_deg(p.output(last_wall + k * 5000).unwrap().quat);
            let step = y - prev;
            assert!((0.05..0.15).contains(&step), "paso {step} en k={k}");
            prev = y;
        }
    }

    #[test]
    fn sin_historico_o_fuera_de_rango() {
        let mut p = Pacer::new();
        assert!(p.output(0).is_none());
        p.push(st(1000, 5.0), 1000);
        assert_eq!(p.output(0).unwrap(), st(1000, 5.0));
        assert_eq!(p.output(10_000_000).unwrap(), st(1000, 5.0));
        // muestra repetida o hacia atrás: se ignora
        p.push(st(1000, 9.0), 2000);
        p.push(st(500, 9.0), 3000);
        assert_eq!(p.hist.len(), 1);
    }
}
