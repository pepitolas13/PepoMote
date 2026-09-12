//! Inyección SIN permisos en Wayland: puntero virtual (protocolo
//! wlr-virtual-pointer) y teclado virtual (virtual-keyboard-unstable-v1).
//! Los ofrecen los compositores basados en wlroots (Sway, Hyprland, MangoWC,
//! river, labwc, niri, Wayfire…) a cualquier cliente: ni /dev/uinput, ni
//! contraseña, ni regla udev. GNOME y KDE no los anuncian: ahí se usa
//! uinput (`linux_uinput`).
//!
//! Conexión propia (como en `screens.rs`), independiente de la de winit y
//! viva en el hilo de telemetría. Nuestros objetos no reciben eventos:
//! `alive()` drena el socket de vez en cuando y detecta si el compositor
//! cerró la conexión, para que la telemetría vuelva a crear el inyector.

use super::linux_common::{all_keys, evdev_key, map_abs, WheelAcc, ABS_MAX};
use super::{Injector, KeyCode, MouseButton};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsFd;
use std::time::Instant;
use wayland_client::backend::WaylandError;
use wayland_client::protocol::wl_pointer::{Axis, AxisSource, ButtonState};
use wayland_client::protocol::wl_registry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;
use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1;
use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1;

const POINTER_NAME: &str = "Wayland (puntero virtual)";
const POINTER_ONLY_NAME: &str = "Wayland (puntero virtual; sin teclado virtual)";
/// Extent de `motion_absolute`: el mismo rango que el ABS de uinput, así
/// `map_abs` sirve tal cual (wlroots normaliza sobre el layout entero).
const ABS_EXTENT: u32 = ABS_MAX as u32;
/// Máscara del modificador Shift (índice real 0 en cualquier keymap xkb).
pub(super) const SHIFT_MASK: u32 = 1;
/// Píxeles por muesca de rueda, como los manda libinput/wlroots.
const PX_PER_NOTCH: f64 = 15.0;
/// `wl_keyboard.keymap_format.xkb_v1`.
const KEYMAP_XKB_V1: u32 = 1;

fn debug() -> bool {
    std::env::var_os("PEPOMOTE_DEBUG").is_some()
}

/// Globales que nos interesan (versión anunciada por el compositor).
#[derive(Default)]
struct State {
    seat: Option<(WlSeat, u32)>,
    pointer_mgr: Option<(ZwlrVirtualPointerManagerV1, u32)>,
    keyboard_mgr: Option<(ZwpVirtualKeyboardManagerV1, u32)>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_seat" if state.seat.is_none() => {
                    let s = registry.bind::<WlSeat, _, _>(name, version.min(4), qh, ());
                    state.seat = Some((s, version));
                }
                "zwlr_virtual_pointer_manager_v1" => {
                    let m = registry.bind::<ZwlrVirtualPointerManagerV1, _, _>(name, version.min(2), qh, ());
                    state.pointer_mgr = Some((m, version));
                }
                "zwp_virtual_keyboard_manager_v1" => {
                    let m = registry.bind::<ZwpVirtualKeyboardManagerV1, _, _>(name, version.min(1), qh, ());
                    state.keyboard_mgr = Some((m, version));
                }
                _ => {}
            }
        }
    }
}

// Ninguno de estos objetos nos manda eventos que importen (wl_seat anuncia
// capacidades y nombre; el resto nada). Siempre en forma `ignore`: la otra
// forma entra en pánico ante cualquier evento.
delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ignore ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ignore ZwlrVirtualPointerV1);
delegate_noop!(State: ignore ZwpVirtualKeyboardManagerV1);
delegate_noop!(State: ignore ZwpVirtualKeyboardV1);

