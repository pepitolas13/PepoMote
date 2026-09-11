//! Inyección de entrada en el SO. Windows: SendInput. Linux: uinput.

#[cfg(target_os = "linux")]
mod linux_uinput;
#[cfg(windows)]
mod windows_input;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyCode {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Enter,
    Escape,
    VolumeUp,
    VolumeDown,
    Mute,
    PlayPause,
    NextTrack,
    PrevTrack,
    Backspace,
    Space,
    Shift,
    /// Tecla de un carácter ASCII (letra minúscula, dígito o signo) en la
    /// posición de un teclado QWERTY; las mayúsculas van con `Shift`.
    Char(char),
}

/// Texto → pulsaciones (tecla, con Shift). Lo que no tiene tecla ASCII se
/// salta (Windows teclea cualquier carácter por otro camino).
pub fn text_keys(text: &str) -> Vec<(KeyCode, bool)> {
    let mut out = Vec::new();
    for c in text.chars() {
        match c {
            '\n' => out.push((KeyCode::Enter, false)),
            '\r' => {}
            '\u{8}' | '\u{7f}' => out.push((KeyCode::Backspace, false)),
            ' ' => out.push((KeyCode::Space, false)),
            'a'..='z' | '0'..='9' => out.push((KeyCode::Char(c), false)),
            'A'..='Z' => out.push((KeyCode::Char(c.to_ascii_lowercase()), true)),
            '-' | '=' | '.' | ',' | '\'' | ';' | '/' | '[' | ']' | '`' | '\\' => out.push((KeyCode::Char(c), false)),
            '_' => out.push((KeyCode::Char('-'), true)),
            '+' => out.push((KeyCode::Char('='), true)),
            '!' => out.push((KeyCode::Char('1'), true)),
            '?' => out.push((KeyCode::Char('/'), true)),
            ':' => out.push((KeyCode::Char(';'), true)),
            '"' => out.push((KeyCode::Char('\''), true)),
            _ => {}
        }
    }
    out
}

pub trait Injector: Send {
    fn move_rel(&mut self, dx: i32, dy: i32);
    /// Coordenadas normalizadas 0..1 sobre la pantalla primaria.
    fn move_abs(&mut self, nx: f32, ny: f32);
    fn button(&mut self, btn: MouseButton, down: bool);
    fn key(&mut self, key: KeyCode, down: bool);
    fn wheel(&mut self, delta: i32);
    /// Teclea un texto en la ventana con el foco (Intro = `\n`, borrar = `\u{8}`).
    fn type_text(&mut self, text: &str) {
        for (key, shift) in text_keys(text) {
            if shift {
                self.key(KeyCode::Shift, true);
            }
            self.key(key, true);
            self.key(key, false);
            if shift {
                self.key(KeyCode::Shift, false);
            }
        }
    }
    /// Posición actual del cursor, normalizada a la pantalla primaria
    /// (puede salirse de 0..1 con varios monitores). None si el SO no
    /// permite leerla (Wayland).
    fn cursor_pos(&mut self) -> Option<(f32, f32)> {
        None
    }
    /// Linux multi-monitor: rect [x0, y0, w, h] (0..1) de la pantalla de
    /// apuntado dentro del escritorio completo que cubre el dispositivo
    /// absoluto. Windows ya apunta a la primaria: no hace nada.
    fn set_screen(&mut self, _target: [f32; 4]) {}
}

#[cfg(windows)]
pub fn new_injector() -> Result<Box<dyn Injector>, String> {
    Ok(Box::new(windows_input::WinInjector::new()))
}

#[cfg(target_os = "linux")]
pub fn new_injector() -> Result<Box<dyn Injector>, String> {
    linux_uinput::UinputInjector::new().map(|i| Box::new(i) as Box<dyn Injector>)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn new_injector() -> Result<Box<dyn Injector>, String> {
    Err("plataforma sin soporte de inyección".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texto_a_teclas() {
        assert_eq!(
            text_keys("Link 2\n"),
            vec![
                (KeyCode::Char('l'), true),
                (KeyCode::Char('i'), false),
                (KeyCode::Char('n'), false),
                (KeyCode::Char('k'), false),
                (KeyCode::Space, false),
                (KeyCode::Char('2'), false),
                (KeyCode::Enter, false),
            ]
        );
        assert_eq!(text_keys("\u{8}"), vec![(KeyCode::Backspace, false)]);
        assert_eq!(text_keys("ñ\r"), vec![], "sin tecla ASCII: se salta");
        assert_eq!(text_keys("!?"), vec![(KeyCode::Char('1'), true), (KeyCode::Char('/'), true)]);
    }
}
