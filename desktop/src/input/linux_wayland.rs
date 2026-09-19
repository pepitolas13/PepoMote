//! Inyección SIN permisos en Wayland: puntero virtual (protocolo
//! wlr-virtual-pointer) y teclado virtual (virtual-keyboard-unstable-v1).
//! Los ofrecen los compositores basados en wlroots (Sway, Hyprland, MangoWC,
//! river, labwc, niri, Wayfire…) a cualquier cliente: ni /dev/uinput, ni
//! contraseña, ni regla udev. GNOME y KDE no los anuncian: ahí se usa
//! uinput (`linux_uinput`).
//!
//! El keymap del teclado virtual es NUESTRO, así que para escribir un
//! carácter sin tecla (ñ, tildes, €, emoji) basta con darle una tecla de
//! repuesto y volver a subir el keymap: es lo que hace `wtype`, y llega a
//! cualquier cliente (nativo, Xwayland, SDL, juegos), a diferencia del
//! protocolo de método de entrada.
//!
//! Conexión propia (como en `screens.rs`), independiente de la de winit y
//! viva en el hilo de telemetría. Nuestros objetos no reciben eventos:
//! `alive()` drena el socket de vez en cuando y detecta si el compositor
//! cerró la conexión, para que la telemetría vuelva a crear el inyector.

