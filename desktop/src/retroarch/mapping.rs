//! Del paquete INPUT del móvil (PROTOCOL.md §4.2) al RetroPad, según el
//! mando que el móvil ha elegido para RetroArch (`pad`, PROTOCOL.md §3):
//!
//! - `retropad`: el mando apaisado de dos sticks (el de Wii U / Switch) con
//!   etiquetas de RetroPad. Mismas posiciones físicas que la SNES: A a la
//!   derecha, B abajo, X arriba, Y a la izquierda; ZL/ZR → L2/R2; − / + →
//!   Select / Start; Home → menú de RetroArch; Capturar → avance rápido.
//! - `nes`: el Mando Wii de lado. 1 → B y 2 → A (Consola Virtual), la A y B
//!   grandes → X e Y (tercer y cuarto botón), cruceta visual tal cual.
//! - `gun`: el Mando Wii derecho apuntando. B (gatillo) y A van al ratón del
//!   SO (telemetría: pistola de luz de RetroArch); 1 → B, 2 → A, cruceta,
//!   − / + y Home como en `nes`.
use super::protocol::{axis_from_stick, PadState, RetroPad};
use crate::net::codec::{self, InputPacket};

/// Mando que el móvil hace en RetroArch (vocabulario del mensaje `pad`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RetroPadKind {
    #[default]
    RetroPad,
    Nes,
    Gun,
}

impl RetroPadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RetroPadKind::RetroPad => "retropad",
            RetroPadKind::Nes => "nes",
            RetroPadKind::Gun => "gun",
        }
    }

    /// Nombre del protocolo → mando; None si no es vocabulario de RetroArch.
    pub fn parse(s: &str) -> Option<RetroPadKind> {
        match s {
            "retropad" => Some(RetroPadKind::RetroPad),
            "nes" => Some(RetroPadKind::Nes),
            "gun" => Some(RetroPadKind::Gun),
            _ => None,
        }
    }

    /// El puntero del móvil mueve el ratón del SO (pistola de luz).
    pub fn points(self) -> bool {
        self == RetroPadKind::Gun
    }
}

/// Bits del INPUT que NO son botones del RetroPad y el receptor trata aparte
/// (menú, avance rápido, gatillo/recarga en la pistola).
pub const BIT_MENU: u32 = codec::BTN_HOME;
pub const BIT_FAST_FORWARD: u32 = codec::BTN_SCREEN;

const fn bit(b: RetroPad) -> u16 {
    1 << (b as u16)
}

/// Bit del INPUT → botón del RetroPad para el mando apaisado.
const RETROPAD: [(u32, RetroPad); 16] = [
    (codec::BTN_A, RetroPad::A),
    (codec::BTN_B, RetroPad::B),
    (codec::BTN_X, RetroPad::X),
    (codec::BTN_Y, RetroPad::Y),
    (codec::BTN_L, RetroPad::L),
    (codec::BTN_R, RetroPad::R),
    (codec::BTN_ZL, RetroPad::L2),
    (codec::BTN_ZR, RetroPad::R2),
    (codec::BTN_STICK_L, RetroPad::L3),
    (codec::BTN_STICK_R, RetroPad::R3),
    (codec::BTN_PLUS, RetroPad::Start),
    (codec::BTN_MINUS, RetroPad::Select),
    (codec::BTN_DPAD_UP, RetroPad::Up),
    (codec::BTN_DPAD_DOWN, RetroPad::Down),
    (codec::BTN_DPAD_LEFT, RetroPad::Left),
    (codec::BTN_DPAD_RIGHT, RetroPad::Right),
];

/// Mando Wii (de lado o apuntando): 1 → B, 2 → A; A y B grandes → X e Y.
/// En la pistola A y B no llegan aquí (van al ratón): telemetría los quita.
const WIIMOTE: [(u32, RetroPad); 10] = [
    (codec::BTN_ONE, RetroPad::B),
    (codec::BTN_TWO, RetroPad::A),
    (codec::BTN_A, RetroPad::X),
    (codec::BTN_B, RetroPad::Y),
    (codec::BTN_PLUS, RetroPad::Start),
    (codec::BTN_MINUS, RetroPad::Select),
    (codec::BTN_DPAD_UP, RetroPad::Up),
    (codec::BTN_DPAD_DOWN, RetroPad::Down),
    (codec::BTN_DPAD_LEFT, RetroPad::Left),
    (codec::BTN_DPAD_RIGHT, RetroPad::Right),
];

/// Botones del INPUT que en la pistola son del ratón, no del RetroPad.
pub const GUN_MOUSE_BITS: u32 = codec::BTN_A | codec::BTN_B;

fn buttons(table: &[(u32, RetroPad)], bits: u32) -> u16 {
    table.iter().filter(|(b, _)| bits & b != 0).fold(0, |acc, (_, r)| acc | bit(*r))
}

