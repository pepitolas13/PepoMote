//! Lo que comparten los dos backends de Linux (uinput y Wayland): las tablas
//! de teclas evdev. El mapeo del apuntado absoluto, la lista de teclas fijas
//! y el acumulador de la rueda viven en `common` (también los usa macOS).

use super::KeyCode;
use evdev::Key;

pub(super) use super::common::{map_abs, WheelAcc, ABS_MAX, FIXED_KEYS};

/// Caracteres con tecla propia en el teclado virtual.
pub(super) const ASCII_KEYS: &str = "abcdefghijklmnopqrstuvwxyz0123456789-=.,';/[]`\\";

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
        KeyCode::BrowserBack => Key::KEY_BACK,
        KeyCode::BrowserForward => Key::KEY_FORWARD,
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
        assert_eq!(all_keys().count(), 17 + ASCII_KEYS.chars().count());
        assert_eq!(codes.len(), all_keys().count());
        assert!(evdev_key(KeyCode::Char('ñ')).is_none());
    }
}
