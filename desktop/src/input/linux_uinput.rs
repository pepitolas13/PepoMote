use super::{Injector, KeyCode, MouseButton};
use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{
    AbsInfo, AbsoluteAxisType, AttributeSet, EventType, InputEvent, Key, RelativeAxisType,
    UinputAbsSetup,
};

const ABS_MAX: i32 = 32767;

/// Tres dispositivos virtuales:
/// - ratón relativo (REL_X/Y, rueda, botones) — clics y modo relativo
/// - ratón ABSOLUTO (ABS_X/Y + botones, la receta de las tabletas de
///   QEMU/VMware) — posicionamiento absoluto. OJO: un "pen" de tableta
///   (BTN_TOOL_PEN) NO vale: KWin/libinput lo ignoran por completo; el
///   ratón absoluto pasa por el fallback de libinput y funciona en X11 y
///   Wayland en todos los compositores (verificado en KWin 6)
/// - teclado (flechas, Enter/Esc, multimedia)
pub struct UinputInjector {
    mouse: VirtualDevice,
    abs: VirtualDevice,
    keys: VirtualDevice,
    /// Resto de rueda por debajo de una muesca (120 = una muesca).
    wheel_acc: i32,
    /// Pantalla de apuntado dentro del escritorio completo ([x0,y0,w,h] 0..1).
    target: [f32; 4],
}

/// (nx, ny) de la pantalla objetivo → valor ABS del dispositivo, que cubre
/// el escritorio entero.
fn map_abs(nx: f32, ny: f32, target: [f32; 4]) -> (i32, i32) {
    let x = target[0] + nx.clamp(0.0, 1.0) * target[2];
    let y = target[1] + ny.clamp(0.0, 1.0) * target[3];
    (
        (x.clamp(0.0, 1.0) * ABS_MAX as f32).round() as i32,
        (y.clamp(0.0, 1.0) * ABS_MAX as f32).round() as i32,
    )
}

impl UinputInjector {
    pub fn new() -> Result<Self, String> {
        let mut buttons = AttributeSet::<Key>::new();
        buttons.insert(Key::BTN_LEFT);
        buttons.insert(Key::BTN_RIGHT);
        let mut rel = AttributeSet::<RelativeAxisType>::new();
        rel.insert(RelativeAxisType::REL_X);
        rel.insert(RelativeAxisType::REL_Y);
        rel.insert(RelativeAxisType::REL_WHEEL);
        rel.insert(RelativeAxisType::REL_WHEEL_HI_RES);
        let mouse = VirtualDeviceBuilder::new()
            .map_err(explain)?
            .name("PepoMote Pointer")
            .with_relative_axes(&rel)
            .map_err(explain)?
            .with_keys(&buttons)
            .map_err(explain)?
            .build()
            .map_err(explain)?;

        // Sin botones de ratón, udev no lo clasifica como puntero y el
        // compositor lo descarta: van aunque nunca se pulsen desde aquí.
        let abs_info = AbsInfo::new(0, 0, ABS_MAX, 0, 0, 0);
        let abs = VirtualDeviceBuilder::new()
            .map_err(explain)?
            .name("PepoMote Absolute Pointer")
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_X, abs_info))
            .map_err(explain)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_Y, abs_info))
            .map_err(explain)?
            .with_keys(&buttons)
            .map_err(explain)?
            .build()
            .map_err(explain)?;

        let mut kb = AttributeSet::<Key>::new();
        for k in [
            Key::KEY_UP,
            Key::KEY_DOWN,
            Key::KEY_LEFT,
            Key::KEY_RIGHT,
            Key::KEY_ENTER,
            Key::KEY_ESC,
            Key::KEY_VOLUMEUP,
            Key::KEY_VOLUMEDOWN,
            Key::KEY_MUTE,
            Key::KEY_PLAYPAUSE,
            Key::KEY_NEXTSONG,
            Key::KEY_PREVIOUSSONG,
            Key::KEY_BACKSPACE,
            Key::KEY_SPACE,
            Key::KEY_LEFTSHIFT,
        ] {
            kb.insert(k);
        }
        // teclado QWERTY para teclear texto (nombre del jugador en Cemu…)
        for c in ASCII_KEYS.chars() {
            if let Some(k) = ascii_key(c) {
                kb.insert(k);
            }
        }
        let keys = VirtualDeviceBuilder::new()
            .map_err(explain)?
            .name("PepoMote Keys")
            .with_keys(&kb)
            .map_err(explain)?
            .build()
            .map_err(explain)?;

        Ok(Self {
            mouse,
            abs,
            keys,
            wheel_acc: 0,
            target: [0.0, 0.0, 1.0, 1.0],
        })
    }
}

