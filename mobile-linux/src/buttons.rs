//! Estado de botones compartido entre la UI (dedo) y el hilo de paquetes
//! (cable). Misma regla que en Android (PressLatch): cada pulsación dura al
//! menos 70 ms EN EL CABLE (~18 paquetes), porque un toque de 20-40 ms se lo
//! traga una ráfaga de pérdida Wi-Fi o el muestreo a 60 Hz del juego. El
//! flanco de bajada sale al instante; solo se retrasa la suelta de los toques
//! cortos; mantener pulsado no cambia nada; dos toques seguidos no se funden
//! (hueco de 10 ms). Aquí se evalúa de forma perezosa al construir cada
//! paquete: sin temporizadores.
//!
//! Lo analógico (sticks, pantalla táctil del GamePad) no lleva latch: va tal
//! cual en cada paquete. Y dos atómicos dicen al hilo de paquetes cómo
//! actuar: si el móvil es GamePad/Pro de Wii U (80 bytes, sensores
//! remapeados) y cómo está girado apaisado.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI8, AtomicU16, AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MIN_PRESS: Duration = Duration::from_millis(70);
const MIN_GAP: Duration = Duration::from_millis(10);

#[derive(Clone, Copy)]
struct Latch {
    phys_down: bool,
    /// Instante en que el cable baja (tras el hueco, si lo hubo).
    wire_down_at: Option<Instant>,
}

pub struct Buttons {
    latches: Mutex<HashMap<u32, Latch>>,
    recenter: AtomicU32,
    scroll: AtomicI32,
    /// Stick del Nunchuk o izquierdo del GamePad (+X derecha, +Y arriba,
    /// −127..127); 0,0 en reposo.
    stick_x: AtomicI8,
    stick_y: AtomicI8,
    /// Stick derecho del GamePad de Wii U, misma convención.
    stick_rx: AtomicI8,
    stick_ry: AtomicI8,
    /// Pantalla táctil del GamePad: fracción 0..65535 (origen arriba a la
    /// izquierda) y si hay un dedo apoyado.
    touch_x: AtomicU16,
    touch_y: AtomicU16,
    touch_down: AtomicBool,
    /// El móvil actúa como GamePad/Pro de Wii U: el hilo de paquetes manda
    /// 80 bytes con el bloque de extensión y remapea los sensores.
    gamepad: AtomicBool,
    /// Giro del móvil apaisado (`frame::Rotation`): 0 = borde superior a la
    /// izquierda, 1 = a la derecha.
    rotation: AtomicU8,
}

impl Default for Buttons {
    fn default() -> Self {
        Self::new()
    }
}

impl Buttons {
    pub fn new() -> Self {
        Self {
            latches: Mutex::new(HashMap::new()),
            recenter: AtomicU32::new(0),
            scroll: AtomicI32::new(0),
            stick_x: AtomicI8::new(0),
            stick_y: AtomicI8::new(0),
            stick_rx: AtomicI8::new(0),
            stick_ry: AtomicI8::new(0),
            touch_x: AtomicU16::new(0),
            touch_y: AtomicU16::new(0),
            touch_down: AtomicBool::new(false),
            gamepad: AtomicBool::new(false),
            rotation: AtomicU8::new(0),
        }
    }

    /// Pulsación física (dedo): el flanco de bajada sale ya.
    pub fn set(&self, bit: u32, down: bool) {
        self.set_at(bit, down, Instant::now());
    }

    pub fn set_at(&self, bit: u32, down: bool, now: Instant) {
        let mut m = self.latches.lock().unwrap();
        let l = m.entry(bit).or_insert(Latch {
            phys_down: false,
            wire_down_at: None,
        });
        if down {
            if l.phys_down {
                return;
            }
            // ¿El toque anterior sigue retenido en el cable? Hueco primero,
            // para que el receptor vea dos pulsaciones y no una fundida
            let still_latched = l.wire_down_at.is_some_and(|d| now < d + MIN_PRESS);
            l.phys_down = true;
            l.wire_down_at = Some(if still_latched { now + MIN_GAP } else { now });
        } else {
            l.phys_down = false;
        }
    }

    /// Máscara que va en el paquete construido en `now`.
    pub fn wire_at(&self, now: Instant) -> u32 {
        let m = self.latches.lock().unwrap();
        let mut mask = 0;
        for (bit, l) in m.iter() {
            let Some(d) = l.wire_down_at else { continue };
            let down = if l.phys_down {
                now >= d
            } else {
                now >= d && now < d + MIN_PRESS
            };
            if down {
                mask |= bit;
            }
        }
        mask
    }