/// Conexión propia + globales (un roundtrip). La usan `new()` y `probe()`.
fn connect() -> Result<(Connection, EventQueue<State>, State), String> {
    let conn = Connection::connect_to_env()
        .map_err(|e| format!("no se pudo conectar al compositor Wayland ({e})"))?;
    let display = conn.display();
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = display.get_registry(&qh, ());
    let mut state = State::default();
    queue
        .roundtrip(&mut state)
        .map_err(|e| format!("fallo leyendo los globales Wayland ({e})"))?;
    Ok((conn, queue, state))
}

/// Lo que anuncia el compositor (para `--diag`).
pub struct WaylandCaps {
    pub seat: Option<u32>,
    pub virtual_pointer: Option<u32>,
    pub virtual_keyboard: Option<u32>,
}

impl WaylandCaps {
    pub fn describe(&self) -> String {
        let v = |x: Option<u32>| x.map(|v| format!("v{v}")).unwrap_or_else(|| "no".into());
        let mut s = format!(
            "wl_seat {} · zwlr_virtual_pointer_manager_v1 {} · zwp_virtual_keyboard_manager_v1 {}",
            v(self.seat),
            v(self.virtual_pointer),
            v(self.virtual_keyboard)
        );
        if self.virtual_pointer.is_none() {
            s.push_str(" — el compositor no ofrece puntero virtual (GNOME/KDE): irá por uinput");
        } else if self.virtual_keyboard.is_none() {
            s.push_str(" — sin teclado virtual: las teclas no estarán disponibles");
        }
        s
    }
}

/// ¿Qué ofrece el compositor? No deja nada vivo.
pub fn probe() -> Result<WaylandCaps, String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_none_or(|d| d.is_empty()) {
        return Err("sin WAYLAND_DISPLAY (sesión X11 o sin Wayland): irá por uinput".into());
    }
    let (_conn, _queue, state) = connect()?;
    Ok(WaylandCaps {
        seat: state.seat.as_ref().map(|s| s.1),
        virtual_pointer: state.pointer_mgr.as_ref().map(|m| m.1),
        virtual_keyboard: state.keyboard_mgr.as_ref().map(|m| m.1),
    })
}

enum BuildError {
    /// Falló el teclado virtual: se reintenta solo con el puntero.
    Keyboard(String),
    Other(String),
}

pub struct WaylandInjector {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
    pointer: ZwlrVirtualPointerV1,
    keyboard: Option<ZwpVirtualKeyboardV1>,
    /// El fd del keymap sigue abierto mientras viva el inyector.
    _keymap: Option<File>,
    /// Reloj de los timestamps (ms; la base da igual, solo el orden).
    t0: Instant,
    wheel: WheelAcc,
    /// Pantalla de apuntado dentro del escritorio completo ([x0,y0,w,h] 0..1).
    target: [f32; 4],
    shift_down: bool,
    dead: bool,
    name: &'static str,
}

impl WaylandInjector {
    pub fn new() -> Result<Self, String> {
        match Self::build(true) {
            Ok(i) => Ok(i),
            Err(BuildError::Keyboard(e)) => {
                if debug() {
                    eprintln!("[inject] teclado virtual rechazado ({e}): solo puntero");
                }
                Self::build(false).map_err(|e| match e {
                    BuildError::Keyboard(m) | BuildError::Other(m) => m,
                })
            }
            Err(BuildError::Other(e)) => Err(e),
        }
    }