fn explain(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "sin permiso para /dev/uinput — pulsa «Reparar ahora» (o ejecuta packaging/linux/install.sh)".into()
    } else {
        format!("uinput: {e}")
    }
}

impl Injector for UinputInjector {
    fn move_rel(&mut self, dx: i32, dy: i32) {
        #[cfg(debug_assertions)]
        if std::env::var_os("PEPOMOTE_DEBUG").is_some() {
            eprintln!("[rel] dx={dx} dy={dy}");
        }
        let _ = self.mouse.emit(&[
            InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_X.0, dx),
            InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_Y.0, dy),
        ]);
    }

    fn move_abs(&mut self, nx: f32, ny: f32) {
        // El dispositivo cubre todo el escritorio: la posición va dentro
        // del rect de la pantalla de apuntado
        let (x, y) = map_abs(nx, ny, self.target);
        #[cfg(debug_assertions)]
        if std::env::var_os("PEPOMOTE_DEBUG").is_some() {
            eprintln!("[abs] nx={nx:.3} ny={ny:.3} target={:.3?} -> dev({x},{y})", self.target);
        }
        let _ = self.abs.emit(&[
            InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_X.0, x),
            InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_Y.0, y),
        ]);
    }

    fn set_screen(&mut self, target: [f32; 4]) {
        self.target = target;
    }

    fn button(&mut self, btn: MouseButton, down: bool) {
        let key = match btn {
            MouseButton::Left => Key::BTN_LEFT,
            MouseButton::Right => Key::BTN_RIGHT,
        };
        let _ = self.mouse.emit(&[InputEvent::new(
            EventType::KEY,
            key.code(),
            if down { 1 } else { 0 },
        )]);
    }

    fn key(&mut self, key: KeyCode, down: bool) {
        let k = match key {
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
            KeyCode::Char(c) => match ascii_key(c) {
                Some(k) => k,
                None => return,
            },
        };
        let _ = self.keys.emit(&[InputEvent::new(
            EventType::KEY,
            k.code(),
            if down { 1 } else { 0 },
        )]);
    }

    fn wheel(&mut self, delta: i32) {
        // La tira de scroll manda unas decenas de unidades por paquete: con
        // solo `delta / 120` casi nunca llegaba a una muesca y no hacía nada.
        // REL_WHEEL_HI_RES (1/120 de muesca) da scroll suave donde el
        // escritorio lo soporta; las muescas enteras salen del acumulador
        // para el resto.
        let mut events = vec![InputEvent::new(
            EventType::RELATIVE,
            RelativeAxisType::REL_WHEEL_HI_RES.0,
            delta,
        )];
        self.wheel_acc += delta;
        let notches = self.wheel_acc / 120;
        if notches != 0 {
            self.wheel_acc -= notches * 120;
            events.push(InputEvent::new(
                EventType::RELATIVE,
                RelativeAxisType::REL_WHEEL.0,
                notches,
            ));
        }
        let _ = self.mouse.emit(&events);
    }
}

/// Caracteres con tecla propia en el teclado virtual.
const ASCII_KEYS: &str = "abcdefghijklmnopqrstuvwxyz0123456789-=.,';/[]`\\";

/// Tecla evdev de un carácter ASCII en la disposición QWERTY (letras y
/// dígitos coinciden en la española; los signos pueden variar).
fn ascii_key(c: char) -> Option<Key> {
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

    #[test]
    fn todas_las_teclas_de_texto_tienen_codigo() {
        for c in ASCII_KEYS.chars() {
            assert!(ascii_key(c).is_some(), "{c:?}");
        }
        assert!(ascii_key('ñ').is_none());
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
}