    /// Lo que el dedo tiene pulsado ahora (para pintar).
    pub fn physical(&self) -> u32 {
        self.latches
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, l)| l.phys_down)
            .fold(0, |acc, (bit, _)| acc | bit)
    }

    /// Suelta todo: botones, scroll, los dos sticks y la pantalla táctil. El
    /// papel (GamePad) y el giro no son estado del dedo: se conservan.
    pub fn release_all(&self) {
        for l in self.latches.lock().unwrap().values_mut() {
            l.phys_down = false;
            l.wire_down_at = None;
        }
        self.scroll.store(0, Ordering::Relaxed);
        self.set_stick(0, 0);
        self.set_stick2(0, 0);
        self.set_touch(0, 0, false);
    }

    /// Stick del Nunchuk / izquierdo: sin latch (es analógico, va en cada
    /// paquete). El protocolo es simétrico (−127..127): −128 se recorta.
    pub fn set_stick(&self, x: i8, y: i8) {
        self.stick_x.store(x.max(-127), Ordering::Relaxed);
        self.stick_y.store(y.max(-127), Ordering::Relaxed);
    }

    pub fn stick(&self) -> (i8, i8) {
        (self.stick_x.load(Ordering::Relaxed), self.stick_y.load(Ordering::Relaxed))
    }

    /// Stick derecho del GamePad, igual que el izquierdo.
    pub fn set_stick2(&self, x: i8, y: i8) {
        self.stick_rx.store(x.max(-127), Ordering::Relaxed);
        self.stick_ry.store(y.max(-127), Ordering::Relaxed);
    }

    pub fn stick2(&self) -> (i8, i8) {
        (self.stick_rx.load(Ordering::Relaxed), self.stick_ry.load(Ordering::Relaxed))
    }

    /// Pantalla táctil del GamePad: posición (fracción 0..65535) y si hay dedo.
    pub fn set_touch(&self, x: u16, y: u16, down: bool) {
        self.touch_x.store(x, Ordering::Relaxed);
        self.touch_y.store(y, Ordering::Relaxed);
        self.touch_down.store(down, Ordering::Relaxed);
    }

    pub fn touch(&self) -> (u16, u16, bool) {
        (
            self.touch_x.load(Ordering::Relaxed),
            self.touch_y.load(Ordering::Relaxed),
            self.touch_down.load(Ordering::Relaxed),
        )
    }

    /// Entrar/salir de la pantalla GamePad: cambia cómo se construye el INPUT.
    pub fn set_gamepad(&self, on: bool) {
        self.gamepad.store(on, Ordering::Relaxed);
    }

    pub fn is_gamepad(&self) -> bool {
        self.gamepad.load(Ordering::Relaxed)
    }

    /// Giro apaisado: 0 = borde superior a la izquierda, 1 = a la derecha.
    pub fn set_rotation(&self, r: u8) {
        self.rotation.store(r, Ordering::Relaxed);
    }

    pub fn rotation(&self) -> u8 {
        self.rotation.load(Ordering::Relaxed)
    }

    pub fn bump_recenter(&self) {
        self.recenter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn recenter_count(&self) -> u8 {
        (self.recenter.load(Ordering::Relaxed) & 0xFF) as u8
    }

    /// Píxeles de la tira de scroll (+ = dedo hacia arriba = scroll up).
    pub fn add_scroll(&self, dy: i32) {
        self.scroll.fetch_add(dy, Ordering::Relaxed);
    }

    pub fn drain_scroll(&self) -> i16 {
        self.scroll
            .swap(0, Ordering::Relaxed)
            .clamp(i16::MIN as i32, i16::MAX as i32) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: u32 = 1;

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn toque_corto_dura_el_minimo_en_el_cable() {
        let b = Buttons::new();
        let t0 = Instant::now();
        b.set_at(A, true, at(t0, 0));
        assert_eq!(b.wire_at(at(t0, 0)), A, "flanco de bajada inmediato");
        b.set_at(A, false, at(t0, 20));
        assert_eq!(b.wire_at(at(t0, 20)), A);
        assert_eq!(b.wire_at(at(t0, 69)), A);
        assert_eq!(b.wire_at(at(t0, 70)), 0);
    }

    #[test]
    fn mantener_suelta_cuando_se_suelta() {
        let b = Buttons::new();
        let t0 = Instant::now();
        b.set_at(A, true, at(t0, 0));
        assert_eq!(b.wire_at(at(t0, 299)), A, "mantenido: sigue pulsado");
        b.set_at(A, false, at(t0, 300));
        assert_eq!(b.wire_at(at(t0, 300)), 0, "pasado el mínimo, se suelta al instante");
    }

    #[test]
    fn dos_toques_rapidos_no_se_funden() {
        let b = Buttons::new();
        let t0 = Instant::now();
        b.set_at(A, true, at(t0, 0));
        b.set_at(A, false, at(t0, 20));
        b.set_at(A, true, at(t0, 40)); // el primero aún retenido → hueco
        b.set_at(A, false, at(t0, 60));
        assert_eq!(b.wire_at(at(t0, 40)), 0, "hueco: el cable sube ya");
        assert_eq!(b.wire_at(at(t0, 49)), 0);
        assert_eq!(b.wire_at(at(t0, 50)), A, "re-pulsación tras 10 ms");
        assert_eq!(b.wire_at(at(t0, 119)), A);
        assert_eq!(b.wire_at(at(t0, 120)), 0);
    }

    #[test]
    fn dedo_y_cable_se_distinguen() {
        let b = Buttons::new();
        let t0 = Instant::now();
        b.set_at(A, true, at(t0, 0));
        b.set_at(A, false, at(t0, 10));
        assert_eq!(b.physical(), 0);
        assert_eq!(b.wire_at(at(t0, 30)), A);
        b.release_all();
        assert_eq!(b.wire_at(at(t0, 30)), 0);
    }

    #[test]
    fn scroll_y_recentrado() {
        let b = Buttons::new();
        b.add_scroll(30);
        b.add_scroll(-10);
        assert_eq!(b.drain_scroll(), 20);
        assert_eq!(b.drain_scroll(), 0);
        b.bump_recenter();
        b.bump_recenter();
        assert_eq!(b.recenter_count(), 2);
    }

    #[test]
    fn stick_se_recorta_y_se_suelta() {
        let b = Buttons::new();
        assert_eq!(b.stick(), (0, 0), "en reposo, centrado");
        b.set_stick(100, -50);
        assert_eq!(b.stick(), (100, -50));
        b.set_stick(-128, 127);
        assert_eq!(b.stick(), (-127, 127), "−128 no existe en el protocolo");
        b.set_stick(127, -128);
        assert_eq!(b.stick(), (127, -127));
        b.set(A, true);
        b.release_all();
        assert_eq!(b.stick(), (0, 0), "release_all también centra el stick");
        assert_eq!(b.physical(), 0);
    }

    #[test]
    fn stick_derecho_y_tactil_del_gamepad() {
        let b = Buttons::new();
        assert_eq!(b.stick2(), (0, 0));
        assert_eq!(b.touch(), (0, 0, false), "sin dedo en la pantalla táctil");
        b.set_stick2(-30, 120);
        assert_eq!(b.stick2(), (-30, 120));
        b.set_stick2(-128, -128);
        assert_eq!(b.stick2(), (-127, -127), "el derecho también recorta −128");
        b.set_touch(0x8000, 0x4000, true);
        assert_eq!(b.touch(), (0x8000, 0x4000, true));
        b.set_touch(0x8000, 0x4000, false);
        assert_eq!(b.touch(), (0x8000, 0x4000, false), "al levantar el dedo quedan las últimas coordenadas");
        b.set_stick2(10, 10);
        b.set_touch(1, 2, true);
        b.release_all();
        assert_eq!(b.stick2(), (0, 0), "release_all centra el stick derecho");
        assert_eq!(b.touch(), (0, 0, false), "y apaga la pantalla táctil");
    }

    #[test]
    fn gamepad_y_giro_no_son_estado_del_dedo() {
        let b = Buttons::new();
        assert!(!b.is_gamepad(), "por defecto, mando de 72 bytes");
        assert_eq!(b.rotation(), 0, "por defecto, borde superior a la izquierda");
        b.set_gamepad(true);
        b.set_rotation(1);
        b.release_all();
        assert!(b.is_gamepad(), "release_all no cambia el papel");
        assert_eq!(b.rotation(), 1, "ni el giro");
        b.set_gamepad(false);
        assert!(!b.is_gamepad());
    }
}
