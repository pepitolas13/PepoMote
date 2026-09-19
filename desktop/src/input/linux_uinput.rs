use super::linux_common::{all_keys, evdev_key, map_abs, WheelAcc, ABS_MAX};
use super::linux_x11_text::X11Text;
use super::text_plan::{self, TextOp};
use super::{InjectError, Injector, KeyCode, MouseButton, TypeReport};
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
/// - teclado (flechas, atrás/adelante del navegador, Enter/Esc, multimedia, QWERTY)
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
    /// Camino X11 para el texto (Unicode completo por XTEST). Se abre a la
    /// primera que hay que teclear, no al crear el inyector: el puntero
    /// funciona igual sin servidor X.
    x11: Option<X11Text>,
    x11_tried: bool,
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
            x11: None,
            x11_tried: false,
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
        ax_denied: false,
    }
}

impl UinputInjector {
    /// Pulsa y suelta una tecla, con Shift alrededor si hace falta.
    fn tap(&mut self, key: KeyCode, shift: bool) {
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

    /// Texto con todos los caracteres que se puedan.
    ///
    /// En X11 va entero por XTEST (cualquier Unicode). En GNOME o KDE con
    /// Wayland no hay forma: uinput manda keycodes y el keymap lo pone el
    /// compositor, así que se escribe lo que tiene tecla, se pliega a ASCII
    /// lo que tiene sustituto honrado (`ñ`→`n`) y se AVISA de todo ello.
    fn type_text(&mut self, text: &str) -> TypeReport {
        if !self.x11_tried {
            self.x11_tried = true;
            if super::linux_x11_text::available() {
                self.x11 = X11Text::open();
            }
        }
        if let Some(x11) = self.x11.as_mut() {
            match x11.type_text(text) {
                Ok(report) => return report,
                // La conexión X se ha caído: se sigue sin ella
                Err(_) => self.x11 = None,
            }
        }
        let plegar = text_plan::folding_enabled(std::env::var("PEPOMOTE_TEXT_FALLBACK").ok().as_deref());
        let mut report = TypeReport::default();
        for op in text_plan::text_ops(text) {
            match op {
                TextOp::Key(key, shift) => self.tap(key, shift),
                TextOp::Unicode(c) => match plegar.then(|| text_plan::ascii_fold(c)).flatten() {
                    Some(ascii) => {
                        report.fold_char(c, ascii);
                        for op in text_plan::text_ops(&ascii.to_string()) {
                            if let TextOp::Key(key, shift) = op {
                                self.tap(key, shift);
                            }
                        }
                    }
                    None => report.drop_char(c),
                },
            }
        }
        report
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
