//! Inyección de entrada en el SO. Windows: SendInput. Linux: puntero y
//! teclado virtuales de Wayland (compositores wlroots: Sway, Hyprland,
//! MangoWC, river, labwc, niri…) y, donde el compositor no los ofrece
//! (GNOME, KDE, X11), uinput.

#[cfg(target_os = "linux")]
mod linux_common;
#[cfg(target_os = "linux")]
mod linux_uinput;
#[cfg(target_os = "linux")]
pub mod linux_wayland;
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
    /// Rueda en 1/120 de muesca; positivo = rueda hacia delante (scroll arriba).
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
    /// Nombre del backend para la ventana y el log: "uinput", "Wayland
    /// (puntero virtual)", "SendInput".
    fn name(&self) -> &'static str;
    /// ¿Sigue vivo? En Wayland la conexión puede morir (compositor
    /// reiniciado, error de protocolo): el hilo de telemetría lo tira y lo
    /// vuelve a crear. Los demás backends no mueren.
    fn alive(&mut self) -> bool {
        true
    }
}

/// Fallo al crear el inyector. Los flags solo se activan cuando se intentó
/// uinput y /dev/uinput no se pudo abrir: encienden el aviso y la
/// reparación de la ventana (nunca con el backend Wayland).
#[derive(Debug, Clone)]
pub struct InjectError {
    pub msg: String,
    /// /dev/uinput existe pero niega el acceso (falta la regla udev o la ACL).
    pub uinput_denied: bool,
    /// No existe /dev/uinput (módulo uinput sin cargar).
    pub uinput_missing: bool,
}

impl std::fmt::Display for InjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

/// Texto para el usuario según el error al abrir /dev/uinput (None = otro
/// error, se enseña tal cual).
#[cfg(any(target_os = "linux", test))]
pub fn uinput_hint(kind: std::io::ErrorKind) -> Option<&'static str> {
    match kind {
        std::io::ErrorKind::PermissionDenied => Some(
            "sin permiso para /dev/uinput — pulsa «Reparar ahora» o copia el comando de la ventana",
        ),
        std::io::ErrorKind::NotFound => Some(
            "el módulo uinput no está cargado (no existe /dev/uinput) — «Reparar ahora» lo carga, o: sudo modprobe uinput",
        ),
        _ => None,
    }
}

/// Backend de Linux a intentar.
#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Uinput,
    Wayland,
    /// Wayland si el compositor ofrece el puntero virtual; si no, uinput.
    WaylandThenUinput,
}

#[cfg(any(target_os = "linux", test))]
impl Choice {
    pub fn describe(self) -> &'static str {
        match self {
            Choice::Uinput => "uinput",
            Choice::Wayland => "Wayland (forzado por PEPOMOTE_INJECT)",
            Choice::WaylandThenUinput => "Wayland si el compositor lo ofrece, con uinput de respaldo",
        }
    }
}

/// `PEPOMOTE_INJECT=wayland|uinput` fuerza uno (sin respaldo); si no, con
/// `WAYLAND_DISPLAY` se prueba Wayland y uinput queda de respaldo.
#[cfg(any(target_os = "linux", test))]
pub fn backend_choice(override_: Option<&str>, wayland_display: bool) -> Choice {
    match override_.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("wayland") => Choice::Wayland,
        Some("uinput") => Choice::Uinput,
        _ if wayland_display => Choice::WaylandThenUinput,
        _ => Choice::Uinput,
    }
}

#[cfg(target_os = "linux")]
pub fn wayland_display_set() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty())
}

#[cfg(windows)]
pub fn new_injector() -> Result<Box<dyn Injector>, InjectError> {
    Ok(Box::new(windows_input::WinInjector::new()))
}

#[cfg(target_os = "linux")]
pub fn new_injector() -> Result<Box<dyn Injector>, InjectError> {
    let choice = backend_choice(
        std::env::var("PEPOMOTE_INJECT").ok().as_deref(),
        wayland_display_set(),
    );
    let wayland = || {
        linux_wayland::WaylandInjector::new()
            .map(|i| Box::new(i) as Box<dyn Injector>)
            .map_err(|msg| InjectError {
                msg,
                uinput_denied: false,
                uinput_missing: false,
            })
    };
    let uinput = || linux_uinput::UinputInjector::new().map(|i| Box::new(i) as Box<dyn Injector>);
    match choice {
        Choice::Uinput => uinput(),
        Choice::Wayland => wayland(),
        Choice::WaylandThenUinput => match wayland() {
            Ok(i) => Ok(i),
            // El mensaje de uinput va primero (ahí está la reparación) y el
            // de Wayland detrás, para saber por qué no se usó
            Err(w) => uinput().map_err(|mut u| {
                u.msg = format!("{} (Wayland: {})", u.msg, w.msg);
                u
            }),
        },
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn new_injector() -> Result<Box<dyn Injector>, InjectError> {
    Err(InjectError {
        msg: "plataforma sin soporte de inyección".into(),
        uinput_denied: false,
        uinput_missing: false,
    })
}

/// Líneas de `--diag` sobre la inyección: qué ofrece el compositor, si
/// /dev/uinput se abre, qué backend se elegiría y si se crea de verdad.
#[cfg(target_os = "linux")]
pub fn diag_lines() -> Vec<String> {
    let mut out = Vec::new();
    match linux_wayland::probe() {
        Ok(caps) => out.push(format!("Wayland: {}", caps.describe())),
        Err(e) => out.push(format!("Wayland: {e}")),
    }
    out.push(match linux_uinput::probe() {
        Ok(()) => "uinput: /dev/uinput se abre (OK)".to_owned(),
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {
                "uinput: no existe /dev/uinput (el módulo uinput no está cargado)".to_owned()
            }
            std::io::ErrorKind::PermissionDenied => "uinput: sin permiso para /dev/uinput".to_owned(),
            _ => format!("uinput: {e}"),
        },
    });
    let override_ = std::env::var("PEPOMOTE_INJECT").ok();
    out.push(format!(
        "Elección: {}",
        backend_choice(override_.as_deref(), wayland_display_set()).describe()
    ));
    match new_injector() {
        Ok(i) => out.push(format!("Inyector: {} (creado y liberado sin problema)", i.name())),
        Err(e) => out.push(format!("Inyector: ninguno — {}", e.msg)),
    }
    out
}

#[cfg(windows)]
pub fn diag_lines() -> Vec<String> {
    vec!["SendInput (Windows): disponible".to_owned()]
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn diag_lines() -> Vec<String> {
    vec!["plataforma sin soporte de inyección".to_owned()]
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

    #[test]
    fn la_seleccion_de_backend_respeta_pepomote_inject() {
        assert_eq!(backend_choice(Some("wayland"), false), Choice::Wayland);
        assert_eq!(backend_choice(Some(" UINPUT "), true), Choice::Uinput);
        assert_eq!(backend_choice(None, true), Choice::WaylandThenUinput);
        assert_eq!(backend_choice(None, false), Choice::Uinput);
        assert_eq!(backend_choice(Some("x"), true), Choice::WaylandThenUinput);
        assert_eq!(backend_choice(Some(""), false), Choice::Uinput);
    }

    #[test]
    fn uinput_hint_distingue_enoent_de_eacces() {
        use std::io::ErrorKind;
        assert!(uinput_hint(ErrorKind::NotFound).unwrap().contains("módulo uinput"));
        assert!(uinput_hint(ErrorKind::PermissionDenied).unwrap().contains("sin permiso"));
        assert!(uinput_hint(ErrorKind::Other).is_none());
    }
}
