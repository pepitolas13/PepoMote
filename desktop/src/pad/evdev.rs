//! El estado del mando en eventos de evdev, en crudo.
//!
//! Vive aquí y no en `rumble::linux` a propósito: así se compila y **se
//! prueba en los tres sistemas**, no solo en Linux. Lo que se juega en estas
//! veinte líneas — la vertical al revés, el signo de la cruceta, qué código
//! lleva cada botón — es justo lo que no se puede dejar sin red, porque un
//! fallo aquí no se ve hasta que alguien juega en Linux.
//!
//! Los números son la ABI de Linux (`include/uapi/linux/input-event-codes.h`),
//! que es estable desde siempre. `rumble::linux` comprueba en sus propios
//! tests que coinciden con los del crate `evdev`.

use super::mapping::xb;
use super::PadState;

/// Tipos de evento (`EV_KEY`, `EV_ABS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Key,
    Abs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ev {
    pub kind: Kind,
    pub code: u16,
    pub value: i32,
}

// Botones. En Linux los nombres canónicos son POR POSICIÓN (`SOUTH` es el de
// abajo), que es como está definido el rombo del mando universal. Ojo con los
// alias del kernel: `BTN_X` es `BTN_NORTH` (arriba) y `BTN_Y` es `BTN_WEST`
// (izquierda), al revés que en un mando de Xbox. Por eso aquí solo se usan
// los nombres de posición.
pub const BTN_SOUTH: u16 = 0x130;
pub const BTN_EAST: u16 = 0x131;
pub const BTN_NORTH: u16 = 0x133;
pub const BTN_WEST: u16 = 0x134;
pub const BTN_TL: u16 = 0x136;
pub const BTN_TR: u16 = 0x137;
pub const BTN_SELECT: u16 = 0x13a;
pub const BTN_START: u16 = 0x13b;
pub const BTN_MODE: u16 = 0x13c;
pub const BTN_THUMBL: u16 = 0x13d;
pub const BTN_THUMBR: u16 = 0x13e;

// Ejes.
pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_Z: u16 = 0x02;
pub const ABS_RX: u16 = 0x03;
pub const ABS_RY: u16 = 0x04;
pub const ABS_RZ: u16 = 0x05;
pub const ABS_HAT0X: u16 = 0x10;
pub const ABS_HAT0Y: u16 = 0x11;

/// Bit de XInput → tecla de evdev, por posición en el mando.
pub const KEYS: [(u16, u16); 11] = [
    (xb::A, BTN_SOUTH),
    (xb::B, BTN_EAST),
    (xb::X, BTN_WEST),
    (xb::Y, BTN_NORTH),
    (xb::LB, BTN_TL),
    (xb::RB, BTN_TR),
    (xb::BACK, BTN_SELECT),
    (xb::START, BTN_START),
    (xb::GUIDE, BTN_MODE),
    (xb::LTHUMB, BTN_THUMBL),
    (xb::RTHUMB, BTN_THUMBR),
];

fn hat(pos: bool, neg: bool) -> i32 {
    i32::from(pos) - i32::from(neg)
}

