//! Lo que comparten los backends de Linux (uinput y Wayland) y el de macOS:
//! el mapeo del apuntado absoluto a la pantalla elegida, la lista de teclas
//! fijas y el acumulador de la rueda.

use super::KeyCode;

/// Valor máximo de los ejes absolutos (Linux). El dispositivo cubre el
/// escritorio ENTERO (uinput: ABS_X/ABS_Y; Wayland: extent de
/// `motion_absolute`).
pub(super) const ABS_MAX: i32 = 32767;

/// (nx, ny) de la pantalla objetivo → (x, y) 0..1 del escritorio entero.
pub(super) fn map_norm(nx: f32, ny: f32, target: [f32; 4]) -> (f32, f32) {
    let x = target[0] + nx.clamp(0.0, 1.0) * target[2];
    let y = target[1] + ny.clamp(0.0, 1.0) * target[3];
    (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))
}

/// (nx, ny) de la pantalla objetivo → valor absoluto del dispositivo, que
/// cubre el escritorio entero (Linux).
pub(super) fn map_abs(nx: f32, ny: f32, target: [f32; 4]) -> (i32, i32) {
    let (x, y) = map_norm(nx, ny, target);
    ((x * ABS_MAX as f32).round() as i32, (y * ABS_MAX as f32).round() as i32)
}

/// Inversa de [`map_norm`] sin recortar: un punto (0..1) del escritorio entero
/// en unidades de la pantalla objetivo. Puede salirse de 0..1: así el motor
/// del puntero distingue el borde de la pantalla de un salto del ratón real.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(super) fn norm_in(target: [f32; 4], x: f32, y: f32) -> (f32, f32) {
    let w = if target[2] > 0.0 { target[2] } else { 1.0 };
    let h = if target[3] > 0.0 { target[3] } else { 1.0 };
    ((x - target[0]) / w, (y - target[1]) / h)
}

/// Teclas sin carácter del teclado virtual.
pub(super) const FIXED_KEYS: [KeyCode; 17] = [
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::Enter,
    KeyCode::Escape,
    KeyCode::VolumeUp,
    KeyCode::VolumeDown,
    KeyCode::Mute,
    KeyCode::PlayPause,
    KeyCode::NextTrack,
    KeyCode::PrevTrack,
    KeyCode::Backspace,
    KeyCode::Space,
    KeyCode::Shift,
    KeyCode::BrowserBack,
    KeyCode::BrowserForward,
];

/// Resto de rueda por debajo de una muesca (120 = una muesca). La tira de
/// scroll manda unas decenas de unidades por paquete: con solo `delta / 120`
/// casi nunca llegaba a una muesca y no hacía nada.
#[derive(Default)]
pub(super) struct WheelAcc(i32);

impl WheelAcc {
    /// Suma `delta` y devuelve las muescas enteras acumuladas (con signo;
    /// 0 si aún no hay una entera).
    pub(super) fn push(&mut self, delta: i32) -> i32 {
        self.0 += delta;
        let notches = self.0 / 120;
        self.0 -= notches * 120;
        notches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_apuntado_cae_dentro_de_la_pantalla_objetivo() {
        // Escritorio 3968×2232 con la pantalla de juego en (1920,1080) 2048×1152
        // (el caso real: tres monitores en L)
        let t = [1920.0 / 3968.0, 1080.0 / 2232.0, 2048.0 / 3968.0, 1152.0 / 2232.0];
        let (cx, cy) = map_abs(0.5, 0.5, t);
        // centro de la pantalla objetivo = (2944, 1656) del escritorio
        assert_eq!(cx, (2944.0 / 3968.0 * ABS_MAX as f32).round() as i32);
        assert_eq!(cy, (1656.0 / 2232.0 * ABS_MAX as f32).round() as i32);
        // esquinas: nunca se sale de la pantalla objetivo aunque nx se pase
        let (x0, y0) = map_abs(-1.0, -1.0, t);
        assert_eq!((x0, y0), map_abs(0.0, 0.0, t));
        let (x1, y1) = map_abs(2.0, 2.0, t);
        assert_eq!((x1, y1), map_abs(1.0, 1.0, t));
        assert_eq!(x1, ABS_MAX);
        assert_eq!(y1, ABS_MAX);
        // una sola pantalla: identidad
        assert_eq!(map_abs(0.25, 0.75, [0.0, 0.0, 1.0, 1.0]), (8192, 24575));
    }

    #[test]
    fn norm_in_es_la_inversa_de_map_norm() {
        let t = [1920.0 / 3968.0, 1080.0 / 2232.0, 2048.0 / 3968.0, 1152.0 / 2232.0];
        for (nx, ny) in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0), (0.2, 0.9)] {
            let (x, y) = map_norm(nx, ny, t);
            let (bx, by) = norm_in(t, x, y);
            assert!((bx - nx).abs() < 1e-5 && (by - ny).abs() < 1e-5, "{nx},{ny} → {x},{y} → {bx},{by}");
        }
        // fuera de la pantalla objetivo: se sale de 0..1 (no se recorta)
        let (bx, _) = norm_in(t, 0.0, 0.5);
        assert!(bx < 0.0);
        // identidad con una sola pantalla; un rect degenerado no divide por cero
        assert_eq!(norm_in([0.0, 0.0, 1.0, 1.0], 0.3, 0.7), (0.3, 0.7));
        assert_eq!(norm_in([0.0, 0.0, 0.0, 0.0], 0.3, 0.7), (0.3, 0.7));
    }

    #[test]
    fn la_rueda_acumula_muescas_enteras() {
        let mut acc = WheelAcc::default();
        assert_eq!(acc.push(50), 0);
        assert_eq!(acc.push(50), 0);
        assert_eq!(acc.push(20), 1); // 120 justos
        assert_eq!(acc.push(-130), -1); // -130 → una muesca atrás, sobran -10
        assert_eq!(acc.push(10), 0); // -10 + 10 = 0
        assert_eq!(acc.push(240), 2);
    }
}
