use super::linux_common::{all_keys, evdev_key, map_abs, WheelAcc, ABS_MAX};
use super::{InjectError, Injector, KeyCode, MouseButton};
use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{
    AbsInfo, AbsoluteAxisType, AttributeSet, EventType, InputEvent, Key, RelativeAxisType,
    UinputAbsSetup,
};

/// Tres dispositivos virtuales:
/// - ratón relativo (REL_X/Y, rueda, botones) — clics y modo relativo
/// - ratón ABSOLUTO (ABS_X/Y + botones, la receta de las tabletas de
///   QEMU/VMware) — posicionamiento absoluto. OJO: un "pen" de tableta
///   (BTN_TOOL_PEN) NO vale: KWin/libinput lo ignoran por completo; el
///   ratón absoluto pasa por el fallback de libinput y funciona en X11 y
///   Wayland en todos los compositores (verificado en KWin 6)
/// - teclado (flechas, Enter/Esc, multimedia, QWERTY)
///
/// Necesita poder abrir /dev/uinput (regla udev `uaccess` o ACL): es el
/// respaldo para GNOME, KDE y X11, donde no hay puntero virtual de Wayland.
pub struct UinputInjector {
    mouse: VirtualDevice,
    abs: VirtualDevice,
    keys: VirtualDevice,
    wheel: WheelAcc,
    /// Pantalla de apuntado dentro del escritorio completo ([x0,y0,w,h] 0..1).
    target: [f32; 4],
}

impl UinputInjector {
    pub fn new() -> Result<Self, InjectError> {
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

        // Las mismas teclas que el teclado virtual de Wayland (tabla común)
        let mut kb = AttributeSet::<Key>::new();
        for k in all_keys().filter_map(evdev_key) {
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
            abs,
            keys,
            wheel: WheelAcc::default(),
            target: [0.0, 0.0, 1.0, 1.0],
        })
    }
}

/// ¿Se puede abrir /dev/uinput? El error distingue módulo sin cargar
/// (NotFound) de permiso denegado (PermissionDenied). Para `--diag`.
pub fn probe() -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/uinput")
        .map(|_| ())
}

fn explain(e: std::io::Error) -> InjectError {
    let kind = e.kind();
    InjectError {
        msg: super::uinput_hint(kind)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("uinput: {e}")),
        uinput_denied: kind == std::io::ErrorKind::PermissionDenied,
        uinput_missing: kind == std::io::ErrorKind::NotFound,
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
        let Some(k) = evdev_key(key) else {
            return;
        };
        let _ = self.keys.emit(&[InputEvent::new(
            EventType::KEY,
            k.code(),
            if down { 1 } else { 0 },
        )]);
    }

    fn wheel(&mut self, delta: i32) {
        // REL_WHEEL_HI_RES (1/120 de muesca) da scroll suave donde el
        // escritorio lo soporta; las muescas enteras salen del acumulador
        // para el resto.
        let mut events = vec![InputEvent::new(
            EventType::RELATIVE,
            RelativeAxisType::REL_WHEEL_HI_RES.0,
            delta,
        )];
        let notches = self.wheel.push(delta);
        if notches != 0 {
            events.push(InputEvent::new(
                EventType::RELATIVE,
                RelativeAxisType::REL_WHEEL.0,
                notches,
            ));
        }
        let _ = self.mouse.emit(&events);
    }

    fn name(&self) -> &'static str {
        "uinput"
    }
}