/// Los eventos que describen este estado, listos para `emit`.
pub fn events(s: &PadState) -> Vec<Ev> {
    let mut out = Vec::with_capacity(KEYS.len() + 8);
    for (bit, code) in KEYS {
        out.push(Ev { kind: Kind::Key, code, value: i32::from(s.buttons & bit != 0) });
    }
    for (code, value) in [
        (ABS_HAT0X, hat(s.buttons & xb::RIGHT != 0, s.buttons & xb::LEFT != 0)),
        // La cruceta del hat crece hacia ABAJO, al revés que el bit de XInput.
        (ABS_HAT0Y, hat(s.buttons & xb::DOWN != 0, s.buttons & xb::UP != 0)),
        (ABS_X, i32::from(s.lx)),
        // Y aquí está la trampa gorda: en evdev la vertical de los sticks
        // crece hacia ABAJO y en XInput hacia arriba. Sin este signo se
        // juega con la cámara invertida y nadie sabe por qué.
        (ABS_Y, -i32::from(s.ly)),
        (ABS_RX, i32::from(s.rx)),
        (ABS_RY, -i32::from(s.ry)),
        (ABS_Z, i32::from(s.lt)),
        (ABS_RZ, i32::from(s.rt)),
    ] {
        out.push(Ev { kind: Kind::Abs, code, value });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valor(evs: &[Ev], kind: Kind, code: u16) -> i32 {
        evs.iter().find(|e| e.kind == kind && e.code == code).expect("ese evento").value
    }

    fn con(s: PadState) -> Vec<Ev> {
        events(&s)
    }

    #[test]
    fn el_reposo_manda_todo_a_cero() {
        for e in con(PadState::default()) {
            assert_eq!(e.value, 0, "{e:?}");
        }
    }

    #[test]
    fn siempre_salen_todos_los_controles() {
        // Se manda el estado entero, no solo lo que cambia: así un mando
        // recién creado queda coherente de una sola vez.
        let evs = con(PadState::default());
        assert_eq!(evs.len(), KEYS.len() + 8);
    }

    #[test]
    fn el_rombo_va_por_posicion() {
        assert_eq!(valor(&con(PadState { buttons: xb::A, ..Default::default() }), Kind::Key, BTN_SOUTH), 1);
        assert_eq!(valor(&con(PadState { buttons: xb::B, ..Default::default() }), Kind::Key, BTN_EAST), 1);
        // Los alias del kernel mienten: la X de Xbox es la de la IZQUIERDA.
        assert_eq!(valor(&con(PadState { buttons: xb::X, ..Default::default() }), Kind::Key, BTN_WEST), 1);
        assert_eq!(valor(&con(PadState { buttons: xb::Y, ..Default::default() }), Kind::Key, BTN_NORTH), 1);
    }

    #[test]
    fn la_vertical_de_los_sticks_va_al_reves_que_en_xinput() {
        let arriba = PadState { ly: 32767, ry: 32767, ..Default::default() };
        let evs = con(arriba);
        assert_eq!(valor(&evs, Kind::Abs, ABS_Y), -32767, "arriba en XInput es negativo en evdev");
        assert_eq!(valor(&evs, Kind::Abs, ABS_RY), -32767);
    }

    #[test]
    fn la_horizontal_de_los_sticks_no_se_toca() {
        let evs = con(PadState { lx: 32767, rx: -32767, ..Default::default() });
        assert_eq!(valor(&evs, Kind::Abs, ABS_X), 32767);
        assert_eq!(valor(&evs, Kind::Abs, ABS_RX), -32767);
    }

    #[test]
    fn la_cruceta_va_por_el_hat() {
        let evs = con(PadState { buttons: xb::RIGHT | xb::UP, ..Default::default() });
        assert_eq!(valor(&evs, Kind::Abs, ABS_HAT0X), 1);
        assert_eq!(valor(&evs, Kind::Abs, ABS_HAT0Y), -1, "arriba es negativo en el hat");

        let evs = con(PadState { buttons: xb::LEFT | xb::DOWN, ..Default::default() });
        assert_eq!(valor(&evs, Kind::Abs, ABS_HAT0X), -1);
        assert_eq!(valor(&evs, Kind::Abs, ABS_HAT0Y), 1);
    }

    #[test]
    fn izquierda_y_derecha_a_la_vez_se_anulan() {
        let evs = con(PadState { buttons: xb::LEFT | xb::RIGHT, ..Default::default() });
        assert_eq!(valor(&evs, Kind::Abs, ABS_HAT0X), 0, "el hat no tiene dos sentidos a la vez");
    }

    #[test]
    fn los_gatillos_van_a_sus_ejes() {
        let evs = con(PadState { lt: 255, rt: 128, ..Default::default() });
        assert_eq!(valor(&evs, Kind::Abs, ABS_Z), 255);
        assert_eq!(valor(&evs, Kind::Abs, ABS_RZ), 128);
    }

    #[test]
    fn soltar_un_boton_manda_un_cero_explicito() {
        // Nunca se mandan solo los cambios, así que el 0 tiene que estar.
        let evs = con(PadState { buttons: xb::A, ..Default::default() });
        assert_eq!(valor(&evs, Kind::Key, BTN_EAST), 0);
    }

    #[test]
    fn ningun_codigo_se_repite() {
        let evs = con(PadState::default());
        for (i, a) in evs.iter().enumerate() {
            for b in &evs[i + 1..] {
                assert!(!(a.kind == b.kind && a.code == b.code), "código repetido: {a:?}");
            }
        }
    }
}
