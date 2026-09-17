//! Modo automático: el receptor cambia de modo solo cuando el usuario abre
//! o cierra Dolphin, Cemu, Eden o RetroArch (vigilante `emu-watch`, una
//! muestra cada 2 s). El mismo vigilante mira el historial de RetroArch para
//! anunciar a los móviles qué juego (y qué consola) acaba de cargar.
//! Las reglas son puras (sin hilos ni procesos) y están testeadas aquí:
//! - abrir Dolphin → modo Dolphin; abrir Cemu → modo Wii U (los dos en la
//!   misma muestra: Dolphin);
//! - cerrar conserva el modo salvo que se active el retorno automático;
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
    pub eden: bool,
    pub retroarch: bool,
}

/// Por qué cambia el modo (para el aviso a los móviles y el log).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    DolphinOpened,
    CemuOpened,
    DolphinClosed,
    CemuClosed,
    EdenOpened,
    EdenClosed,
    RetroArchOpened,
    RetroArchClosed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub mode: Mode,
    pub reason: Reason,
}

/// El cambio que toca (si toca) al pasar de `prev` a `now` estando en `mode`.
pub fn next(prev: EmuState, now: EmuState, mode: Mode, enabled: bool, return_to_pointer: bool) -> Option<Change> {
    if !enabled || prev == now {
        return None;
    }
    let change = if !prev.dolphin && now.dolphin {
        Change { mode: Mode::Dolphin, reason: Reason::DolphinOpened }
    } else if !prev.cemu && now.cemu {
        Change { mode: Mode::Cemu, reason: Reason::CemuOpened }
    } else if !prev.eden && now.eden {
        Change { mode: Mode::Switch, reason: Reason::EdenOpened }
    } else if !prev.retroarch && now.retroarch {
        Change { mode: Mode::RetroArch, reason: Reason::RetroArchOpened }
    } else if return_to_pointer && prev.dolphin && !now.dolphin && mode == Mode::Dolphin {
        Change { mode: fallback(now), reason: Reason::DolphinClosed }
    } else if return_to_pointer && prev.cemu && !now.cemu && mode == Mode::Cemu {
        Change { mode: fallback(now), reason: Reason::CemuClosed }
    } else if return_to_pointer && prev.eden && !now.eden && mode == Mode::Switch {
        Change { mode: fallback(now), reason: Reason::EdenClosed }
    } else if return_to_pointer && prev.retroarch && !now.retroarch && mode == Mode::RetroArch {
        Change { mode: fallback(now), reason: Reason::RetroArchClosed }
    } else {
        return None;
    };
    (change.mode != mode).then_some(change)
}

fn fallback(now: EmuState) -> Mode {
    if now.dolphin {Mode::Dolphin} else if now.cemu {Mode::Cemu} else if now.eden {Mode::Switch} else if now.retroarch {Mode::RetroArch} else {Mode::Pointer}
}

/// Aviso para los móviles (y el log).
pub fn notice(c: Change) -> &'static str {
    match (c.reason, c.mode) {
        (Reason::DolphinOpened, _) => tr!("auto.dolphin_opened"),
        (Reason::CemuOpened, _) => tr!("auto.cemu_opened"),
        (Reason::DolphinClosed, Mode::Cemu) => tr!("auto.dolphin_closed_cemu"),
        (Reason::DolphinClosed, Mode::Switch) => tr!("auto.dolphin_closed_eden"),
        (Reason::DolphinClosed, Mode::RetroArch) => tr!("auto.dolphin_closed_retroarch"),
        (Reason::DolphinClosed, _) => tr!("auto.dolphin_closed"),
        (Reason::CemuClosed, Mode::Dolphin) => tr!("auto.cemu_closed_dolphin"),
        (Reason::CemuClosed, Mode::Switch) => tr!("auto.cemu_closed_eden"),
        (Reason::EdenOpened, _) => tr!("auto.eden_opened"),
        (Reason::EdenClosed, Mode::Dolphin) => tr!("auto.eden_closed_dolphin"),
        (Reason::EdenClosed, Mode::Cemu) => tr!("auto.eden_closed_cemu"),
        (Reason::EdenClosed, Mode::RetroArch) => tr!("auto.eden_closed_retroarch"),
        (Reason::EdenClosed, _) => tr!("auto.eden_closed"),
        (Reason::CemuClosed, Mode::RetroArch) => tr!("auto.cemu_closed_retroarch"),
        (Reason::CemuClosed, _) => tr!("auto.cemu_closed"),
        (Reason::RetroArchOpened, _) => tr!("auto.retroarch_opened"),
        (Reason::RetroArchClosed, Mode::Dolphin) => tr!("auto.retroarch_closed_dolphin"),
        (Reason::RetroArchClosed, Mode::Cemu) => tr!("auto.retroarch_closed_cemu"),
        (Reason::RetroArchClosed, Mode::Switch) => tr!("auto.retroarch_closed_eden"),
        (Reason::RetroArchClosed, _) => tr!("auto.retroarch_closed"),
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
    let (eden, eden_dir) = crate::eden::running_exe();
    crate::eden::learn_dir(shared, eden_dir);
    let (retroarch, retroarch_dir) = crate::retroarch::running_exe();
    crate::retroarch::learn_dir(shared, retroarch_dir);
    EmuState { dolphin, cemu, eden, retroarch }
}

