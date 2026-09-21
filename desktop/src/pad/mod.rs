//! Mando universal: el móvil como mando de Xbox 360 para cualquier juego.
//!
//! El mando virtual ya existe — lo crea `rumble` para poder *recibir* la
//! vibración del juego (ViGEmBus en Windows, uinput en Linux) y hasta ahora
//! se quedaba siempre en reposo. Este módulo es la flecha de vuelta: los
//! botones del móvil llegan hasta él.
//!
//! No hay nada que configurar en el juego. El mando lo crea el sistema, así
//! que todo programa lo ve solo, sin que PepoMote tenga que escribir ningún
//! fichero de nadie (a diferencia de los modos de emulador).
//!
//! - [`mapping`] traduce el INPUT a un estado de mando. Puro.
//! - [`aim`] convierte el giro del móvil en el stick derecho. Puro.
//! - [`evdev`] pasa ese estado a eventos de Linux. Puro, y compilado en
//!   todos los sistemas para que sus trampas se prueben en todos.
//! - [`Feed`] decide *cuándo* hay que mandarlo. Puro, con el reloj por
//!   parámetro, igual que `rumble::Track`.

pub mod aim;
/// En Linux es la salida de verdad; en los demás sistemas se compila solo
/// para los tests, que así vigilan sus trampas desde cualquier máquina sin
/// dejar código muerto en el binario.
#[cfg(any(target_os = "linux", test))]
pub mod evdev;
pub mod mapping;

use crate::net::MAX_PLAYERS;
pub use mapping::{pad_state, PadState};
use pmp::InputPacket;
use std::time::{Duration, Instant};

/// Sin noticias de un jugador durante este tiempo, su mando vuelve a reposo.
/// Si no, un móvil que se va al fondo (o pierde la red) con un botón pulsado
/// dejaría el juego con ese botón clavado hasta que se cierre la sesión.
pub const IDLE_RESET: Duration = Duration::from_millis(1000);

/// El estado del mando para este paquete, giro incluido.
pub fn state_of(p: &InputPacket) -> PadState {
    pad_state(p, aim::stick(p, aim::FULL_SCALE_DEG_S))
}

#[derive(Clone, Copy, Default)]
struct Slot {
    sent: Option<PadState>,
    at: Option<Instant>,
}

/// Qué mandar y cuándo. El mando virtual conserva lo último que se le
/// escribió, así que solo se le habla cuando algo cambia: a 250 Hz, repetir
/// el mismo estado serían cientos de IOCTL por segundo para nada.
#[derive(Default)]
pub struct Feed {
    slots: [Slot; MAX_PLAYERS],
}

impl Feed {
    pub fn new() -> Feed {
        Feed::default()
    }

    /// El estado que hay que escribir ahora, o `None` si no ha cambiado.
    pub fn push(&mut self, slot: usize, s: PadState, now: Instant) -> Option<PadState> {
        let Some(sl) = self.slots.get_mut(slot) else { return None };
        sl.at = Some(now);
        if sl.sent == Some(s) {
            return None;
        }
        sl.sent = Some(s);
        Some(s)
    }

    /// Jugadores que llevan demasiado sin mandar nada y tienen algo pulsado:
    /// se les suelta el mando para que el juego no se quede con un botón
    /// clavado.
    pub fn idle(&mut self, now: Instant) -> Vec<(usize, PadState)> {
        let mut out = Vec::new();
        for (i, sl) in self.slots.iter_mut().enumerate() {
            let Some(at) = sl.at else { continue };
            if now.duration_since(at) < IDLE_RESET {
                continue;
            }
            sl.at = Some(now);
            if sl.sent.is_some_and(|s| s != PadState::default()) {
                sl.sent = Some(PadState::default());
                out.push((i, PadState::default()));
            }
        }
        out
    }

    /// El jugador se ha ido (o cambió el modo): la próxima vez se manda todo.
    pub fn forget(&mut self, slot: usize) {
        if let Some(sl) = self.slots.get_mut(slot) {
            *sl = Slot::default();
        }
    }

    /// Cambió el modo: lo que se le escribió al mando ya no vale (al salir
    /// del mando universal se le deja en reposo desde fuera, y al volver hay
    /// que reescribirlo entero aunque el jugador siga con el mismo botón).
    pub fn forget_all(&mut self) {
        self.slots = [Slot::default(); MAX_PLAYERS];
    }