    fn build(with_keyboard: bool) -> Result<Self, BuildError> {
        let (conn, mut queue, mut state) = connect().map_err(BuildError::Other)?;
        let qh = queue.handle();
        let Some((mgr, _)) = state.pointer_mgr.clone() else {
            return Err(BuildError::Other(
                "el compositor no ofrece zwlr_virtual_pointer_manager_v1 (GNOME/KDE no lo tienen)".into(),
            ));
        };
        let seat = state.seat.as_ref().map(|(s, _)| s.clone());
        let pointer = mgr.create_virtual_pointer(seat.as_ref(), &qh, ());
        let mut keyboard = None;
        let mut keymap = None;
        if with_keyboard {
            if let (Some((kmgr, _)), Some(seat)) = (state.keyboard_mgr.as_ref(), seat.as_ref()) {
                let kb = kmgr.create_virtual_keyboard(seat, &qh, ());
                let file = keymap_file().map_err(BuildError::Keyboard)?;
                let bytes = keymap_bytes();
                kb.keymap(KEYMAP_XKB_V1, file.as_fd(), bytes.len() as u32);
                keyboard = Some(kb);
                keymap = Some(file);
            }
        }
        // El roundtrip confirma que el compositor aceptó los objetos: un
        // error de protocolo o de política aflora aquí, no más tarde
        if let Err(e) = queue.roundtrip(&mut state) {
            let msg = format!("el compositor rechazó la inyección ({e})");
            return Err(if keyboard.is_some() {
                BuildError::Keyboard(msg)
            } else {
                BuildError::Other(msg)
            });
        }
        let name = if keyboard.is_some() { POINTER_NAME } else { POINTER_ONLY_NAME };
        Ok(Self {
            conn,
            queue,
            state,
            pointer,
            keyboard,
            _keymap: keymap,
            t0: Instant::now(),
            wheel: WheelAcc::default(),
            target: [0.0, 0.0, 1.0, 1.0],
            shift_down: false,
            dead: false,
            name,
        })
    }

    fn now(&self) -> u32 {
        self.t0.elapsed().as_millis() as u32
    }

    /// Manda lo encolado. Un WouldBlock es pasajero; otro error = conexión
    /// muerta (la telemetría lo verá en `alive()`).
    fn flush(&mut self) {
        match self.conn.flush() {
            Ok(()) => {}
            Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                if !self.dead && debug() {
                    eprintln!("[inject] conexión Wayland perdida: {e}");
                }
                self.dead = true;
            }
        }
    }
}

impl Drop for WaylandInjector {
    fn drop(&mut self) {
        if let Some(kb) = self.keyboard.take() {
            if self.shift_down {
                let t = self.now();
                kb.modifiers(0, 0, 0, 0);
                kb.key(t, evdev::Key::KEY_LEFTSHIFT.code() as u32, 0);
            }
            kb.destroy();
        }
        self.pointer.destroy();
        if let Some((mgr, _)) = self.state.pointer_mgr.take() {
            mgr.destroy();
        }
        let _ = self.conn.flush();
    }
}

/// (valor en px, muescas) de wl_pointer para `notches` muescas nuestras.
/// Nuestro + es rueda hacia delante (scroll arriba, como REL_WHEEL y
/// Windows); en wl_pointer el + es hacia abajo, y una muesca son 15 px.
pub(super) fn wheel_axis(notches: i32) -> (f64, i32) {
    (-PX_PER_NOTCH * notches as f64, -notches)
}

impl Injector for WaylandInjector {
    fn move_rel(&mut self, dx: i32, dy: i32) {
        let t = self.now();
        self.pointer.motion(t, dx as f64, dy as f64);
        self.pointer.frame();
        self.flush();
    }

    fn move_abs(&mut self, nx: f32, ny: f32) {
        let (x, y) = map_abs(nx, ny, self.target);
        let t = self.now();
        self.pointer.motion_absolute(t, x as u32, y as u32, ABS_EXTENT, ABS_EXTENT);
        self.pointer.frame();
        self.flush();
    }

    fn set_screen(&mut self, target: [f32; 4]) {
        self.target = target;
    }

    fn button(&mut self, btn: MouseButton, down: bool) {
        let code = match btn {
            MouseButton::Left => evdev::Key::BTN_LEFT,
            MouseButton::Right => evdev::Key::BTN_RIGHT,
        }
        .code() as u32;
        let state = if down { ButtonState::Pressed } else { ButtonState::Released };
        let t = self.now();
        self.pointer.button(t, code, state);
        self.pointer.frame();
        self.flush();
    }

