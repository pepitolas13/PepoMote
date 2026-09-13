//! Inyección en macOS con CGEvent (Quartz Event Services): el sistema ve un
//! ratón y un teclado de verdad. Exige el permiso de Accesibilidad: sin él
//! `new_injector` lo dice (la ventana enseña cómo darlo) y, en cuanto se
//! concede, el reintento de telemetría crea el inyector sin reiniciar; si se
//! revoca, `alive()` lo tira y todo vuelve a esperar.

use super::common::{map_norm, norm_in, WheelAcc};
use super::macos_keys::{ascii_keycode, fixed_chord, media_key_type, ClickCounter, KVK_DELETE, KVK_RETURN};
use super::{InjectError, Injector, KeyCode, MouseButton};
use core_graphics::display::CGDisplay;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::NSPoint;
use std::time::{Duration, Instant};

/// Escritorio entero (todas las pantallas) en puntos, con el origen en la
/// esquina superior izquierda de la principal: el sistema de coordenadas de
/// los CGEvent.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Desktop {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Desktop {
    fn detect() -> Desktop {
        let main = CGDisplay::main().bounds();
        let mut d = Desktop {
            x0: main.origin.x,
            y0: main.origin.y,
            x1: main.origin.x + main.size.width,
            y1: main.origin.y + main.size.height,
        };
        if let Ok(ids) = CGDisplay::active_displays() {
            for id in ids {
                let b = CGDisplay::new(id).bounds();
                d.x0 = d.x0.min(b.origin.x);
                d.y0 = d.y0.min(b.origin.y);
                d.x1 = d.x1.max(b.origin.x + b.size.width);
                d.y1 = d.y1.max(b.origin.y + b.size.height);
            }
        }
        d
    }

    fn w(&self) -> f64 {
        (self.x1 - self.x0).max(1.0)
    }

    fn h(&self) -> f64 {
        (self.y1 - self.y0).max(1.0)
    }

    /// (x, y) 0..1 del escritorio → punto en puntos.
    fn point(&self, x: f32, y: f32) -> CGPoint {
        CGPoint::new(self.x0 + x as f64 * self.w(), self.y0 + y as f64 * self.h())
    }

    /// Punto → (x, y) 0..1 del escritorio.
    fn norm(&self, p: CGPoint) -> (f32, f32) {
        (((p.x - self.x0) / self.w()) as f32, ((p.y - self.y0) / self.h()) as f32)
    }

    fn clamp(&self, p: CGPoint) -> CGPoint {
        CGPoint::new(p.x.clamp(self.x0, self.x1 - 1.0), p.y.clamp(self.y0, self.y1 - 1.0))
    }
}

pub struct MacInjector {
    source: CGEventSource,
    wheel: WheelAcc,
    /// Rect 0..1 de la pantalla de apuntado dentro del escritorio (`set_screen`).
    target: [f32; 4],
    /// Botones (izquierdo, derecho) que el SO ve pulsados: moviendo con uno
    /// pulsado el evento es «arrastre», no «movimiento».
    held: [bool; 2],
    clicks: ClickCounter,
    desktop: Desktop,
    desktop_at: Instant,
}

impl MacInjector {
    pub fn new() -> Result<Self, InjectError> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| InjectError {
            msg: "no se pudo crear la fuente de eventos (CGEventSource)".into(),
            uinput_denied: false,
            uinput_missing: false,
            ax_denied: false,
        })?;
        Ok(Self {
            source,
            wheel: WheelAcc::default(),
            target: [0.0, 0.0, 1.0, 1.0],
            held: [false; 2],
            clicks: ClickCounter::new(),
            desktop: Desktop::detect(),
            desktop_at: Instant::now(),
        })
    }

    /// Monitores que van y vienen: se relee cada 2 s (barato).
    fn desktop(&mut self) -> Desktop {
        if self.desktop_at.elapsed() >= Duration::from_secs(2) {
            self.desktop = Desktop::detect();
            self.desktop_at = Instant::now();
        }
        self.desktop
    }

    fn location(&self) -> CGPoint {
        CGEvent::new(self.source.clone())
            .map(|e| e.location())
            .unwrap_or(CGPoint::new(0.0, 0.0))
    }

    fn move_type(&self) -> (CGEventType, CGMouseButton) {
        if self.held[0] {
            (CGEventType::LeftMouseDragged, CGMouseButton::Left)
        } else if self.held[1] {
            (CGEventType::RightMouseDragged, CGMouseButton::Right)
        } else {
            (CGEventType::MouseMoved, CGMouseButton::Left)
        }
    }

    /// Mueve el cursor a `to` con los deltas que leen los juegos (mouse look).
    fn post_move(&mut self, to: CGPoint, dx: i64, dy: i64) {
        let (ty, btn) = self.move_type();
        if let Ok(ev) = CGEvent::new_mouse_event(self.source.clone(), ty, to, btn) {
            ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, dx);
            ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y, dy);
            ev.post(CGEventTapLocation::HID);
        }
    }

    fn post_key(&self, code: u16, down: bool, command: bool, text: Option<&str>) {
        if let Ok(ev) = CGEvent::new_keyboard_event(self.source.clone(), code, down) {
            if command {
                ev.set_flags(CGEventFlags::CGEventFlagCommand);
            }
            if let Some(t) = text {
                ev.set_string(t);
            }
            ev.post(CGEventTapLocation::HID);
        }
    }

    /// Teclas multimedia: no son teclas de teclado sino eventos
    /// «SystemDefined» de AppKit (subtipo 8, `NX_KEYTYPE_*` en data1).
    fn post_media(key_type: u32, down: bool) {
        let flags = NSEventModifierFlags(if down { 0xa00 } else { 0xb00 });
        let data1 = ((key_type as isize) << 16) | ((if down { 0xA } else { 0xB }) << 8);
        unsafe {
            let Some(ev) = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                NSEventType::SystemDefined,
                NSPoint::new(0.0, 0.0),
                flags,
                0.0,
                0,
                None,
                8,
                data1,
                -1,
            ) else {
                return;
            };
            let cg = ev.eventRef() as core_graphics::sys::CGEventRef;
            if cg.is_null() {
                return;
            }
            // El NSEvent es dueño de su CGEvent: retención propia antes de
            // envolverlo (el drop la suelta)
            core_foundation::base::CFRetain(cg as *const _);
            CGEvent::from_ptr(cg).post(CGEventTapLocation::HID);
        }
    }
}

