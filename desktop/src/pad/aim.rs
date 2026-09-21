//! Apuntado por giro: el movimiento del móvil mueve el stick derecho.
//!
//! Un mando de Xbox no tiene giroscopio, así que la única forma de que el
//! giro llegue a un juego cualquiera es por el stick. Cuanto más deprisa
//! gira el móvil, más lejos del centro se va el stick — igual que hace
//! Steam Input con los mandos de Switch.
//!
//! El sesgo del giroscopio se mata con la **zona muerta suave** del motor de
//! puntero, que es la respuesta que ya tiene este proyecto para el giro sin
//! referencia absoluta (el fallback h1 de `pointer::engine`): un sesgo
//! pequeño sin cancelar dejaría el stick fuera del centro para siempre y la
//! cámara giraría sola. Por eso aquí no hay estado ni estimador: función
//! pura, mismo umbral, mismos signos.

use crate::pointer::{soft_deadzone, GYRO_DEADZONE_RADS, SIGN_X, SIGN_Y};
use pmp::InputPacket;

/// Velocidad de giro que lleva el stick al tope, en grados por segundo.
/// A menos, más sensible. 220 °/s es un giro de muñeca rápido pero cómodo.
pub const FULL_SCALE_DEG_S: f32 = 220.0;

const FULL: i16 = 32767;

/// El stick derecho que pide el giro de este paquete, o el centro si el
/// móvil está quieto (o no tiene giroscopio de verdad).
pub fn stick(p: &InputPacket, full_scale_deg_s: f32) -> (i16, i16) {
    // `FLAG_TILT` es un móvil sin giroscopio real apuntando por inclinación:
    // su `gyro` no dice nada y moverlo sería ruido.
    if p.flags & pmp::FLAG_TILT != 0 {
        return (0, 0);
    }
    let full = full_scale_deg_s.max(1.0).to_radians();

    // Los mismos ejes y signos que el fallback relativo del puntero: el
    // horizontal sale de `gyro[2]` y el vertical de `gyro[0]`.
    let gz = soft_deadzone(p.gyro[2], GYRO_DEADZONE_RADS);
    let gx = soft_deadzone(p.gyro[0], GYRO_DEADZONE_RADS);

    // En pantalla la Y crece hacia abajo; en el stick, hacia arriba. De ahí
    // el signo cambiado en el vertical y no en el horizontal.
    let x = SIGN_X * gz / full;
    let y = -SIGN_Y * gx / full;
    (clamp(x), clamp(y))
}

fn clamp(v: f32) -> i16 {
    if !v.is_finite() {
        return 0;
    }
    (v.clamp(-1.0, 1.0) * f32::from(FULL)).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn con_gyro(gx: f32, gz: f32) -> InputPacket {
        let mut p = InputPacket::default();
        p.gyro = [gx, 0.0, gz];
        p
    }

    #[test]
    fn el_movil_quieto_deja_el_stick_en_el_centro() {
        assert_eq!(stick(&con_gyro(0.0, 0.0), FULL_SCALE_DEG_S), (0, 0));
    }

    #[test]
    fn el_sesgo_pequeno_del_giroscopio_no_mueve_nada() {
        // Un MEMS barato en reposo deriva bastante menos que la zona muerta
        // (0,03 rad/s ≈ 1,7 °/s). Si esto se rompe, la cámara gira sola.
        let casi = GYRO_DEADZONE_RADS * 0.9;
        assert_eq!(stick(&con_gyro(casi, casi), FULL_SCALE_DEG_S), (0, 0));
        assert_eq!(stick(&con_gyro(-casi, -casi), FULL_SCALE_DEG_S), (0, 0));
    }

    #[test]
    fn a_la_velocidad_de_tope_el_stick_llega_al_maximo() {
        let tope = FULL_SCALE_DEG_S.to_radians() + GYRO_DEADZONE_RADS;
        let (x, _) = stick(&con_gyro(0.0, tope), FULL_SCALE_DEG_S);
        assert_eq!(x.abs(), FULL);
        let (_, y) = stick(&con_gyro(tope, 0.0), FULL_SCALE_DEG_S);
        assert_eq!(y.abs(), FULL);
    }

    #[test]
    fn pasarse_de_vueltas_no_desborda() {
        let (x, y) = stick(&con_gyro(500.0, -500.0), FULL_SCALE_DEG_S);
        assert_eq!((x.abs(), y.abs()), (FULL, FULL), "se recorta, no da la vuelta");
    }

    #[test]
    fn los_ejes_van_como_el_puntero() {
        // Mismo convenio que el fallback relativo de `pointer::engine`: el
        // horizontal sale de gyro[2] y el vertical de gyro[0], con la Y del
        // stick al revés que la de la pantalla.
        let medio = FULL_SCALE_DEG_S.to_radians() / 2.0 + GYRO_DEADZONE_RADS;
        let (x, y) = stick(&con_gyro(0.0, medio), FULL_SCALE_DEG_S);
        assert_eq!(y, 0, "gyro[2] no toca el vertical");
        assert_eq!(x.signum(), SIGN_X.signum() as i16);

        let (x, y) = stick(&con_gyro(medio, 0.0), FULL_SCALE_DEG_S);
        assert_eq!(x, 0, "gyro[0] no toca el horizontal");
        assert_eq!(y.signum(), -SIGN_Y.signum() as i16);
    }

    #[test]
    fn mas_sensibilidad_es_menos_grados_por_segundo() {
        let g = 1.0_f32;
        let (lento, _) = stick(&con_gyro(0.0, g), 400.0);
        let (rapido, _) = stick(&con_gyro(0.0, g), 100.0);
        assert!(rapido.abs() > lento.abs(), "{rapido} vs {lento}");
    }

    #[test]
    fn un_movil_sin_giroscopio_no_apunta() {
        let mut p = con_gyro(5.0, 5.0);
        p.flags |= pmp::FLAG_TILT;
        assert_eq!(stick(&p, FULL_SCALE_DEG_S), (0, 0));
    }

    #[test]
    fn un_gyro_roto_no_tumba_nada() {
        assert_eq!(stick(&con_gyro(f32::NAN, f32::INFINITY), FULL_SCALE_DEG_S), (0, 0));
        let (x, y) = stick(&con_gyro(1.0, 1.0), 0.0);
        assert_eq!((x.abs(), y.abs()), (FULL, FULL), "sensibilidad cero no divide por cero");
    }
}
