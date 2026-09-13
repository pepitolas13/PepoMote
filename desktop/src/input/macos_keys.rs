//! Tablas del inyector de macOS, puras (sin FFI: se prueban en cualquier SO):
//! acordes de las teclas fijas, tipos de las teclas multimedia, keycodes
//! QWERTY de los caracteres y la cuenta de clics para el doble clic.

use super::{KeyCode, MouseButton};
use std::time::{Duration, Instant};

/// Un acorde: keycode virtual (kVK_*), si lleva ⌘ y el texto que produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Chord {
    pub code: u16,
    pub command: bool,
    pub text: Option<&'static str>,
}

/// Keycodes virtuales de las teclas sin carácter (Carbon `kVK_*`).
pub(super) const KVK_RETURN: u16 = 0x24;
pub(super) const KVK_DELETE: u16 = 0x33;
const KVK_ESCAPE: u16 = 0x35;
const KVK_SPACE: u16 = 0x31;
const KVK_SHIFT: u16 = 0x38;
const KVK_LEFT: u16 = 0x7B;
const KVK_RIGHT: u16 = 0x7C;
const KVK_DOWN: u16 = 0x7D;
const KVK_UP: u16 = 0x7E;
const KVK_LEFT_BRACKET: u16 = 0x21;
const KVK_RIGHT_BRACKET: u16 = 0x1E;

/// Tecla fija → acorde (None: multimedia o carácter, que van por otro camino).
pub(super) fn fixed_chord(key: KeyCode) -> Option<Chord> {
    let plain = |code| Some(Chord { code, command: false, text: None });
    match key {
        KeyCode::ArrowUp => plain(KVK_UP),
        KeyCode::ArrowDown => plain(KVK_DOWN),
        KeyCode::ArrowLeft => plain(KVK_LEFT),
        KeyCode::ArrowRight => plain(KVK_RIGHT),
        KeyCode::Enter => plain(KVK_RETURN),
        KeyCode::Escape => plain(KVK_ESCAPE),
        KeyCode::Backspace => plain(KVK_DELETE),
        KeyCode::Space => plain(KVK_SPACE),
        KeyCode::Shift => plain(KVK_SHIFT),
        // ⌘[ / ⌘]: atrás y adelante en Safari, Chrome, Firefox y el Finder.
        // El carácter va con el evento para que el atajo case también en un
        // teclado ISO/español (donde esa tecla física no es el corchete)
        KeyCode::BrowserBack => Some(Chord { code: KVK_LEFT_BRACKET, command: true, text: Some("[") }),
        KeyCode::BrowserForward => Some(Chord { code: KVK_RIGHT_BRACKET, command: true, text: Some("]") }),
        KeyCode::VolumeUp
        | KeyCode::VolumeDown
        | KeyCode::Mute
        | KeyCode::PlayPause
        | KeyCode::NextTrack
        | KeyCode::PrevTrack
        | KeyCode::Char(_) => None,
    }
}

/// `NX_KEYTYPE_*` de las teclas multimedia (eventos SystemDefined de AppKit).
pub(super) fn media_key_type(key: KeyCode) -> Option<u32> {
    Some(match key {
        KeyCode::VolumeUp => 0,
        KeyCode::VolumeDown => 1,
        KeyCode::Mute => 7,
        KeyCode::PlayPause => 16,
        KeyCode::NextTrack => 17,
        KeyCode::PrevTrack => 18,
        _ => return None,
    })
}

/// Keycode virtual (`kVK_ANSI_*`) de un carácter en la disposición QWERTY US.
/// El carácter real se manda además como texto del evento, así en un
/// teclado español sale lo que se tecleó y no lo que hay en esa posición.
pub(super) fn ascii_keycode(c: char) -> Option<u16> {
    Some(match c {
        'a' => 0x00,
        's' => 0x01,
        'd' => 0x02,
        'f' => 0x03,
        'h' => 0x04,
        'g' => 0x05,
        'z' => 0x06,
        'x' => 0x07,
        'c' => 0x08,
        'v' => 0x09,
        'b' => 0x0B,
        'q' => 0x0C,
        'w' => 0x0D,
        'e' => 0x0E,
        'r' => 0x0F,
        'y' => 0x10,
        't' => 0x11,
        '1' => 0x12,
        '2' => 0x13,
        '3' => 0x14,
        '4' => 0x15,
        '6' => 0x16,
        '5' => 0x17,
        '=' => 0x18,
        '9' => 0x19,
        '7' => 0x1A,
        '-' => 0x1B,
        '8' => 0x1C,
        '0' => 0x1D,
        ']' => 0x1E,
        'o' => 0x1F,
        'u' => 0x20,
        '[' => 0x21,
        'i' => 0x22,
        'p' => 0x23,
        'l' => 0x25,
        'j' => 0x26,
        '\'' => 0x27,
        'k' => 0x28,
        ';' => 0x29,
        '\\' => 0x2A,
        ',' => 0x2B,
        '/' => 0x2C,
        'n' => 0x2D,
        'm' => 0x2E,
        '.' => 0x2F,
        '`' => 0x32,
        _ => return None,
    })
}

