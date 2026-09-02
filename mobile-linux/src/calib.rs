//! Calibración de ejes: signo de cada eje del acelerómetro y del giroscopio
//! respecto al marco que esperan la fusión y el receptor (el de Android:
//! x derecha, y hacia arriba/adelante, z saliendo de la pantalla; plano boca
//! arriba => accel z ≈ +9,8). En los móviles Linux el IMU llega por caminos
//! que a veces traen algún eje invertido (mount_matrix dudosa en IIO, `.orient`
//! del registro Android en el SSC de Qualcomm…) y entonces el puntero va al
//! revés en vertical o la fusión pelea gyro contra accel (tirones). Seis
//! posturas/gestos guiados bastan para deducir los seis signos; solo signos,
//! porque en un móvil el marco es siempre el de retrato (no hay que permutar).

use crate::sensor::Sample;
use serde::{Deserialize, Serialize};

pub const AXIS: [&str; 3] = ["x", "y", "z"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Axes {
    pub accel: [i8; 3],
    pub gyro: [i8; 3],
}

impl Default for Axes {
    fn default() -> Self {
        Self {
            accel: [1; 3],
            gyro: [1; 3],
        }
    }
}

impl Axes {
    pub fn is_identity(&self) -> bool {
        self.accel == [1; 3] && self.gyro == [1; 3]
    }

    pub fn apply(&self, s: Sample) -> Sample {
        let mut o = s;
        for i in 0..3 {
            o.accel[i] = s.accel[i] * self.accel[i] as f32;
            o.gyro[i] = s.gyro[i] * self.gyro[i] as f32;
        }
        o
    }

    /// "accel +-+ gyro +++"
    pub fn describe(&self) -> String {
        let f = |v: [i8; 3]| v.iter().map(|s| if *s < 0 { '-' } else { '+' }).collect::<String>();
        format!("accel {} gyro {}", f(self.accel), f(self.gyro))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Postura quieta: la gravedad debe caer en `axis` con signo +.
    Pose,
    /// Gesto: el giro acumulado debe caer en `axis` con signo +.
    Motion,
}

pub struct Step {
    pub title: &'static str,
    pub text: &'static str,
    pub kind: Kind,
    pub axis: usize,
}

/// Los seis pasos. Signos esperados (marco Android, regla de la mano derecha):
/// plano boca arriba => +z; de pie => +y; sobre el lado izquierdo => +x;
/// levantar el borde superior => giro +x; levantar el borde izquierdo => +y;
/// girar antihorario visto desde arriba => +z.
pub const STEPS: [Step; 6] = [
    Step {
        title: "1 · Plano boca arriba",
        text: "Deja el móvil plano sobre la mesa, con la pantalla hacia arriba, y pulsa Listo.",
        kind: Kind::Pose,
        axis: 2,
    },
    Step {
        title: "2 · De pie",
        text: "Ponlo de pie, apoyado en el borde de abajo (el del USB), con la pantalla hacia ti, como si lo estuvieras leyendo. Sujétalo quieto y pulsa Listo.",
        kind: Kind::Pose,
        axis: 1,
    },
    Step {
        title: "3 · Sobre el lado izquierdo",
        text: "Túmbalo de canto sobre su lado IZQUIERDO, con la pantalla hacia ti: queda apaisado, con la cámara hacia tu izquierda. Quieto, y pulsa Listo.",
        kind: Kind::Pose,
        axis: 0,
    },
    Step {
        title: "4 · Levanta el borde superior",
        text: "Déjalo plano. Al pulsar Listo tienes 2 segundos: levanta el borde de ARRIBA (el de la cámara) unos 45° y vuelve a bajarlo, como si el móvil asintiera.",
        kind: Kind::Motion,
        axis: 0,
    },
    Step {
        title: "5 · Levanta el borde izquierdo",
        text: "Plano otra vez. Al pulsar Listo tienes 2 segundos: levanta el lado IZQUIERDO unos 45° (se ladea hacia la derecha) y bájalo.",
        kind: Kind::Motion,
        axis: 1,
    },
    Step {
        title: "6 · Gira como un volante",
        text: "Plano sobre la mesa. Al pulsar Listo tienes 2 segundos: sin levantarlo, gíralo un cuarto de vuelta hacia la IZQUIERDA, como un volante.",
        kind: Kind::Motion,
        axis: 2,
    },
];

pub fn mean_accel(samples: &[Sample]) -> [f32; 3] {
    if samples.is_empty() {
        return [0.0; 3];
    }
    let mut m = [0f64; 3];
    for s in samples {
        for (acc, v) in m.iter_mut().zip(s.accel) {
            *acc += v as f64;
        }
    }
    let k = samples.len() as f64;
    [(m[0] / k) as f32, (m[1] / k) as f32, (m[2] / k) as f32]
}

/// Giro acumulado en grados por eje, integrando el gyro (rad/s) con el dt de
/// sus timestamps (µs). Huecos absurdos (> 250 ms) se ignoran.
pub fn integrate_deg(samples: &[Sample]) -> [f32; 3] {
    let mut acc = [0f64; 3];
    for w in samples.windows(2) {
        let dt = w[1].t_us.saturating_sub(w[0].t_us) as f64 / 1e6;
        if !(1e-5..0.25).contains(&dt) {
            continue;
        }
        for (a, g) in acc.iter_mut().zip(w[1].gyro) {
            *a += g as f64 * dt;
        }
    }
    let d = 180.0 / std::f64::consts::PI;
    [(acc[0] * d) as f32, (acc[1] * d) as f32, (acc[2] * d) as f32]
}

/// Signo del eje `axis` en una postura: la gravedad (≥ 6 m/s²) debe caer en
/// ese eje y dominar claramente (≥ 2× los otros).
pub fn pose_sign(mean: [f32; 3], axis: usize) -> Result<i8, String> {
    let v = mean[axis].abs();
    let others = (0..3).filter(|&i| i != axis).map(|i| mean[i].abs()).fold(0f32, f32::max);
    if v < 6.0 || v < 2.0 * others {
        return Err(format!(
            "No veo la gravedad en el eje {} (accel [{:.1} {:.1} {:.1}]). Repite la postura, quieto.",
            AXIS[axis], mean[0], mean[1], mean[2]
        ));
    }
    Ok(if mean[axis] > 0.0 { 1 } else { -1 })
}

/// Signo del eje `axis` en un gesto: el giro acumulado (≥ 20°) debe caer en
/// ese eje y dominar (≥ 1,5× los otros).
pub fn motion_sign(deg: [f32; 3], axis: usize) -> Result<i8, String> {
    let v = deg[axis].abs();
    let others = (0..3).filter(|&i| i != axis).map(|i| deg[i].abs()).fold(0f32, f32::max);
    if v < 20.0 || v < 1.5 * others {
        return Err(format!(
            "No veo un giro claro en el eje {} (giro [{:.0}° {:.0}° {:.0}°]). Repite el gesto, más amplio y solo ese.",
            AXIS[axis], deg[0], deg[1], deg[2]
        ));
    }
    Ok(if deg[axis] > 0.0 { 1 } else { -1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t_us: u64, gyro: [f32; 3], accel: [f32; 3]) -> Sample {
        Sample { t_us, gyro, accel }
    }

    #[test]
    fn signo_de_postura() {
        assert_eq!(pose_sign([0.1, -0.3, 9.7], 2), Ok(1));
        assert_eq!(pose_sign([0.1, -0.3, -9.7], 2), Ok(-1));
        assert_eq!(pose_sign([0.2, -9.6, 0.9], 1), Ok(-1));
        assert!(pose_sign([5.0, 5.0, 5.0], 2).is_err()); // nada domina
        assert!(pose_sign([0.0, 0.0, 3.0], 2).is_err()); // demasiado flojo
    }

    #[test]
    fn signo_de_gesto() {
        assert_eq!(motion_sign([45.0, 3.0, -5.0], 0), Ok(1));
        assert_eq!(motion_sign([-45.0, 3.0, -5.0], 0), Ok(-1));
        assert!(motion_sign([10.0, 0.0, 0.0], 0).is_err()); // poco giro
        assert!(motion_sign([30.0, 28.0, 0.0], 0).is_err()); // ambiguo
    }

    #[test]
    fn integra_el_gyro_con_sus_timestamps() {
        // 1 rad/s sobre x durante 0,5 s a 200 Hz => 28,6°
        let samples: Vec<Sample> = (0..=100).map(|i| s(i * 5000, [1.0, 0.0, 0.0], [0.0; 3])).collect();
        let d = integrate_deg(&samples);
        assert!((d[0] - 28.65).abs() < 0.2, "{d:?}");
        assert_eq!(d[1], 0.0);
        // un hueco de 1 s no cuenta
        let mut with_gap = samples.clone();
        with_gap.push(s(2_000_000, [1.0, 0.0, 0.0], [0.0; 3]));
        assert!((integrate_deg(&with_gap)[0] - d[0]).abs() < 1e-3);
    }

    #[test]
    fn aplica_signos() {
        let a = Axes {
            accel: [1, -1, 1],
            gyro: [-1, 1, 1],
        };
        let o = a.apply(s(1, [1.0, 2.0, 3.0], [4.0, 5.0, 6.0]));
        assert_eq!(o.gyro, [-1.0, 2.0, 3.0]);
        assert_eq!(o.accel, [4.0, -5.0, 6.0]);
        assert_eq!(a.describe(), "accel +-+ gyro -++");
        assert!(Axes::default().is_identity());
        assert!(!a.is_identity());
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(serde_json::from_str::<Axes>(&json).unwrap(), a);
    }

    #[test]
    fn media_de_accel() {
        let m = mean_accel(&[s(0, [0.0; 3], [0.0, 0.0, 9.0]), s(1, [0.0; 3], [0.0, 0.0, 11.0])]);
        assert_eq!(m, [0.0, 0.0, 10.0]);
        assert_eq!(mean_accel(&[]), [0.0; 3]);
    }
}