    /// Olvida a los que ya no están. Importa porque al irse un jugador su
    /// mando virtual se desenchufa, y el que se cree después nace en reposo:
    /// si siguiéramos creyendo que tiene algo pulsado no se lo escribiríamos.
    pub fn forget_absent(&mut self, live: &[bool; MAX_PLAYERS]) {
        for (i, alive) in live.iter().enumerate() {
            if !alive {
                self.forget(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pulsado(b: u16) -> PadState {
        PadState { buttons: b, ..PadState::default() }
    }

    #[test]
    fn el_primer_estado_siempre_sale() {
        let mut f = Feed::new();
        let t = Instant::now();
        assert_eq!(f.push(0, PadState::default(), t), Some(PadState::default()));
    }

    #[test]
    fn el_mismo_estado_no_se_repite() {
        let mut f = Feed::new();
        let t = Instant::now();
        let s = pulsado(mapping::xb::A);
        assert_eq!(f.push(0, s, t), Some(s));
        assert_eq!(f.push(0, s, t + Duration::from_millis(4)), None);
        assert_eq!(f.push(0, s, t + Duration::from_millis(500)), None, "no hay refresco: el mando conserva lo último");
    }

    #[test]
    fn soltar_el_boton_si_sale() {
        let mut f = Feed::new();
        let t = Instant::now();
        let s = pulsado(mapping::xb::A);
        f.push(0, s, t);
        assert_eq!(f.push(0, PadState::default(), t + Duration::from_millis(8)), Some(PadState::default()));
    }

    #[test]
    fn cada_jugador_va_por_su_cuenta() {
        let mut f = Feed::new();
        let t = Instant::now();
        let s = pulsado(mapping::xb::B);
        assert_eq!(f.push(0, s, t), Some(s));
        assert_eq!(f.push(1, s, t), Some(s), "el jugador 2 no hereda lo del 1");
        assert_eq!(f.push(0, s, t), None);
    }

    #[test]
    fn un_movil_que_calla_suelta_el_mando() {
        let mut f = Feed::new();
        let t = Instant::now();
        f.push(0, pulsado(mapping::xb::A), t);
        assert!(f.idle(t + IDLE_RESET / 2).is_empty(), "todavía no");
        let out = f.idle(t + IDLE_RESET + Duration::from_millis(1));
        assert_eq!(out, vec![(0, PadState::default())], "el botón no se queda clavado");
    }

    #[test]
    fn un_mando_en_reposo_no_genera_trabajo_al_callar() {
        let mut f = Feed::new();
        let t = Instant::now();
        f.push(0, PadState::default(), t);
        assert!(f.idle(t + IDLE_RESET * 3).is_empty(), "ya estaba suelto");
    }

    #[test]
    fn soltado_una_vez_no_se_suelta_en_bucle() {
        let mut f = Feed::new();
        let t = Instant::now();
        f.push(0, pulsado(mapping::xb::A), t);
        let t2 = t + IDLE_RESET + Duration::from_millis(1);
        assert_eq!(f.idle(t2).len(), 1);
        assert!(f.idle(t2 + IDLE_RESET * 2).is_empty());
    }

    #[test]
    fn un_jugador_que_nunca_hablo_no_se_toca() {
        let mut f = Feed::new();
        assert!(f.idle(Instant::now() + IDLE_RESET * 10).is_empty());
    }

    #[test]
    fn olvidar_obliga_a_reenviar() {
        let mut f = Feed::new();
        let t = Instant::now();
        let s = pulsado(mapping::xb::X);
        f.push(0, s, t);
        assert_eq!(f.push(0, s, t), None);
        f.forget(0);
        assert_eq!(f.push(0, s, t), Some(s), "mando nuevo: hay que escribirlo entero");
    }

    #[test]
    fn cambiar_de_modo_obliga_a_reescribir_el_mando_entero() {
        let mut f = Feed::new();
        let t = Instant::now();
        let s = pulsado(mapping::xb::A);
        f.push(0, s, t);
        f.push(1, s, t);
        f.forget_all();
        assert_eq!(f.push(0, s, t), Some(s));
        assert_eq!(f.push(1, s, t), Some(s));
    }

    #[test]
    fn al_irse_un_jugador_se_olvida_lo_suyo_y_no_lo_de_los_demas() {
        let mut f = Feed::new();
        let t = Instant::now();
        let s = pulsado(mapping::xb::A);
        f.push(0, s, t);
        f.push(1, s, t);
        let mut live = [false; MAX_PLAYERS];
        live[1] = true;
        f.forget_absent(&live);
        assert_eq!(f.push(0, s, t), Some(s), "el que se fue se reescribe entero");
        assert_eq!(f.push(1, s, t), None, "el que sigue no se toca");
    }

    #[test]
    fn un_slot_fuera_de_rango_no_rompe() {
        let mut f = Feed::new();
        assert_eq!(f.push(99, PadState::default(), Instant::now()), None);
        f.forget(99);
    }

    #[test]
    fn el_giro_entra_en_el_estado_completo() {
        let mut p = InputPacket::default();
        p.gyro = [0.0, 0.0, aim::FULL_SCALE_DEG_S.to_radians()];
        assert_ne!(state_of(&p).rx, 0, "mover el móvil mueve el stick derecho");
        assert_eq!(state_of(&InputPacket::default()), PadState::default(), "quieto es reposo");
    }
}