/// Vigilante de emuladores (hilo `emu-watch`, cada 2 s): aplica lo que quedó
/// pendiente porque el emulador estaba abierto en cuanto se cierra (Dolphin y
/// Cemu sobreescriben su configuración al salir) y, con el modo automático
/// activado, cambia de modo al abrir o cerrar Dolphin o Cemu.
pub fn start_watcher(shared: SharedState) {
    let _ = crate::threads::spawn_guarded(
        "emu-watch",
        crate::threads::OnPanic::Restart { after: Duration::from_secs(5), max: 50 },
        move || {
        let mut debounce = Debounce::default();
        // Qué juego acaba de cargar RetroArch: su historial cambia al cargar
        // contenido (la misma partida reabierta no lo toca: se relee al abrirse)
        let mut games = crate::retroarch::GameWatch::default();
        let home = directories::BaseDirs::new().map(|b| b.home_dir().to_owned());
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let sample = observe(&shared);
            let cfg = shared.lock().unwrap_or_else(|e| e.into_inner()).config.clone();
            if games.poll(&crate::retroarch::data_dirs(&cfg), home.as_deref(), sample.retroarch) {
                crate::retroarch::publish_game(&shared, games.game().cloned());
            }
            let (dolphin_pending, cemu_pending, cemu_cleanup, eden_pending, retroarch_pending) = {
                let s = shared.lock().unwrap_or_else(|e| e.into_inner());
                (s.dolphin_pending, s.cemu_pending, s.cemu_cleanup_pending, s.eden_pending, s.retroarch_pending)
            };
            if dolphin_pending && !sample.dolphin {
                crate::dolphin::apply_pending(&shared);
            }
            if cemu_pending && !sample.cemu {
                crate::cemu::apply_pending(&shared);
            }
            if eden_pending && !sample.eden {
                crate::eden::apply_pending(&shared);
            }
            if retroarch_pending && !sample.retroarch {
                crate::retroarch::apply_pending(&shared);
            }
            if cemu_cleanup && !sample.cemu {
                crate::cemu::apply_cleanup_pending(&shared);
            }
            if let Some((prev, now)) = debounce.observe(sample) {
                let (enabled, return_to_pointer, mode) = {
                    let s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    (s.config.auto_mode, s.config.return_to_pointer, s.mode)
                };
                if let Some(change) = next(prev, now, mode, enabled, return_to_pointer) {
                    crate::net::control::set_mode_from_pc(&shared, change.mode, notice(change));
                }
            }
        }
    },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: EmuState = EmuState { dolphin: false, cemu: false, eden: false, retroarch: false };
    const DOLPHIN: EmuState = EmuState { dolphin: true, ..NONE };
    const CEMU: EmuState = EmuState { cemu: true, ..NONE };
    const BOTH: EmuState = EmuState { dolphin: true, cemu: true, ..NONE };
    const RETROARCH: EmuState = EmuState { retroarch: true, ..NONE };

    fn change(prev: EmuState, now: EmuState, mode: Mode) -> Option<(Mode, Reason)> {
        next(prev, now, mode, true, true).map(|c| (c.mode, c.reason))
    }

    #[test]
    fn requested_default_keeps_console_mode_after_close() {
        let eden = EmuState { eden: true, ..NONE };
        for (prev, mode) in [(DOLPHIN, Mode::Dolphin), (CEMU, Mode::Cemu), (eden, Mode::Switch)] {
            assert_eq!(next(prev, NONE, mode, true, false), None, "closing {mode:?} must preserve its mode");
        }
        assert_eq!(next(BOTH, CEMU, Mode::Dolphin, true, false), None);
        assert_eq!(next(BOTH, DOLPHIN, Mode::Cemu, true, false), None);
        assert_eq!(next(NONE, DOLPHIN, Mode::Switch, true, false).unwrap().mode, Mode::Dolphin);
        assert_eq!(next(NONE, CEMU, Mode::Dolphin, true, false).unwrap().mode, Mode::Cemu);
        assert_eq!(next(NONE, eden, Mode::Pointer, true, false).unwrap().mode, Mode::Switch);
    }

    #[test]
    fn switch_open_close_and_fallback_for_three_emulators() {
        let eden=EmuState {eden:true,..NONE};
        let all=EmuState {eden:true,..BOTH};
        assert_eq!(change(NONE,eden,Mode::Pointer),Some((Mode::Switch,Reason::EdenOpened)));
        assert_eq!(change(eden,NONE,Mode::Switch),Some((Mode::Pointer,Reason::EdenClosed)));
        assert_eq!(change(all,BOTH,Mode::Switch),Some((Mode::Dolphin,Reason::EdenClosed)));
        assert_eq!(change(all,eden,Mode::Dolphin),Some((Mode::Switch,Reason::DolphinClosed)));
        assert_eq!(change(all,eden,Mode::Cemu),Some((Mode::Switch,Reason::CemuClosed)));
        assert_eq!(change(NONE,all,Mode::Pointer),Some((Mode::Dolphin,Reason::DolphinOpened)));
        assert_eq!(change(eden,NONE,Mode::Pointer),None);
    }

    #[test]
    fn retroarch_abre_cierra_y_cede_a_los_demas() {
        assert_eq!(change(NONE, RETROARCH, Mode::Pointer), Some((Mode::RetroArch, Reason::RetroArchOpened)));
        assert_eq!(change(NONE, RETROARCH, Mode::Switch), Some((Mode::RetroArch, Reason::RetroArchOpened)));
        assert_eq!(change(RETROARCH, NONE, Mode::RetroArch), Some((Mode::Pointer, Reason::RetroArchClosed)));
        assert_eq!(next(RETROARCH, NONE, Mode::RetroArch, true, false), None, "cerrar conserva el modo por defecto");
        // los demás abiertos a la vez: lo que se abre manda; al cerrar RetroArch, el que quede
        let ra_dolphin = EmuState { retroarch: true, ..DOLPHIN };
        assert_eq!(change(NONE, ra_dolphin, Mode::Pointer), Some((Mode::Dolphin, Reason::DolphinOpened)));
        assert_eq!(change(RETROARCH, ra_dolphin, Mode::RetroArch), Some((Mode::Dolphin, Reason::DolphinOpened)));
        assert_eq!(change(ra_dolphin, DOLPHIN, Mode::RetroArch), Some((Mode::Dolphin, Reason::RetroArchClosed)));
        assert_eq!(change(ra_dolphin, RETROARCH, Mode::Dolphin), Some((Mode::RetroArch, Reason::DolphinClosed)));
        let ra_eden = EmuState { retroarch: true, eden: true, ..NONE };
        assert_eq!(change(ra_eden, RETROARCH, Mode::Switch), Some((Mode::RetroArch, Reason::EdenClosed)));
        assert_eq!(change(ra_eden, EmuState { eden: true, ..NONE }, Mode::RetroArch), Some((Mode::Switch, Reason::RetroArchClosed)));
        // cerrar RetroArch cuando no manda no cambia nada
        assert_eq!(change(RETROARCH, NONE, Mode::Pointer), None);
        assert_eq!(change(ra_dolphin, DOLPHIN, Mode::Dolphin), None);
        let n = |p, now, m| notice(next(p, now, m, true, true).unwrap());
        assert_eq!(n(NONE, RETROARCH, Mode::Pointer), "RetroArch abierto: modo RetroArch");
        assert_eq!(n(RETROARCH, NONE, Mode::RetroArch), "RetroArch cerrado: modo puntero");
        assert_eq!(n(ra_dolphin, DOLPHIN, Mode::RetroArch), "RetroArch cerrado: modo Dolphin (Dolphin sigue abierto)");
        assert_eq!(n(ra_dolphin, RETROARCH, Mode::Dolphin), "Dolphin cerrado: modo RetroArch (RetroArch sigue abierto)");
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
            for returning in [false, true] {
                assert_eq!(next(p, n, m, false, returning), None);
            }
        }
    }

    #[test]
    fn sin_flanco_nada() {
        for s in [NONE, DOLPHIN, CEMU, BOTH, RETROARCH] {
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
        let n = |p, now, m| notice(next(p, now, m, true, true).unwrap());
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