/// Botones y sticks de un paquete para el mando `kind`.
pub fn pad_state(kind: RetroPadKind, p: &InputPacket) -> PadState {
    match kind {
        RetroPadKind::RetroPad => {
            let sticks = p.flags & codec::FLAG_STICK_VALID != 0;
            let ext = sticks && p.flags & codec::FLAG_EXT != 0;
            PadState {
                buttons: buttons(&RETROPAD, p.buttons),
                axes: [
                    if sticks { axis_from_stick(p.stick_x, false) } else { 0 },
                    if sticks { axis_from_stick(p.stick_y, true) } else { 0 },
                    if ext { axis_from_stick(p.stick_rx, false) } else { 0 },
                    if ext { axis_from_stick(p.stick_ry, true) } else { 0 },
                ],
            }
        }
        RetroPadKind::Nes => PadState { buttons: buttons(&WIIMOTE, p.buttons), axes: [0; 4] },
        RetroPadKind::Gun => PadState { buttons: buttons(&WIIMOTE, p.buttons & !GUN_MOUSE_BITS), axes: [0; 4] },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(buttons: u32, flags: u8) -> InputPacket {
        InputPacket {
            flags,
            session_id: 1,
            seq: 1,
            t_sensor_us: 0,
            quat: [1.0, 0.0, 0.0, 0.0],
            gyro: [0.0; 3],
            accel: [0.0; 3],
            buttons,
            recenter_count: 0,
            battery_pct: 50,
            touch_scroll_dy: 0,
            stick_x: 0,
            stick_y: 0,
            stick_rx: 0,
            stick_ry: 0,
            touch_x: 0,
            touch_y: 0,
        }
    }

    #[test]
    fn el_mando_apaisado_es_una_snes_con_gatillos() {
        let all = codec::BTN_A | codec::BTN_B | codec::BTN_X | codec::BTN_Y | codec::BTN_L | codec::BTN_R
            | codec::BTN_ZL | codec::BTN_ZR | codec::BTN_STICK_L | codec::BTN_STICK_R | codec::BTN_PLUS
            | codec::BTN_MINUS | codec::BTN_DPAD_UP | codec::BTN_DPAD_DOWN | codec::BTN_DPAD_LEFT
            | codec::BTN_DPAD_RIGHT;
        let s = pad_state(RetroPadKind::RetroPad, &packet(all, 0));
        assert_eq!(s.buttons, 0xffff, "los 16 botones del RetroPad");
        let s = pad_state(RetroPadKind::RetroPad, &packet(codec::BTN_A | codec::BTN_ZL | codec::BTN_MINUS, 0));
        assert_eq!(s.buttons, bit(RetroPad::A) | bit(RetroPad::L2) | bit(RetroPad::Select));
        // Home, Capturar, 1, 2, C, Z, micrófono… no son botones del RetroPad
        let s = pad_state(
            RetroPadKind::RetroPad,
            &packet(codec::BTN_HOME | codec::BTN_SCREEN | codec::BTN_ONE | codec::BTN_TWO | codec::BTN_C | codec::BTN_Z | codec::BTN_MIC | codec::BTN_NEAR | codec::BTN_PRECISION, 0),
        );
        assert_eq!(s.buttons, 0);
    }

    #[test]
    fn los_sticks_solo_con_su_bandera_y_con_y_invertida() {
        let mut p = packet(0, 0);
        p.stick_x = 127;
        p.stick_y = 127;
        p.stick_rx = -64;
        p.stick_ry = -127;
        assert_eq!(pad_state(RetroPadKind::RetroPad, &p).axes, [0; 4], "sin FLAG_STICK_VALID no hay stick");
        p.flags = codec::FLAG_STICK_VALID;
        assert_eq!(pad_state(RetroPadKind::RetroPad, &p).axes, [i16::MAX, -i16::MAX, 0, 0], "sin FLAG_EXT no hay stick derecho");
        p.flags = codec::FLAG_STICK_VALID | codec::FLAG_EXT;
        let a = pad_state(RetroPadKind::RetroPad, &p).axes;
        assert_eq!(a[0], i16::MAX);
        assert_eq!(a[1], -i16::MAX, "arriba en el móvil es negativo en libretro");
        assert!(a[2] < -16000 && a[2] > -16600);
        assert_eq!(a[3], i16::MAX, "abajo en el móvil es positivo en libretro");
        // el Mando Wii nunca manda sticks
        assert_eq!(pad_state(RetroPadKind::Nes, &p).axes, [0; 4]);
        assert_eq!(pad_state(RetroPadKind::Gun, &p).axes, [0; 4]);
    }

    #[test]
    fn el_mando_wii_de_lado_es_un_mando_de_nes() {
        let s = pad_state(RetroPadKind::Nes, &packet(codec::BTN_ONE | codec::BTN_DPAD_RIGHT | codec::BTN_PLUS, 0));
        assert_eq!(s.buttons, bit(RetroPad::B) | bit(RetroPad::Right) | bit(RetroPad::Start));
        let s = pad_state(RetroPadKind::Nes, &packet(codec::BTN_TWO | codec::BTN_A | codec::BTN_B | codec::BTN_MINUS, 0));
        assert_eq!(s.buttons, bit(RetroPad::A) | bit(RetroPad::X) | bit(RetroPad::Y) | bit(RetroPad::Select));
        // los del GamePad no existen en un Mando Wii
        assert_eq!(pad_state(RetroPadKind::Nes, &packet(codec::BTN_X | codec::BTN_ZL | codec::BTN_STICK_L, 0)).buttons, 0);
    }

    #[test]
    fn en_la_pistola_a_y_b_son_del_raton() {
        let s = pad_state(RetroPadKind::Gun, &packet(codec::BTN_A | codec::BTN_B | codec::BTN_ONE | codec::BTN_TWO, 0));
        assert_eq!(s.buttons, bit(RetroPad::B) | bit(RetroPad::A));
        assert!(RetroPadKind::Gun.points());
        assert!(!RetroPadKind::Nes.points());
    }

    #[test]
    fn nombres_del_protocolo() {
        for k in [RetroPadKind::RetroPad, RetroPadKind::Nes, RetroPadKind::Gun] {
            assert_eq!(RetroPadKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(RetroPadKind::parse("pro"), None);
        assert_eq!(RetroPadKind::parse("wiimote"), None);
        assert_eq!(RetroPadKind::default(), RetroPadKind::RetroPad);
    }
}