    fn key(&mut self, key: KeyCode, down: bool) {
        let Some(kb) = self.keyboard.as_ref() else {
            return;
        };
        let Some(k) = evdev_key(key) else {
            return;
        };
        let t = self.now();
        kb.key(t, k.code() as u32, if down { 1 } else { 0 });
        if key == KeyCode::Shift {
            // wlroots no actualiza el estado xkb con las teclas de un teclado
            // virtual: el modificador hay que decirlo aparte
            kb.modifiers(if down { SHIFT_MASK } else { 0 }, 0, 0, 0);
            self.shift_down = down;
        }
        self.flush();
    }

    fn wheel(&mut self, delta: i32) {
        let notches = self.wheel.push(delta);
        if notches == 0 {
            return;
        }
        let (value, discrete) = wheel_axis(notches);
        let t = self.now();
        self.pointer.axis_source(AxisSource::Wheel);
        self.pointer.axis_discrete(t, Axis::VerticalScroll, value, discrete);
        self.pointer.frame();
        self.flush();
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn alive(&mut self) -> bool {
        if self.dead {
            return false;
        }
        // Eventos pendientes (casi nunca los hay) y errores de protocolo
        if self.queue.dispatch_pending(&mut self.state).is_err() {
            self.dead = true;
            return false;
        }
        if let Some(guard) = self.queue.prepare_read() {
            match guard.read() {
                Ok(_) => {}
                Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => self.dead = true,
            }
        }
        if self.queue.dispatch_pending(&mut self.state).is_err() || self.conn.protocol_error().is_some() {
            self.dead = true;
        }
        self.flush();
        !self.dead
    }
}

// ---------------------------------------------------------------------------
// Keymap del teclado virtual
// ---------------------------------------------------------------------------

const LETTERS: [(&str, &str); 26] = [
    ("a", "A"),
    ("b", "B"),
    ("c", "C"),
    ("d", "D"),
    ("e", "E"),
    ("f", "F"),
    ("g", "G"),
    ("h", "H"),
    ("i", "I"),
    ("j", "J"),
    ("k", "K"),
    ("l", "L"),
    ("m", "M"),
    ("n", "N"),
    ("o", "O"),
    ("p", "P"),
    ("q", "Q"),
    ("r", "R"),
    ("s", "S"),
    ("t", "T"),
    ("u", "U"),
    ("v", "V"),
    ("w", "W"),
    ("x", "X"),
    ("y", "Y"),
    ("z", "Z"),
];

/// Keysyms (nivel base y, si lo hay, con Shift) de cada tecla del teclado
/// virtual. Disposición US: es NUESTRO keymap, el compositor lo usa tal
/// cual, así que `text_keys` teclea lo mismo en cualquier idioma.
fn keysyms(key: KeyCode) -> Option<(&'static str, Option<&'static str>)> {
    Some(match key {
        KeyCode::ArrowUp => ("Up", None),
        KeyCode::ArrowDown => ("Down", None),
        KeyCode::ArrowLeft => ("Left", None),
        KeyCode::ArrowRight => ("Right", None),
        KeyCode::Enter => ("Return", None),
        KeyCode::Escape => ("Escape", None),
        KeyCode::VolumeUp => ("XF86AudioRaiseVolume", None),
        KeyCode::VolumeDown => ("XF86AudioLowerVolume", None),
        KeyCode::Mute => ("XF86AudioMute", None),
        KeyCode::PlayPause => ("XF86AudioPlay", None),
        KeyCode::NextTrack => ("XF86AudioNext", None),
        KeyCode::PrevTrack => ("XF86AudioPrev", None),
        KeyCode::Backspace => ("BackSpace", None),
        KeyCode::Space => ("space", None),
        KeyCode::Shift => ("Shift_L", None),
        KeyCode::Char(c) => match c {
            'a'..='z' => {
                let (base, upper) = LETTERS[(c as u8 - b'a') as usize];
                (base, Some(upper))
            }
            '0' => ("0", Some("parenright")),
            '1' => ("1", Some("exclam")),
            '2' => ("2", Some("at")),
            '3' => ("3", Some("numbersign")),
            '4' => ("4", Some("dollar")),
            '5' => ("5", Some("percent")),
            '6' => ("6", Some("asciicircum")),
            '7' => ("7", Some("ampersand")),
            '8' => ("8", Some("asterisk")),
            '9' => ("9", Some("parenleft")),
            '-' => ("minus", Some("underscore")),
            '=' => ("equal", Some("plus")),
            '.' => ("period", Some("greater")),
            ',' => ("comma", Some("less")),
            '\'' => ("apostrophe", Some("quotedbl")),
            ';' => ("semicolon", Some("colon")),
            '/' => ("slash", Some("question")),
            '[' => ("bracketleft", Some("braceleft")),
            ']' => ("bracketright", Some("braceright")),
            '`' => ("grave", Some("asciitilde")),
            '\\' => ("backslash", Some("bar")),
            _ => return None,
        },
    })
}

/// Keymap xkb en texto, con solo nuestras teclas (código xkb = evdev + 8),
/// en el formato que usa wtype: tipos y compatibilidad del sistema
/// (`include "complete"`), símbolos propios.
pub(super) fn keymap_text() -> String {
    let mut keys: BTreeMap<u16, (&'static str, Option<&'static str>)> = BTreeMap::new();
    for k in all_keys() {
        if let (Some(code), Some(syms)) = (evdev_key(k), keysyms(k)) {
            keys.insert(code.code() + 8, syms);
        }
    }
    let max = keys.keys().last().copied().unwrap_or(8);
    let mut s = String::new();
    s.push_str("xkb_keymap {\n\txkb_keycodes \"(unnamed)\" {\n\t\tminimum = 8;\n");
    s.push_str(&format!("\t\tmaximum = {max};\n"));
    for n in keys.keys() {
        s.push_str(&format!("\t\t<K{n}> = {n};\n"));
    }
    s.push_str("\t};\n");
    s.push_str("\txkb_types \"(unnamed)\" { include \"complete\" };\n");
    s.push_str("\txkb_compatibility \"(unnamed)\" { include \"complete\" };\n");
    s.push_str("\txkb_symbols \"(unnamed)\" {\n");
    for (n, (base, shifted)) in &keys {
        match shifted {
            Some(sh) => s.push_str(&format!("\t\tkey <K{n}> {{ [ {base}, {sh} ] }};\n")),
            None => s.push_str(&format!("\t\tkey <K{n}> {{ [ {base} ] }};\n")),
        }
    }
    let shift = evdev::Key::KEY_LEFTSHIFT.code() + 8;
    s.push_str(&format!("\t\tmodifier_map Shift {{ <K{shift}> }};\n"));
    s.push_str("\t};\n};\n");
    s
}

/// El keymap como lo quiere el compositor: texto + NUL final.
pub(super) fn keymap_bytes() -> Vec<u8> {
    let mut b = keymap_text().into_bytes();
    b.push(0);
    b
}

/// Archivo temporal con el keymap, ya desenlazado (solo vive por su fd, que
/// es lo que viaja al compositor). Sin crates nuevas: /dev/shm, $TMPDIR o
/// $XDG_RUNTIME_DIR.
fn keymap_file() -> Result<File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let bytes = keymap_bytes();
    let mut dirs: Vec<std::path::PathBuf> = vec!["/dev/shm".into(), std::env::temp_dir()];
    if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR") {
        dirs.push(d.into());
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let mut last = String::new();
    for dir in dirs {
        let path = dir.join(format!("pepomote-keymap-{}-{nanos}", std::process::id()));
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut f) => {
                let _ = std::fs::remove_file(&path); // el fd sigue vivo; nadie más lo ve
                if let Err(e) = f.write_all(&bytes) {
                    last = format!("{}: {e}", dir.display());
                    continue;
                }
                return Ok(f);
            }
            Err(e) => last = format!("{}: {e}", dir.display()),
        }
    }
    Err(format!("no pude crear el archivo temporal del keymap ({last})"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::text_keys;

    fn keycode(k: KeyCode) -> u16 {
        evdev_key(k).unwrap().code() + 8
    }

    #[test]
    fn el_keymap_lista_cada_tecla_una_vez_con_evdev_mas_8() {
        let text = keymap_text();
        let mut n_keys = 0;
        for k in all_keys() {
            let n = keycode(k);
            let decl = format!("<K{n}> = {n};");
            let sym = format!("key <K{n}> {{");
            assert_eq!(text.matches(&decl).count(), 1, "{k:?}: {decl}");
            assert_eq!(text.matches(&sym).count(), 1, "{k:?}: {sym}");
            n_keys += 1;
        }
        assert_eq!(n_keys, 62);
        assert_eq!(text.matches("\t\tkey <").count(), 62);
        assert!(text.contains("minimum = 8;"));
        let max = all_keys().map(keycode).max().unwrap();
        assert!(text.contains(&format!("maximum = {max};")));
        assert_eq!(keycode(KeyCode::Char('a')), 38); // KEY_A = 30
    }

    #[test]
    fn el_keymap_tiene_los_simbolos_con_shift_que_teclea_text_keys() {
        let text = keymap_text();
        for needle in [
            "[ a, A ]",
            "[ minus, underscore ]",
            "[ equal, plus ]",
            "[ 1, exclam ]",
            "[ slash, question ]",
            "[ semicolon, colon ]",
            "[ apostrophe, quotedbl ]",
            "[ Return ]",
            "[ BackSpace ]",
            "[ space ]",
        ] {
            assert!(text.contains(needle), "{needle}");
        }
        // todo lo que text_keys sabe teclear tiene keysym (y nivel 2 si va con Shift)
        let all: String = (32u8..127).map(|b| b as char).collect();
        for (k, shift) in text_keys(&all) {
            let (_, level2) = keysyms(k).unwrap_or_else(|| panic!("{k:?} sin keysym"));
            if shift {
                assert!(level2.is_some(), "{k:?} con Shift sin nivel 2");
            }
        }
    }

    #[test]
    fn el_keymap_incluye_shift_y_multimedia_y_termina_en_nul() {
        let text = keymap_text();
        assert!(text.contains("[ Shift_L ]"));
        assert!(text.contains(&format!("modifier_map Shift {{ <K{}> }};", keycode(KeyCode::Shift))));
        assert!(text.contains("XF86AudioPlay"));
        assert!(text.contains("XF86AudioRaiseVolume"));
        assert_eq!(text.matches("include \"complete\"").count(), 2);
        assert!(text.ends_with("};\n"));
        let bytes = keymap_bytes();
        assert_eq!(bytes.len(), text.len() + 1);
        assert_eq!(*bytes.last().unwrap(), 0);
    }

    #[test]
    fn la_rueda_invierte_el_signo_para_wl_pointer() {
        assert_eq!(wheel_axis(1), (-15.0, -1));
        assert_eq!(wheel_axis(-2), (30.0, 2));
    }

    #[test]
    fn el_extent_absoluto_coincide_con_map_abs() {
        assert_eq!(map_abs(1.0, 1.0, [0.0, 0.0, 1.0, 1.0]), (ABS_MAX, ABS_MAX));
        assert_eq!(ABS_EXTENT, ABS_MAX as u32);
    }

    #[test]
    fn keysyms_desconocidos_no_generan_tecla() {
        assert!(keysyms(KeyCode::Char('ñ')).is_none());
        assert!(keysyms(KeyCode::Char('A')).is_none());
    }
}