/// Cuenta de clics para el doble y el triple clic. macOS no la deduce de la
/// cadencia de los eventos inyectados: va en el campo `clickState`.
pub(super) struct ClickCounter {
    last: Option<(MouseButton, Instant, (f64, f64))>,
    count: i64,
}

impl ClickCounter {
    /// Dos pulsaciones del mismo botón a menos de medio segundo y de 4 puntos
    /// son un doble clic (los valores de Preferencias por defecto).
    pub(super) const WINDOW: Duration = Duration::from_millis(500);
    pub(super) const RADIUS: f64 = 4.0;

    pub(super) fn new() -> Self {
        Self { last: None, count: 0 }
    }

    /// Al pulsar: 1, 2 o 3 (no pasa de 3).
    pub(super) fn press(&mut self, btn: MouseButton, now: Instant, at: (f64, f64)) -> i64 {
        let again = self.last.is_some_and(|(b, t, p)| {
            b == btn
                && now.duration_since(t) <= Self::WINDOW
                && (p.0 - at.0).abs() <= Self::RADIUS
                && (p.1 - at.1).abs() <= Self::RADIUS
        });
        self.count = if again { (self.count + 1).min(3) } else { 1 };
        self.last = Some((btn, now, at));
        self.count
    }

    /// Al soltar va el mismo valor que al pulsar.
    pub(super) fn current(&self) -> i64 {
        self.count.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::super::common::FIXED_KEYS;
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn todas_las_teclas_ascii_tienen_keycode_distinto() {
        let ascii = "abcdefghijklmnopqrstuvwxyz0123456789-=.,';/[]`\\";
        let codes: BTreeSet<u16> = ascii.chars().map(|c| ascii_keycode(c).unwrap_or_else(|| panic!("{c:?}"))).collect();
        assert_eq!(codes.len(), ascii.chars().count());
        assert!(ascii_keycode('ñ').is_none());
        assert!(ascii_keycode('A').is_none(), "las mayúsculas van con Shift");
    }

    #[test]
    fn cada_tecla_fija_es_acorde_o_multimedia_pero_no_ambos() {
        for k in FIXED_KEYS {
            let chord = fixed_chord(k).is_some();
            let media = media_key_type(k).is_some();
            assert!(chord != media, "{k:?}: acorde={chord} multimedia={media}");
        }
        assert!(fixed_chord(KeyCode::Char('a')).is_none());
        assert!(media_key_type(KeyCode::Char('a')).is_none());
        assert_eq!(media_key_type(KeyCode::VolumeUp), Some(0));
        assert_eq!(media_key_type(KeyCode::PlayPause), Some(16));
        assert_eq!(fixed_chord(KeyCode::Enter), Some(Chord { code: KVK_RETURN, command: false, text: None }));
    }

    #[test]
    fn atras_y_adelante_llevan_comando_y_su_caracter() {
        assert_eq!(fixed_chord(KeyCode::BrowserBack), Some(Chord { code: 0x21, command: true, text: Some("[") }));
        assert_eq!(fixed_chord(KeyCode::BrowserForward), Some(Chord { code: 0x1E, command: true, text: Some("]") }));
        assert!(!fixed_chord(KeyCode::ArrowLeft).unwrap().command);
    }

    #[test]
    fn doble_clic_dentro_de_medio_segundo_y_cuatro_puntos() {
        let t0 = Instant::now();
        let mut c = ClickCounter::new();
        assert_eq!(c.press(MouseButton::Left, t0, (10.0, 10.0)), 1);
        assert_eq!(c.current(), 1);
        assert_eq!(c.press(MouseButton::Left, t0 + Duration::from_millis(200), (12.0, 9.0)), 2);
        assert_eq!(c.press(MouseButton::Left, t0 + Duration::from_millis(400), (12.0, 9.0)), 3);
        assert_eq!(c.press(MouseButton::Left, t0 + Duration::from_millis(600), (12.0, 9.0)), 3, "no pasa de 3");
        // lejos: vuelve a 1
        assert_eq!(c.press(MouseButton::Left, t0 + Duration::from_millis(700), (40.0, 9.0)), 1);
        // otro botón: 1
        assert_eq!(c.press(MouseButton::Right, t0 + Duration::from_millis(800), (40.0, 9.0)), 1);
        // pasado el medio segundo: 1
        assert_eq!(c.press(MouseButton::Right, t0 + Duration::from_millis(1400), (40.0, 9.0)), 1);
    }
}
