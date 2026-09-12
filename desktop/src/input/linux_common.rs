//! Lo que comparten los dos backends de Linux (uinput y Wayland): el mapeo
//! del apuntado absoluto, las tablas de teclas y el acumulador de la rueda.

use super::KeyCode;
use evdev::Key;

/// Valor máximo de los ejes absolutos. El dispositivo cubre el escritorio
/// ENTERO (uinput: ABS_X/ABS_Y; Wayland: extent de `motion_absolute`).
pub(super) const ABS_MAX: i32 = 32767;

/// (nx, ny) de la pantalla objetivo → valor absoluto del dispositivo, que
/// cubre el escritorio entero.
pub(super) fn map_abs(nx: f32, ny: f32, target: [f32; 4]) -> (i32, i32) {
    let x = target[0] + nx.clamp(0.0, 1.0) * target[2];
    let y = target[1] + ny.clamp(0.0, 1.0) * target[3];
    (
        (x.clamp(0.0, 1.0) * ABS_MAX as f32).round() as i32,
        (y.clamp(0.0, 1.0) * ABS_MAX as f32).round() as i32,
    )
}

/// Caracteres con tecla propia en el teclado virtual.
pub(super) const ASCII_KEYS: &str = "abcdefghijklmnopqrstuvwxyz0123456789-=.,';/[]`\\";

/// Teclas sin carácter del teclado virtual.
pub(super) const FIXED_KEYS: [KeyCode; 15] = [
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::Enter,
    KeyCode::Escape,
    KeyCode::VolumeUp,
    KeyCode::VolumeDown,
    KeyCode::Mute,
    KeyCode::PlayPause,
    KeyCode::NextTrack,
    KeyCode::PrevTrack,
    KeyCode::Backspace,
    KeyCode::Space,
    KeyCode::Shift,
];

/// Todas las teclas que existen en ambos backends: las fijas y una por
/// carácter de [`ASCII_KEYS`].
pub(super) fn all_keys() -> impl Iterator<Item = KeyCode> {
    FIXED_KEYS
        .iter()
        .copied()
        .chain(ASCII_KEYS.chars().map(KeyCode::Char))
}

/// Tecla evdev de cualquier [`KeyCode`] (None: carácter sin tecla).
pub(super) fn evdev_key(key: KeyCode) -> Option<Key> {
    Some(match key {
        KeyCode::ArrowUp => Key::KEY_UP,
        KeyCode::ArrowDown => Key::KEY_DOWN,
        KeyCode::ArrowLeft => Key::KEY_LEFT,
        KeyCode::ArrowRight => Key::KEY_RIGHT,
        KeyCode::Enter => Key::KEY_ENTER,
        KeyCode::Escape => Key::KEY_ESC,
        KeyCode::VolumeUp => Key::KEY_VOLUMEUP,
        KeyCode::VolumeDown => Key::KEY_VOLUMEDOWN,
        KeyCode::Mute => Key::KEY_MUTE,
        KeyCode::PlayPause => Key::KEY_PLAYPAUSE,
        KeyCode::NextTrack => Key::KEY_NEXTSONG,
        KeyCode::PrevTrack => Key::KEY_PREVIOUSSONG,
        KeyCode::Backspace => Key::KEY_BACKSPACE,
        KeyCode::Space => Key::KEY_SPACE,
        KeyCode::Shift => Key::KEY_LEFTSHIFT,
        KeyCode::Char(c) => return ascii_key(c),
    })
}