use super::linux_common::{all_keys, evdev_key, map_abs, WheelAcc, ABS_MAX};
use super::text_plan::{self, render_keymap, unicode_keysym, TextOp};
use super::{Injector, KeyCode, MouseButton, TypeReport};
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
use crate::tr;

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
        .map_err(|e| tr!("inj.wayland_connect", e))?;
    let display = conn.display();
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = display.get_registry(&qh, ());
    let mut state = State::default();
    queue
        .roundtrip(&mut state)
        .map_err(|e| tr!("inj.wayland_globals", e))?;
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
    /// Reparto vivo carácter → tecla de repuesto (código xkb) que el
    /// compositor tiene ahora mismo. Se conserva entre mensajes: el segundo
    /// «ñ» seguido no vuelve a subir keymap.
    unicode: BTreeMap<char, u16>,
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
                let (file, len) = keymap_file_with(&BTreeMap::new()).map_err(BuildError::Keyboard)?;
                kb.keymap(KEYMAP_XKB_V1, file.as_fd(), len as u32);
                keyboard = Some(kb);
                keymap = Some(file);
            }
        }
        // El roundtrip confirma que el compositor aceptó los objetos: un
        // error de protocolo o de política aflora aquí, no más tarde
        if let Err(e) = queue.roundtrip(&mut state) {
            let msg = tr!("inj.wayland_rejected", e);
            return Err(if keyboard.is_some() {
                BuildError::Keyboard(msg)
            } else {
                BuildError::Other(msg)
            });
        }
        let name = if keyboard.is_some() { tr!("inj.wayland") } else { tr!("inj.wayland_no_kb") };
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
            unicode: BTreeMap::new(),
            dead: false,
            name,
        })
    }

    fn now(&self) -> u32 {
        self.t0.elapsed().as_millis() as u32
    }

    /// Sube al compositor un reparto nuevo de teclas de repuesto para los
    /// caracteres sin tecla propia. Devuelve si pudo; si no, el tramo teclea
    /// lo que ya tenía tecla y el resto se avisa.
    fn apply_keymap(&mut self, extra: &BTreeMap<char, u16>) -> bool {
        let Some(kb) = self.keyboard.clone() else {
            return false;
        };
        // Nada latcheado al cambiar de mapa: el Shift de antes ya no vale
        if self.shift_down {
            let t = self.now();
            if let Some(k) = evdev_key(KeyCode::Shift) {
                kb.key(t, k.code() as u32, 0);
            }
            kb.modifiers(0, 0, 0, 0);
            self.shift_down = false;
        }
        let (file, len) = match keymap_file_with(extra) {
            Ok(v) => v,
            Err(e) => {
                if debug() {
                    eprintln!("[wayland] keymap Unicode: {e}");
                }
                return false;
            }
        };
        kb.keymap(KEYMAP_XKB_V1, file.as_fd(), len as u32);
        // El roundtrip es la garantía de verdad: keymap y teclas van por el
        // mismo socket y en orden, así que el compositor lo ha compilado
        // antes de recibir la primera tecla. Un error de protocolo aflora aquí.
        if self.queue.roundtrip(&mut self.state).is_err() {
            self.dead = true;
            return false;
        }
        // El fd viejo se cierra DESPUÉS del roundtrip
        self._keymap = Some(file);
        // wlroots no reinicia el estado xkb al cambiar de mapa: lo decimos
        kb.modifiers(0, 0, 0, 0);
        let settle = settle_ms();
        if settle > 0 {
            std::thread::sleep(std::time::Duration::from_millis(settle));
        }
        true
    }

    /// Pulsa y suelta una tecla de repuesto (código xkb = evdev + 8). Va en
    /// un solo nivel, así que sale igual haya o no Shift.
    fn press_spare(&mut self, xkb: u16) {
        let t = self.now();
        let Some(kb) = self.keyboard.as_ref() else {
            return;
        };
        let code = xkb.saturating_sub(8) as u32;
        kb.key(t, code, 1);
        kb.key(t, code, 0);
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

    /// Cualquier carácter: los que tienen tecla, por su tecla; los demás, por
    /// una tecla de repuesto con su keysym Unicode. Solo se sube keymap
    /// cuando aparece un carácter que no estaba: el texto ASCII no lo toca.
    fn type_text(&mut self, text: &str) -> TypeReport {
        let mut report = TypeReport::default();
        if self.keyboard.is_none() {
            // Compositor con puntero virtual pero sin teclado virtual: antes
            // no tecleaba nada y tampoco lo decía. Intro y retroceso tampoco
            // tienen keysym Unicode, pero siguen siendo acciones de teclado.
            report.rejected_by_os = !text_plan::text_ops(text).is_empty();
            for c in text.chars() {
                if unicode_keysym(c).is_some() {
                    report.drop_char(c);
                }
            }
            return report;
        }
        let ops = text_plan::text_ops(text);
        for chunk in text_plan::plan_text(&ops, &self.unicode) {
            if let Some(map) = chunk.keymap {
                if self.apply_keymap(&map) {
                    self.unicode = map;
                }
            }
            for op in chunk.ops {
                match op {
                    TextOp::Key(key, shift) => {
                        if shift {
                            self.key(KeyCode::Shift, true);
                        }
                        self.key(key, true);
                        self.key(key, false);
                        if shift {
                            self.key(KeyCode::Shift, false);
                        }
                    }
                    TextOp::Unicode(c) => match self.unicode.get(&c).copied() {
                        Some(xkb) => self.press_spare(xkb),
                        // El keymap no se pudo subir: se dice, no se tira
                        None => report.drop_char(c),
                    },
                }
            }
        }
        self.flush();
        report
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
        KeyCode::BrowserBack => ("XF86Back", None),
        KeyCode::BrowserForward => ("XF86Forward", None),
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
    keymap_text_with(&BTreeMap::new())
}

/// Espera tras cambiar el keymap (ms). Es defensiva, no teórica: el orden lo
/// garantiza el socket, pero Chromium y Electron recompilan el keymap en otro
/// hilo y se les conoce por comerse la primera tecla. Solo se paga una vez
/// por mensaje, y únicamente si el mapa cambió. `PEPOMOTE_TYPE_SETTLE_MS=0`
/// la quita.
fn settle_ms() -> u64 {
    std::env::var("PEPOMOTE_TYPE_SETTLE_MS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(20)
}

/// El keymap de siempre MÁS una tecla de repuesto por carácter de `extra`.
/// Con `extra` vacío devuelve exactamente el de siempre, byte a byte.
pub(super) fn keymap_text_with(extra: &BTreeMap<char, u16>) -> String {
    let mut keys: BTreeMap<u16, (String, Option<String>)> = BTreeMap::new();
    for k in all_keys() {
        if let (Some(code), Some((base, shifted))) = (evdev_key(k), keysyms(k)) {
            keys.insert(code.code() + 8, (base.to_owned(), shifted.map(str::to_owned)));
        }
    }
    // Un solo nivel: con `include "complete"` xkb le da el tipo ONE_LEVEL,
    // así que el carácter sale igual aunque haya un modificador pulsado
    for (c, code) in extra {
        if let Some(sym) = unicode_keysym(*c) {
            keys.insert(*code, (sym, None));
        }
    }
    render_keymap(&keys, evdev::Key::KEY_LEFTSHIFT.code() + 8)
}

/// El keymap como lo quiere el compositor: texto + NUL final. Lo real lo
/// escribe `keymap_file_with`; esto fija el contrato en el test.
#[cfg(test)]
fn keymap_bytes() -> Vec<u8> {
    let mut b = keymap_text().into_bytes();
    b.push(0);
    b
}

/// Archivo temporal con el keymap (y su longitud en bytes), ya desenlazado
/// (solo vive por su fd, que es lo que viaja al compositor). Sin crates
/// nuevas: /dev/shm, $TMPDIR o $XDG_RUNTIME_DIR.
fn keymap_file_with(extra: &BTreeMap<char, u16>) -> Result<(File, usize), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut bytes = keymap_text_with(extra).into_bytes();
    bytes.push(0);
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
                return Ok((f, bytes.len()));
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
        assert_eq!(n_keys, 64);
        assert_eq!(text.matches("\t\tkey <").count(), 64);
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
            // atrás/adelante del navegador (un keysym mal escrito queda en NoSymbol sin error)
            "[ XF86Back ]",
            "[ XF86Forward ]",
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
    fn el_keymap_sin_unicode_es_exactamente_el_de_siempre() {
        assert_eq!(keymap_text_with(&BTreeMap::new()), keymap_text());
    }

    #[test]
    fn el_keymap_con_unicode_declara_las_teclas_de_repuesto() {
        use crate::input::text_plan::spare_xkb;
        let extra = BTreeMap::from([('ñ', spare_xkb(0)), ('😀', spare_xkb(1)), ('€', spare_xkb(2))]);
        let text = keymap_text_with(&extra);
        assert!(text.contains("<K208> = 208;"), "{text}");
        assert!(text.contains("key <K208> { [ U00F1 ] };"), "{text}");
        assert!(text.contains("key <K209> { [ U1F600 ] };"), "{text}");
        assert!(text.contains("key <K210> { [ U20AC ] };"), "{text}");
        assert!(text.contains("maximum = 210;"), "{text}");
        // y las 64 de siempre siguen enteras
        assert_eq!(text.matches("\t\tkey <").count(), 64 + 3);
        assert!(text.contains("[ a, A ]"));
        assert!(text.contains("[ Return ]"));
        assert!(text.contains(&format!("modifier_map Shift {{ <K{}> }};", keycode(KeyCode::Shift))));
    }

    #[test]
    fn la_espera_tras_cambiar_el_keymap_se_puede_ajustar() {
        // por defecto hay espera; el valor exacto lo decide el entorno
        assert!(settle_ms() <= 200);
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