impl Injector for MacInjector {
    fn name(&self) -> &'static str {
        "CGEvent"
    }

    fn move_rel(&mut self, dx: i32, dy: i32) {
        let d = self.desktop();
        let cur = self.location();
        let to = d.clamp(CGPoint::new(cur.x + dx as f64, cur.y + dy as f64));
        self.post_move(to, dx as i64, dy as i64);
    }

    fn move_abs(&mut self, nx: f32, ny: f32) {
        let d = self.desktop();
        let (x, y) = map_norm(nx, ny, self.target);
        let to = d.clamp(d.point(x, y));
        let cur = self.location();
        self.post_move(to, (to.x - cur.x).round() as i64, (to.y - cur.y).round() as i64);
    }

    fn button(&mut self, btn: MouseButton, down: bool) {
        let at = self.location();
        let idx = match btn {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
        };
        // macOS no deduce el doble clic de la cadencia: va en clickState
        let clicks = if down { self.clicks.press(btn, Instant::now(), (at.x, at.y)) } else { self.clicks.current() };
        self.held[idx] = down;
        let (ty, b) = match (btn, down) {
            (MouseButton::Left, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
            (MouseButton::Left, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
            (MouseButton::Right, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
            (MouseButton::Right, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
        };
        if let Ok(ev) = CGEvent::new_mouse_event(self.source.clone(), ty, at, b) {
            ev.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, clicks);
            ev.post(CGEventTapLocation::HID);
        }
    }

    fn key(&mut self, key: KeyCode, down: bool) {
        if let Some(t) = media_key_type(key) {
            Self::post_media(t, down);
            return;
        }
        if let Some(ch) = fixed_chord(key) {
            self.post_key(ch.code, down, ch.command, ch.text);
            return;
        }
        if let KeyCode::Char(c) = key {
            // keycode QWERTY + el carácter: en un teclado español sale lo que
            // se tecleó, no lo que hay en esa posición
            if let Some(code) = ascii_keycode(c) {
                self.post_key(code, down, false, Some(&c.to_string()));
            }
        }
    }

    /// Texto tal cual (cualquier carácter, como texto del evento) a la app
    /// con el foco.
    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            match c {
                '\n' => {
                    self.post_key(KVK_RETURN, true, false, None);
                    self.post_key(KVK_RETURN, false, false, None);
                }
                '\r' => {}
                '\u{8}' | '\u{7f}' => {
                    self.post_key(KVK_DELETE, true, false, None);
                    self.post_key(KVK_DELETE, false, false, None);
                }
                _ => {
                    let code = ascii_keycode(c.to_ascii_lowercase()).unwrap_or(0);
                    let s = c.to_string();
                    self.post_key(code, true, false, Some(&s));
                    self.post_key(code, false, false, Some(&s));
                }
            }
        }
    }

    fn wheel(&mut self, delta: i32) {
        let notches = self.wheel.push(delta);
        if notches == 0 {
            return;
        }
        if let Ok(ev) = CGEvent::new_scroll_event(self.source.clone(), ScrollEventUnit::LINE, 1, notches, 0, 0) {
            ev.post(CGEventTapLocation::HID);
        }
    }

    /// En unidades de la pantalla de apuntado (las mismas que `move_abs`).
    fn cursor_pos(&mut self) -> Option<(f32, f32)> {
        let d = self.desktop();
        let (x, y) = d.norm(self.location());
        Some(norm_in(self.target, x, y))
    }

    /// El escritorio entero en unidades de la pantalla de apuntado.
    fn cursor_bounds(&mut self) -> Option<(f32, f32, f32, f32)> {
        let (x0, y0) = norm_in(self.target, 0.0, 0.0);
        let (x1, y1) = norm_in(self.target, 1.0, 1.0);
        Some((x0, y0, x1, y1))
    }

    fn set_screen(&mut self, target: [f32; 4]) {
        self.target = target;
    }

    /// Si el usuario quita el permiso de Accesibilidad, el inyector muere y
    /// telemetría lo vuelve a crear cuando lo devuelva.
    fn alive(&mut self) -> bool {
        crate::macos::ax_trusted()
    }
}
