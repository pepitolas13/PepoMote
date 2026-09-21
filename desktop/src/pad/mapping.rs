//! Del INPUT del móvil al estado de un mando de Xbox 360. Puro y sin E/S,
//! como `retroarch::mapping` y `dsu::mapping`: la tabla se lee de un vistazo
//! y se prueba sin levantar nada.
//!
//! Los bits de [`xb`] son los de XInput tal cual (los mismos valores que
//! `XINPUT_GAMEPAD.wButtons`), para que en Windows el estado pase a
//! `vigem_client::XGamepad` sin traducir. En Linux cada bit se lleva a su
//! tecla de evdev en `rumble::linux`.

use pmp::InputPacket;

/// Bits de botón de XInput (`XINPUT_GAMEPAD.wButtons`).
pub mod xb {
    pub const UP: u16 = 0x0001;
    pub const DOWN: u16 = 0x0002;
    pub const LEFT: u16 = 0x0004;
    pub const RIGHT: u16 = 0x0008;
    pub const START: u16 = 0x0010;
    pub const BACK: u16 = 0x0020;
    pub const LTHUMB: u16 = 0x0040;
    pub const RTHUMB: u16 = 0x0080;
    pub const LB: u16 = 0x0100;
    pub const RB: u16 = 0x0200;
    pub const GUIDE: u16 = 0x0400;
    pub const A: u16 = 0x1000;
    pub const B: u16 = 0x2000;
    pub const X: u16 = 0x4000;
    pub const Y: u16 = 0x8000;
}

/// Estado de un mando de Xbox 360, neutro entre plataformas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PadState {
    pub buttons: u16,
    pub lt: u8,
    pub rt: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

/// Bit del INPUT → bit de XInput.
///
/// El rombo va **por posición en la pantalla, no por letra**: la plantilla
/// `XBOX` del móvil pone A abajo, B a la derecha, X a la izquierda e Y
/// arriba, cada una emitiendo su propio bit, así que aquí la correspondencia
/// es directa y lo que el jugador lee es lo que el juego recibe. (La
/// plantilla `RETROPAD` de Nintendo, con A a la derecha, no se usa en este
/// modo precisamente para no tener que cruzar nada.)
const BUTTONS: [(u32, u16); 15] = [
    (pmp::BTN_A, xb::A),
    (pmp::BTN_B, xb::B),
    (pmp::BTN_X, xb::X),
    (pmp::BTN_Y, xb::Y),
    (pmp::BTN_DPAD_UP, xb::UP),
    (pmp::BTN_DPAD_DOWN, xb::DOWN),
    (pmp::BTN_DPAD_LEFT, xb::LEFT),
    (pmp::BTN_DPAD_RIGHT, xb::RIGHT),
    (pmp::BTN_L, xb::LB),
    (pmp::BTN_R, xb::RB),
    (pmp::BTN_PLUS, xb::START),
    (pmp::BTN_MINUS, xb::BACK),
    (pmp::BTN_HOME, xb::GUIDE),
    (pmp::BTN_STICK_L, xb::LTHUMB),
    (pmp::BTN_STICK_R, xb::RTHUMB),
];

/// ZL y ZR son digitales en el móvil y analógicos en el mando: pulsado = a
/// fondo. Un mando de Xbox da por pisado el gatillo a partir de 30, así que
/// 255 no deja lugar a dudas.
const TRIGGER_ON: u8 = 255;

/// Eje del móvil (−127..127) al del mando (−32767..32767). El signo no se
/// toca: en PMP y en XInput el positivo es arriba y a la derecha en los dos.
fn axis(v: i8) -> i16 {
    (i32::from(v.clamp(-127, 127)) * 32767 / 127) as i16
}

