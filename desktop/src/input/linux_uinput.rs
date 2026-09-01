use super::{Injector, MouseButton};
use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AttributeSet, EventType, InputEvent, Key, RelativeAxisType};

pub struct UinputInjector {
    dev: VirtualDevice,
}

impl UinputInjector {
    pub fn new() -> Result<Self, String> {
        let mut keys = AttributeSet::<Key>::new();
        keys.insert(Key::BTN_LEFT);
        keys.insert(Key::BTN_RIGHT);

        let mut rel = AttributeSet::<RelativeAxisType>::new();
        rel.insert(RelativeAxisType::REL_X);
        rel.insert(RelativeAxisType::REL_Y);
        rel.insert(RelativeAxisType::REL_WHEEL);

        let dev = VirtualDeviceBuilder::new()
            .map_err(explain_uinput_err)?
            .name("PepoMote Pointer")
            .with_relative_axes(&rel)
            .map_err(explain_uinput_err)?
            .with_keys(&keys)
            .map_err(explain_uinput_err)?
            .build()
            .map_err(explain_uinput_err)?;

        Ok(Self { dev })
    }
}

fn explain_uinput_err(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "sin permiso para /dev/uinput — ejecuta packaging/linux/install.sh y vuelve a iniciar sesión".into()
    } else {
        format!("uinput: {e}")
    }
}

impl Injector for UinputInjector {
    fn move_rel(&mut self, dx: i32, dy: i32) {
        let _ = self.dev.emit(&[
            InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_X.0, dx),
            InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_Y.0, dy),
        ]);
    }

    fn button(&mut self, btn: MouseButton, down: bool) {
        let key = match btn {
            MouseButton::Left => Key::BTN_LEFT,
            MouseButton::Right => Key::BTN_RIGHT,
        };
        let _ = self.dev.emit(&[InputEvent::new(
            EventType::KEY,
            key.code(),
            if down { 1 } else { 0 },
        )]);
    }

    fn wheel(&mut self, delta: i32) {
        // REL_WHEEL va en "muescas": ~120 px de tira táctil = 1 muesca
        let notches = delta / 120;
        if notches != 0 {
            let _ = self.dev.emit(&[InputEvent::new(
                EventType::RELATIVE,
                RelativeAxisType::REL_WHEEL.0,
                notches,
            )]);
        }
    }
}
