//! Estado de botones compartido entre la UI (dedo) y el hilo de paquetes
//! (cable). Misma regla que en Android (PressLatch): cada pulsación dura al
//! menos 70 ms EN EL CABLE (~18 paquetes), porque un toque de 20-40 ms se lo
//! traga una ráfaga de pérdida Wi-Fi o el muestreo a 60 Hz del juego. El
//! flanco de bajada sale al instante; solo se retrasa la suelta de los toques
//! cortos; mantener pulsado no cambia nada; dos toques seguidos no se funden
//! (hueco de 10 ms). Aquí se evalúa de forma perezosa al construir cada
//! paquete: sin temporizadores.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
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

    pub fn release_all(&self) {
        for l in self.latches.lock().unwrap().values_mut() {
            l.phys_down = false;
            l.wire_down_at = None;
        }
        self.scroll.store(0, Ordering::Relaxed);
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
}