/// El estado del mando para este paquete. `aim` es el stick derecho que sale
/// del giroscopio; se usa solo si el dedo no está moviendo el stick de la
/// pantalla, para que no se peleen.
pub fn pad_state(p: &InputPacket, aim: (i16, i16)) -> PadState {
    let mut s = PadState::default();

    for (pmp_bit, x_bit) in BUTTONS {
        if p.buttons & pmp_bit != 0 {
            s.buttons |= x_bit;
        }
    }
    if p.buttons & pmp::BTN_ZL != 0 {
        s.lt = TRIGGER_ON;
    }
    if p.buttons & pmp::BTN_ZR != 0 {
        s.rt = TRIGGER_ON;
    }

    if p.flags & pmp::FLAG_STICK_VALID != 0 {
        s.lx = axis(p.stick_x);
        s.ly = axis(p.stick_y);
    }

    // El stick derecho de la pantalla manda sobre el giro: el dedo es
    // intención explícita y el giro es de fondo.
    let touch_stick = p.flags & pmp::FLAG_EXT != 0 && (p.stick_rx != 0 || p.stick_ry != 0);
    if touch_stick {
        s.rx = axis(p.stick_rx);
        s.ry = axis(p.stick_ry);
    } else {
        s.rx = aim.0;
        s.ry = aim.1;
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(buttons: u32) -> InputPacket {
        let mut p = InputPacket::default();
        p.buttons = buttons;
        p
    }

    #[test]
    fn cada_boton_va_a_su_bit_de_xinput() {
        for (pmp_bit, x_bit) in BUTTONS {
            let s = pad_state(&packet(pmp_bit), (0, 0));
            assert_eq!(s.buttons, x_bit, "el bit {pmp_bit:#x} del INPUT");
        }
    }

    #[test]
    fn el_boton_de_abajo_del_rombo_es_la_a_de_xbox() {
        // La plantilla XBOX del móvil dibuja A abajo, B derecha, X izquierda
        // e Y arriba. Si alguien cambia esta tabla al estilo Nintendo, el
        // jugador pulsaría «A» y el juego recibiría otra cosa.
        assert_eq!(pad_state(&packet(pmp::BTN_A), (0, 0)).buttons, xb::A);
        assert_eq!(pad_state(&packet(pmp::BTN_B), (0, 0)).buttons, xb::B);
        assert_eq!(pad_state(&packet(pmp::BTN_X), (0, 0)).buttons, xb::X);
        assert_eq!(pad_state(&packet(pmp::BTN_Y), (0, 0)).buttons, xb::Y);
    }

    #[test]
    fn varios_botones_a_la_vez_se_suman() {
        let s = pad_state(&packet(pmp::BTN_A | pmp::BTN_DPAD_UP | pmp::BTN_L), (0, 0));
        assert_eq!(s.buttons, xb::A | xb::UP | xb::LB);
    }

    #[test]
    fn los_bits_que_no_son_del_mando_no_pulsan_nada() {
        // Precisión y Acercar son «mantener» del receptor; el micro y los
        // multimedia no existen en un mando de Xbox.
        let otros = pmp::BTN_PRECISION | pmp::BTN_NEAR | pmp::BTN_MIC | pmp::BTN_MEDIA_MUTE | pmp::BTN_ONE | pmp::BTN_TWO;
        let s = pad_state(&packet(otros), (0, 0));
        assert_eq!(s.buttons, 0);
        assert_eq!((s.lt, s.rt), (0, 0));
    }

    #[test]
    fn zl_y_zr_pisan_los_gatillos_a_fondo_y_no_son_botones() {
        let s = pad_state(&packet(pmp::BTN_ZL | pmp::BTN_ZR), (0, 0));
        assert_eq!((s.lt, s.rt), (TRIGGER_ON, TRIGGER_ON));
        assert_eq!(s.buttons, 0, "los gatillos no ocupan ningún bit");
    }

    #[test]
    fn el_stick_escala_a_fondo_sin_invertir() {
        let mut p = packet(0);
        p.flags |= pmp::FLAG_STICK_VALID;
        p.stick_x = 127;
        p.stick_y = 127;
        let s = pad_state(&p, (0, 0));
        assert_eq!((s.lx, s.ly), (32767, 32767), "arriba y derecha son positivos en los dos");

        p.stick_x = -127;
        p.stick_y = -127;
        let s = pad_state(&p, (0, 0));
        assert_eq!((s.lx, s.ly), (-32767, -32767));

        p.stick_x = 0;
        p.stick_y = 0;
        assert_eq!((pad_state(&p, (0, 0)).lx, pad_state(&p, (0, 0)).ly), (0, 0));
    }

    #[test]
    fn sin_la_bandera_del_stick_se_queda_en_el_centro() {
        let mut p = packet(0);
        p.stick_x = 127;
        p.stick_y = -127;
        let s = pad_state(&p, (0, 0));
        assert_eq!((s.lx, s.ly), (0, 0), "un móvil que no manda stick no lo mueve");
    }

    #[test]
    fn el_giro_mueve_el_stick_derecho_si_no_hay_dedo() {
        let mut p = packet(0);
        p.flags |= pmp::FLAG_EXT;
        let s = pad_state(&p, (5000, -6000));
        assert_eq!((s.rx, s.ry), (5000, -6000));
    }

    #[test]
    fn el_dedo_en_el_stick_derecho_gana_al_giro() {
        let mut p = packet(0);
        p.flags |= pmp::FLAG_EXT;
        p.stick_rx = 100;
        let s = pad_state(&p, (5000, -6000));
        assert_eq!(s.rx, axis(100));
        assert_eq!(s.ry, 0, "el dedo manda en los dos ejes a la vez");
    }

    #[test]
    fn sin_bloque_extendido_el_giro_sigue_valiendo() {
        // Un móvil que manda paquetes de 72 B no tiene stick derecho táctil,
        // pero sí giroscopio: apuntar tiene que seguir funcionando.
        let s = pad_state(&packet(0), (7000, 0));
        assert_eq!((s.rx, s.ry), (7000, 0));
    }
}
