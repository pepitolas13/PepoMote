//! Remapeo de los sensores cuando el móvil se sostiene apaisado como GamePad
//! de Wii U (modo Cemu). El receptor y Cemu esperan el marco de un DS4
//! tumbado: X = derecha del mando, Y = hacia el borde superior del mando
//! (lejos del jugador), Z = saliendo de la pantalla. En vertical (Wiimote)
//! eso coincide con los ejes del móvil y no se toca nada; apaisado hay que
//! remapear ANTES de escribir el paquete, según hacia dónde quede el borde
//! superior del móvil. Todo son funciones puras.

use serde::{Deserialize, Serialize};

/// Cómo está girado el móvil apaisado: dónde queda su borde superior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rotation {
    /// Borde superior a la IZQUIERDA (Android `ROTATION_90`). Por defecto.
    #[default]
    Left,
    /// Borde superior a la DERECHA (`ROTATION_270`).
    Right,
}

impl Rotation {
    /// Codificación en el atómico que comparte la UI con el hilo de paquetes:
    /// 0 = izquierda, 1 = derecha. Cualquier otro valor = sin remapeo.
    pub fn from_u8(v: u8) -> Option<Rotation> {
        match v {
            0 => Some(Rotation::Left),
            1 => Some(Rotation::Right),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Rotation::Left => 0,
            Rotation::Right => 1,
        }
    }

    pub fn toggled(self) -> Rotation {
        match self {
            Rotation::Left => Rotation::Right,
            Rotation::Right => Rotation::Left,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Rotation::Left => "izquierda",
            Rotation::Right => "derecha",
        }
    }
}

/// √½ = 0,70710678 del contrato (mismo f32 que la constante de la biblioteca).
const HALF_SQRT2: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Producto de Hamilton `q ⊗ r` (orden w, x, y, z; `r` a la derecha).
pub fn hamilton(q: [f32; 4], r: [f32; 4]) -> [f32; 4] {
    let [w1, x1, y1, z1] = q;
    let [w2, x2, y2, z2] = r;
    [
        w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
        w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
        w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
        w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
    ]
}

/// Vector (accel o gyro) del marco del móvil al del mando apaisado.
/// `0.0 − x` en vez de `−x`: igual para todo valor no nulo, pero un eje a 0
/// no se convierte en −0.0 (así el cable lleva exactamente los mismos bytes
/// que sin remapeo cuando el eje es cero).
pub fn remap_vec(v: [f32; 3], rot: Rotation) -> [f32; 3] {
    match rot {
        Rotation::Left => [0.0 - v[1], v[0], v[2]],
        Rotation::Right => [v[1], 0.0 - v[0], v[2]],
    }
}

/// Orientación: `quat ⊗ (√½, 0, 0, ∓√½)` (− con el borde superior a la
/// izquierda, + a la derecha).
pub fn remap_quat(q: [f32; 4], rot: Rotation) -> [f32; 4] {
    let r = match rot {
        Rotation::Left => [HALF_SQRT2, 0.0, 0.0, -HALF_SQRT2],
        Rotation::Right => [HALF_SQRT2, 0.0, 0.0, HALF_SQRT2],
    };
    hamilton(q, r)
}

/// Los tres a la vez; sin giro conocido (`None`) no se toca nada.
pub fn remap(quat: [f32; 4], gyro: [f32; 3], accel: [f32; 3], rot: Option<Rotation>) -> ([f32; 4], [f32; 3], [f32; 3]) {
    match rot {
        Some(r) => (remap_quat(quat, r), remap_vec(gyro, r), remap_vec(accel, r)),
        None => (quat, gyro, accel),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    #[test]
    fn plano_boca_arriba_sigue_igual() {
        let g = [0.0, 0.0, 9.8];
        assert_eq!(remap_vec(g, Rotation::Left), g);
        assert_eq!(remap_vec(g, Rotation::Right), g);
    }

    #[test]
    fn ejes_x_e_y_del_movil_giran_con_el() {
        assert_eq!(remap_vec([1.0, 0.0, 0.0], Rotation::Left), [0.0, 1.0, 0.0]);
        assert_eq!(remap_vec([1.0, 0.0, 0.0], Rotation::Right), [0.0, -1.0, 0.0]);
        assert_eq!(remap_vec([0.0, 1.0, 0.0], Rotation::Left), [-1.0, 0.0, 0.0]);
        assert_eq!(remap_vec([0.0, 1.0, 0.0], Rotation::Right), [1.0, 0.0, 0.0]);
        // el gyro usa exactamente la misma permutación
        let (_, g, a) = remap([1.0, 0.0, 0.0, 0.0], [1.0, 2.0, 3.0], [1.0, 2.0, 3.0], Some(Rotation::Left));
        assert_eq!(g, [-2.0, 1.0, 3.0]);
        assert_eq!(a, [-2.0, 1.0, 3.0]);
    }

    #[test]
    fn quat_identidad_pasa_a_medio_giro_en_z() {
        let id = [1.0, 0.0, 0.0, 0.0];
        assert!(close(remap_quat(id, Rotation::Left), [0.7071068, 0.0, 0.0, -0.7071068]));
        assert!(close(remap_quat(id, Rotation::Right), [0.7071068, 0.0, 0.0, 0.7071068]));
    }

    #[test]
    fn producto_de_hamilton() {
        let i = [0.0, 1.0, 0.0, 0.0];
        let j = [0.0, 0.0, 1.0, 0.0];
        let k = [0.0, 0.0, 0.0, 1.0];
        assert_eq!(hamilton(i, j), k, "i ⊗ j = k");
        assert_eq!(hamilton(j, i), [0.0, 0.0, 0.0, -1.0], "no conmuta: j ⊗ i = −k");
        let q = [0.5, -0.5, 0.5, -0.5];
        assert_eq!(hamilton(q, [1.0, 0.0, 0.0, 0.0]), q, "la identidad a la derecha no cambia nada");
        // r a la DERECHA: el orden importa y es el del contrato
        let r = [HALF_SQRT2, 0.0, 0.0, -HALF_SQRT2];
        assert!(!close(hamilton(q, r), hamilton(r, q)));
        assert!(close(remap_quat(q, Rotation::Left), hamilton(q, r)));
    }

    #[test]
    fn sin_giro_conocido_no_se_remapea() {
        let q = [0.5, -0.5, 0.5, -0.5];
        let g = [1.0, -1.0, 0.5];
        let a = [-1.0, 2.0, -0.5];
        assert_eq!(remap(q, g, a, None), (q, g, a));
        assert_eq!(Rotation::from_u8(0), Some(Rotation::Left));
        assert_eq!(Rotation::from_u8(1), Some(Rotation::Right));
        assert_eq!(Rotation::from_u8(2), None, "otro valor: sin remapeo");
        assert_eq!(Rotation::Left.as_u8(), 0);
        assert_eq!(Rotation::Right.as_u8(), 1);
        assert_eq!(Rotation::Left.toggled(), Rotation::Right);
        assert_eq!(Rotation::Right.toggled(), Rotation::Left);
        assert_eq!(Rotation::default(), Rotation::Left, "por defecto, borde superior a la izquierda");
    }

    #[test]
    fn el_giro_se_guarda_en_minusculas() {
        assert_eq!(serde_json::to_string(&Rotation::Right).unwrap(), "\"right\"");
        assert_eq!(serde_json::from_str::<Rotation>("\"left\"").unwrap(), Rotation::Left);
    }
}
