//! Modo automático: el receptor cambia de modo solo cuando el usuario abre
//! o cierra Dolphin o Cemu (vigilante `emu-watch`, una muestra cada 2 s).
//! Las reglas son puras (sin hilos ni procesos) y están testeadas aquí:
//! - abrir Dolphin → modo Dolphin; abrir Cemu → modo Wii U (los dos en la
//!   misma muestra: Dolphin);
//! - cerrar el emulador cuyo modo está activo → puntero, o el modo del otro
//!   emulador si sigue abierto;
//! - cerrar un emulador que no manda (el usuario ya había vuelto al puntero
//!   a mano, o está en el otro modo) no cambia nada;
//! - sin flanco no hay cambio: lo elegido a mano en el móvil se respeta
//!   hasta la siguiente vez que se abra o cierre algo.

use crate::state::{Mode, SharedState};
use std::time::Duration;
use crate::tr;

/// Qué emuladores están abiertos.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmuState {
    pub dolphin: bool,
    pub cemu: bool,
}

/// Por qué cambia el modo (para el aviso a los móviles y el log).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    DolphinOpened,
    CemuOpened,
    DolphinClosed,
    CemuClosed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub mode: Mode,
    pub reason: Reason,
}

/// El cambio que toca (si toca) al pasar de `prev` a `now` estando en `mode`.
pub fn next(prev: EmuState, now: EmuState, mode: Mode, enabled: bool) -> Option<Change> {
    if !enabled || prev == now {
        return None;
    }
    let change = if !prev.dolphin && now.dolphin {
        Change { mode: Mode::Dolphin, reason: Reason::DolphinOpened }
    } else if !prev.cemu && now.cemu {
        Change { mode: Mode::Cemu, reason: Reason::CemuOpened }
    } else if prev.dolphin && !now.dolphin && mode == Mode::Dolphin {
        Change { mode: if now.cemu { Mode::Cemu } else { Mode::Pointer }, reason: Reason::DolphinClosed }
    } else if prev.cemu && !now.cemu && mode == Mode::Cemu {
        Change { mode: if now.dolphin { Mode::Dolphin } else { Mode::Pointer }, reason: Reason::CemuClosed }
    } else {
        return None;
    };
    (change.mode != mode).then_some(change)
}

/// Aviso para los móviles (y el log).
pub fn notice(c: Change) -> &'static str {
    match (c.reason, c.mode) {
        (Reason::DolphinOpened, _) => tr!("auto.dolphin_opened"),
        (Reason::CemuOpened, _) => tr!("auto.cemu_opened"),
        (Reason::DolphinClosed, Mode::Cemu) => tr!("auto.dolphin_closed_cemu"),
        (Reason::DolphinClosed, _) => tr!("auto.dolphin_closed"),
        (Reason::CemuClosed, Mode::Dolphin) => tr!("auto.cemu_closed_dolphin"),
        (Reason::CemuClosed, _) => tr!("auto.cemu_closed"),
    }
}

/// Dos muestras seguidas iguales antes de dar un cambio por bueno: un
/// emulador que se reinicia solo, o un instante sin proceso, no hace saltar
/// el modo. La primera muestra fija el estado sin flanco: un emulador que ya
/// estaba abierto al arrancar el receptor no cambia nada.
#[derive(Debug, Default)]
pub struct Debounce {
    stable: Option<EmuState>,
    pending: Option<EmuState>,
}

impl Debounce {
    /// Muestra nueva; devuelve (antes, ahora) cuando el estado estable cambia.
    pub fn observe(&mut self, sample: EmuState) -> Option<(EmuState, EmuState)> {
        let Some(stable) = self.stable else {
            self.stable = Some(sample);
            return None;
        };
        if sample == stable {
            self.pending = None;
            return None;
        }
        if self.pending == Some(sample) {
            self.pending = None;
            self.stable = Some(sample);
            return Some((stable, sample));
        }
        self.pending = Some(sample);
        None
    }
}

