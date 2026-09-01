use super::{Injector, KeyCode, MouseButton};
use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{
    AbsInfo, AbsoluteAxisType, AttributeSet, EventType, InputEvent, Key, RelativeAxisType,
    UinputAbsSetup,
};

const ABS_MAX: i32 = 32767;

/// Tres dispositivos virtuales:
/// - ratón relativo (REL_X/Y, rueda, botones) — clics y modo relativo
/// - "pen" absoluto (ABS_X/Y + BTN_TOOL_PEN) — posicionamiento absoluto,
///   funciona igual en X11 y Wayland en todos los compositores
/// - teclado (flechas, Enter/Esc, multimedia)
pub struct UinputInjector {
    mouse: VirtualDevice,
    pen: VirtualDevice,
    keys: VirtualDevice,
    pen_active: bool,
    /// Resto de rueda por debajo de una muesca (120 = una muesca).
    wheel_acc: i32,
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

        let mut pen_keys = AttributeSet::<Key>::new();
        pen_keys.insert(Key::BTN_TOOL_PEN);
        let abs_info = AbsInfo::new(0, 0, ABS_MAX, 0, 0, 0);
        let pen = VirtualDeviceBuilder::new()
            .map_err(explain)?
            .name("PepoMote Pen")
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_X, abs_info))
            .map_err(explain)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_Y, abs_info))
            .map_err(explain)?
            .with_keys(&pen_keys)
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
        ] {
            kb.insert(k);
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
            pen,
            keys,
            pen_active: false,
            wheel_acc: 0,
        })
    }
}

fn explain(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "sin permiso para /dev/uinput — ejecuta packaging/linux/install.sh y vuelve a iniciar sesión".into()
    } else {
        format!("uinput: {e}")
    }
}

impl Injector for UinputInjector {
    fn move_rel(&mut self, dx: i32, dy: i32) {
        let _ = self.mouse.emit(&[
            InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_X.0, dx),
            InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_Y.0, dy),
        ]);
    }

    fn move_abs(&mut self, nx: f32, ny: f32) {
        // El espacio del pen ya cubre todo el escritorio: recorte a 0..1
        let x = (nx.clamp(0.0, 1.0) * ABS_MAX as f32).round() as i32;
        let y = (ny.clamp(0.0, 1.0) * ABS_MAX as f32).round() as i32;
        if !self.pen_active {
            // El pen entra "en rango": hover, sin clic
            let _ = self.pen.emit(&[InputEvent::new(
                EventType::KEY,
                Key::BTN_TOOL_PEN.code(),
                1,
            )]);
            self.pen_active = true;
        }
        let _ = self.pen.emit(&[
            InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_X.0, x),
            InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_Y.0, y),
        ]);
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