/// Tecla evdev de un carácter ASCII en la disposición QWERTY (letras y
/// dígitos coinciden en la española; los signos pueden variar).
pub(super) fn ascii_key(c: char) -> Option<Key> {
    Some(match c {
        'a' => Key::KEY_A,
        'b' => Key::KEY_B,
        'c' => Key::KEY_C,
        'd' => Key::KEY_D,
        'e' => Key::KEY_E,
        'f' => Key::KEY_F,
        'g' => Key::KEY_G,
        'h' => Key::KEY_H,
        'i' => Key::KEY_I,
        'j' => Key::KEY_J,
        'k' => Key::KEY_K,
        'l' => Key::KEY_L,
        'm' => Key::KEY_M,
        'n' => Key::KEY_N,
        'o' => Key::KEY_O,
        'p' => Key::KEY_P,
        'q' => Key::KEY_Q,
        'r' => Key::KEY_R,
        's' => Key::KEY_S,
        't' => Key::KEY_T,
        'u' => Key::KEY_U,
        'v' => Key::KEY_V,
        'w' => Key::KEY_W,
        'x' => Key::KEY_X,
        'y' => Key::KEY_Y,
        'z' => Key::KEY_Z,
        '0' => Key::KEY_0,
        '1' => Key::KEY_1,
        '2' => Key::KEY_2,
        '3' => Key::KEY_3,
        '4' => Key::KEY_4,
        '5' => Key::KEY_5,
        '6' => Key::KEY_6,
        '7' => Key::KEY_7,
        '8' => Key::KEY_8,
        '9' => Key::KEY_9,
        '-' => Key::KEY_MINUS,
        '=' => Key::KEY_EQUAL,
        '.' => Key::KEY_DOT,
        ',' => Key::KEY_COMMA,
        '\'' => Key::KEY_APOSTROPHE,
        ';' => Key::KEY_SEMICOLON,
        '/' => Key::KEY_SLASH,
        '[' => Key::KEY_LEFTBRACE,
        ']' => Key::KEY_RIGHTBRACE,
        '`' => Key::KEY_GRAVE,
        '\\' => Key::KEY_BACKSLASH,
        _ => return None,
    })
}

/// Resto de rueda por debajo de una muesca (120 = una muesca). La tira de
/// scroll manda unas decenas de unidades por paquete: con solo `delta / 120`
/// casi nunca llegaba a una muesca y no hacía nada.
#[derive(Default)]
pub(super) struct WheelAcc(i32);

impl WheelAcc {
    /// Suma `delta` y devuelve las muescas enteras acumuladas (con signo;
    /// 0 si aún no hay una entera).
    pub(super) fn push(&mut self, delta: i32) -> i32 {
        self.0 += delta;
        let notches = self.0 / 120;
        self.0 -= notches * 120;
        notches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn todas_las_teclas_de_texto_tienen_codigo() {
        for c in ASCII_KEYS.chars() {
            assert!(ascii_key(c).is_some(), "{c:?}");
        }
        assert!(ascii_key('ñ').is_none());
        // y todas las teclas del teclado virtual, con códigos distintos
        let codes: BTreeSet<u16> = all_keys()
            .map(|k| evdev_key(k).unwrap_or_else(|| panic!("{k:?} sin tecla")).code())
            .collect();
        assert_eq!(all_keys().count(), 15 + ASCII_KEYS.chars().count());
        assert_eq!(codes.len(), all_keys().count());
        assert!(evdev_key(KeyCode::Char('ñ')).is_none());
    }

    #[test]
    fn el_apuntado_cae_dentro_de_la_pantalla_objetivo() {
        // Escritorio 3968×2232 con la pantalla de juego en (1920,1080) 2048×1152
        // (el caso real: tres monitores en L)
        let t = [1920.0 / 3968.0, 1080.0 / 2232.0, 2048.0 / 3968.0, 1152.0 / 2232.0];
        let (cx, cy) = map_abs(0.5, 0.5, t);
        // centro de la pantalla objetivo = (2944, 1656) del escritorio
        assert_eq!(cx, (2944.0 / 3968.0 * ABS_MAX as f32).round() as i32);
        assert_eq!(cy, (1656.0 / 2232.0 * ABS_MAX as f32).round() as i32);
        // esquinas: nunca se sale de la pantalla objetivo aunque nx se pase
        let (x0, y0) = map_abs(-1.0, -1.0, t);
        assert_eq!((x0, y0), map_abs(0.0, 0.0, t));
        let (x1, y1) = map_abs(2.0, 2.0, t);
        assert_eq!((x1, y1), map_abs(1.0, 1.0, t));
        assert_eq!(x1, ABS_MAX);
        assert_eq!(y1, ABS_MAX);
        // una sola pantalla: identidad
        assert_eq!(map_abs(0.25, 0.75, [0.0, 0.0, 1.0, 1.0]), (8192, 24575));
    }

    #[test]
    fn la_rueda_acumula_muescas_enteras() {
        let mut acc = WheelAcc::default();
        assert_eq!(acc.push(50), 0);
        assert_eq!(acc.push(50), 0);
        assert_eq!(acc.push(20), 1); // 120 justos
        assert_eq!(acc.push(-130), -1); // -130 → una muesca atrás, sobran -10
        assert_eq!(acc.push(10), 0); // -10 + 10 = 0
        assert_eq!(acc.push(240), 2);
    }
}