/// Una muestra: qué emuladores están abiertos (y, de paso, dónde viven: la
/// carpeta se aprende al verlos abiertos, para configurarlos cerrados).
fn observe(shared: &SharedState) -> EmuState {
    // Solo para los e2e en el propio equipo: como si no hubiera emuladores
    if std::env::var_os("PEPOMOTE_ASSUME_EMULATOR_CLOSED").is_some() {
        return EmuState::default();
    }
    let (dolphin, dolphin_dir) = crate::dolphin::running_exe();
    let (cemu, cemu_dir) = crate::cemu::running_exe();
    crate::dolphin::learn_dir(shared, dolphin_dir);
    crate::cemu::learn_dir(shared, cemu_dir);
    EmuState { dolphin, cemu }
}

/// Vigilante de emuladores (hilo `emu-watch`, cada 2 s): aplica lo que quedó
/// pendiente porque el emulador estaba abierto en cuanto se cierra (Dolphin y
/// Cemu sobreescriben su configuración al salir) y, con el modo automático
/// activado, cambia de modo al abrir o cerrar Dolphin o Cemu.
pub fn start_watcher(shared: SharedState) {
    let _ = std::thread::Builder::new().name("emu-watch".into()).spawn(move || {
        let mut debounce = Debounce::default();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let sample = observe(&shared);
            let (dolphin_pending, cemu_pending) = {
                let s = shared.lock().unwrap_or_else(|e| e.into_inner());
                (s.dolphin_pending, s.cemu_pending)
            };
            if dolphin_pending && !sample.dolphin {
                crate::dolphin::apply_pending(&shared);
            }
            if cemu_pending && !sample.cemu {
                crate::cemu::apply_pending(&shared);
            }
            if let Some((prev, now)) = debounce.observe(sample) {
                let (enabled, mode) = {
                    let s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    (s.config.auto_mode, s.mode)
                };
                if let Some(change) = next(prev, now, mode, enabled) {
                    crate::net::control::set_mode_from_pc(&shared, change.mode, notice(change));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: EmuState = EmuState { dolphin: false, cemu: false };
    const DOLPHIN: EmuState = EmuState { dolphin: true, cemu: false };
    const CEMU: EmuState = EmuState { dolphin: false, cemu: true };
    const BOTH: EmuState = EmuState { dolphin: true, cemu: true };

    fn change(prev: EmuState, now: EmuState, mode: Mode) -> Option<(Mode, Reason)> {
        next(prev, now, mode, true).map(|c| (c.mode, c.reason))
    }

    #[test]
    fn arranque_de_dolphin_cambia_a_dolphin() {
        assert_eq!(change(NONE, DOLPHIN, Mode::Pointer), Some((Mode::Dolphin, Reason::DolphinOpened)));
        // también estando en Wii U: lo último que se abre manda
        assert_eq!(change(CEMU, BOTH, Mode::Cemu), Some((Mode::Dolphin, Reason::DolphinOpened)));
    }

    #[test]
    fn arranque_de_cemu_cambia_a_wiiu() {
        assert_eq!(change(NONE, CEMU, Mode::Pointer), Some((Mode::Cemu, Reason::CemuOpened)));
        assert_eq!(change(DOLPHIN, BOTH, Mode::Dolphin), Some((Mode::Cemu, Reason::CemuOpened)));
    }

    #[test]
    fn los_dos_a_la_vez_gana_dolphin() {
        assert_eq!(change(NONE, BOTH, Mode::Pointer), Some((Mode::Dolphin, Reason::DolphinOpened)));
        // Cemu se cierra y Dolphin se abre en la misma muestra: manda el que se abre
        assert_eq!(change(CEMU, DOLPHIN, Mode::Cemu), Some((Mode::Dolphin, Reason::DolphinOpened)));
    }

    #[test]
    fn cierre_del_emulador_activo_vuelve_a_puntero() {
        assert_eq!(change(DOLPHIN, NONE, Mode::Dolphin), Some((Mode::Pointer, Reason::DolphinClosed)));
        assert_eq!(change(CEMU, NONE, Mode::Cemu), Some((Mode::Pointer, Reason::CemuClosed)));
    }

    #[test]
    fn cierre_con_el_otro_abierto_salta_al_otro() {
        assert_eq!(change(BOTH, CEMU, Mode::Dolphin), Some((Mode::Cemu, Reason::DolphinClosed)));
        assert_eq!(change(BOTH, DOLPHIN, Mode::Cemu), Some((Mode::Dolphin, Reason::CemuClosed)));
    }

    #[test]
    fn cierre_de_un_emulador_que_no_manda_no_cambia_nada() {
        // el usuario volvió al puntero a mano con Dolphin abierto
        assert_eq!(change(DOLPHIN, NONE, Mode::Pointer), None);
        // se juega en Wii U y se cierra Dolphin, que estaba de fondo
        assert_eq!(change(BOTH, CEMU, Mode::Cemu), None);
        assert_eq!(change(CEMU, NONE, Mode::Dolphin), None);
    }

    #[test]
    fn desactivado_nunca_cambia() {
        for (p, n, m) in [(NONE, DOLPHIN, Mode::Pointer), (NONE, CEMU, Mode::Pointer), (DOLPHIN, NONE, Mode::Dolphin)] {
            assert_eq!(next(p, n, m, false), None);
        }
    }

    #[test]
    fn sin_flanco_nada() {
        for s in [NONE, DOLPHIN, CEMU, BOTH] {
            for m in Mode::ALL {
                assert_eq!(change(s, s, m), None, "{s:?} {m:?}");
            }
        }
    }

    #[test]
    fn ya_en_ese_modo_no_repite() {
        assert_eq!(change(NONE, DOLPHIN, Mode::Dolphin), None);
        assert_eq!(change(NONE, CEMU, Mode::Cemu), None);
    }

    #[test]
    fn los_avisos_dicen_lo_que_ha_pasado() {
        let n = |p, now, m| notice(next(p, now, m, true).unwrap());
        assert_eq!(n(NONE, DOLPHIN, Mode::Pointer), "Dolphin abierto: modo Dolphin");
        assert_eq!(n(NONE, CEMU, Mode::Pointer), "Cemu abierto: modo Wii U");
        assert_eq!(n(DOLPHIN, NONE, Mode::Dolphin), "Dolphin cerrado: modo puntero");
        assert_eq!(n(CEMU, NONE, Mode::Cemu), "Cemu cerrado: modo puntero");
        assert_eq!(n(BOTH, CEMU, Mode::Dolphin), "Dolphin cerrado: modo Wii U (Cemu sigue abierto)");
        assert_eq!(n(BOTH, DOLPHIN, Mode::Cemu), "Cemu cerrado: modo Dolphin (Dolphin sigue abierto)");
    }

    #[test]
    fn debounce_exige_dos_muestras() {
        let mut d = Debounce::default();
        assert_eq!(d.observe(DOLPHIN), None, "la primera muestra no es flanco: Dolphin ya estaba abierto");
        assert_eq!(d.observe(DOLPHIN), None);
        assert_eq!(d.observe(NONE), None, "una muestra sin Dolphin aún no cuenta");
        assert_eq!(d.observe(DOLPHIN), None, "ha vuelto: era un reinicio, nada");
        assert_eq!(d.observe(NONE), None);
        assert_eq!(d.observe(NONE), Some((DOLPHIN, NONE)), "dos seguidas: cerrado de verdad");
        assert_eq!(d.observe(NONE), None, "ya estable");
        assert_eq!(d.observe(CEMU), None);
        assert_eq!(d.observe(BOTH), None, "cambió otra vez antes de confirmarse: se empieza de nuevo");
        assert_eq!(d.observe(BOTH), Some((NONE, BOTH)));
    }
}
