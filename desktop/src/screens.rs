//! Disposición de monitores (Linux). El ratón absoluto de uinput cubre el
//! rectángulo envolvente de TODO el escritorio (así lo mapean KWin, mutter,
//! wlroots y X11); con varios monitores, "centro" caería en una esquina y el
//! recorrido se repartiría entre pantallas. Aquí se obtiene la disposición
//! real y el apuntado se mapea a UNA pantalla, la de juego.
//!
//! Wayland: xdg-output, en coordenadas LÓGICAS — las mismas que usa el
//! compositor para mapear el dispositivo (escala incluida). X11: xrandr.

use crate::state::SharedState;
use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub primary: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Layout {
    pub screens: Vec<Screen>,
}

impl Layout {
    /// Rectángulo envolvente (x, y, w, h) de todas las pantallas.
    pub fn bbox(&self) -> (i32, i32, i32, i32) {
        let x0 = self.screens.iter().map(|s| s.x).min().unwrap_or(0);
        let y0 = self.screens.iter().map(|s| s.y).min().unwrap_or(0);
        let x1 = self.screens.iter().map(|s| s.x + s.w).max().unwrap_or(1);
        let y1 = self.screens.iter().map(|s| s.y + s.h).max().unwrap_or(1);
        (x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
    }

    /// Pantalla concreta elegida en Ajustes (`wanted`, por nombre) si sigue
    /// conectada. `""` (o nombre inexistente) = ninguna: el apuntado cubre
    /// TODAS las pantallas (el escritorio entero).
    pub fn target(&self, wanted: &str) -> Option<&Screen> {
        let wanted = std::env::var("PEPOMOTE_SCREEN").unwrap_or_else(|_| wanted.to_owned());
        if wanted.is_empty() {
            return None;
        }
        self.screens.iter().find(|s| s.name == wanted)
    }

    /// Zona de apuntado: rect normalizado dentro del envolvente [x0,y0,w,h]
    /// (0..1), aspect (w/h en px) y ancho en px. Con `wanted` vacío es el
    /// escritorio completo (todas las pantallas); con una pantalla elegida,
    /// esa. El inyector aplica el rect; el motor usa aspect y ancho.
    pub fn pointing(&self, wanted: &str) -> ([f32; 4], f32, f32) {
        let (bx, by, bw, bh) = self.bbox();
        match self.target(wanted) {
            Some(t) => (
                [
                    (t.x - bx) as f32 / bw as f32,
                    (t.y - by) as f32 / bh as f32,
                    t.w as f32 / bw as f32,
                    t.h as f32 / bh as f32,
                ],
                t.w as f32 / t.h.max(1) as f32,
                t.w as f32,
            ),
            // Todas: todo el envolvente, con su aspect real (mapeo isótropo)
            None => ([0.0, 0.0, 1.0, 1.0], bw as f32 / bh as f32, bw as f32),
        }
    }
}

/// Hilo dedicado a la disposición de monitores. Va aparte del hilo de
/// telemetría a propósito: `detect()` hace roundtrips a Wayland y en algún
/// compositor podría tardar; ahí NO puede colgar la inyección del cursor.
/// Publica en el estado compartido la lista de pantallas (para el selector)
/// y el mapeo ya resuelto (`pointing`) para la config vigente.
pub fn watch(shared: SharedState) {
    let _ = std::thread::Builder::new()
        .name("pmp-screens".into())
        .spawn(move || {
            let debug = std::env::var_os("PEPOMOTE_DEBUG").is_some();
            let mut last_desc = String::new();
            loop {
                let wanted = shared.lock().unwrap().config.screen.clone();
                match detect() {
                    Some(layout) if !layout.screens.is_empty() => {
                        let pointing = layout.pointing(&wanted);
                        let list: Vec<(String, i32, i32)> =
                            layout.screens.iter().map(|s| (s.name.clone(), s.w, s.h)).collect();
                        if debug {
                            let desc = format!("{:?} wanted={wanted} -> {:?}", list, pointing);
                            if desc != last_desc {
                                let dst = layout.target(&wanted).map(|t| t.name.clone())
                                    .unwrap_or_else(|| "TODAS".into());
                                eprintln!("[screens] {} monitor(es), apuntando a {dst} -> rect {:.3?} aspect {:.3}",
                                    layout.screens.len(), pointing.0, pointing.1);
                                last_desc = desc;
                            }
                        }
                        let mut s = shared.lock().unwrap();
                        s.screens = list;
                        s.pointing = Some(pointing);
                    }
                    // Sin datos (X11 sin xrandr, Wayland raro): el inyector usa
                    // todo el escritorio. No borramos una lista previa buena.
                    _ => {
                        shared.lock().unwrap().pointing.get_or_insert(([0.0, 0.0, 1.0, 1.0], 16.0 / 9.0, 1920.0));
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        });
}

/// Disposición actual, o None si no hay forma de saberla (entonces se asume
/// una sola pantalla: mapeo identidad).
pub fn detect() -> Option<Layout> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        if let Some(l) = wayland::detect() {
            if !l.screens.is_empty() {
                return Some(l);
            }
        }
    }
    if std::env::var_os("DISPLAY").is_some() {
        let out = Command::new("xrandr").arg("--query").output().ok()?;
        let l = parse_xrandr(&String::from_utf8_lossy(&out.stdout));
        if !l.screens.is_empty() {
            return Some(l);
        }
    }
    None
}

/// `DP-3 connected primary 2560x1440+2400+1350 (normal left …) 597mm x 336mm`
pub fn parse_xrandr(text: &str) -> Layout {
    let mut screens = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(name), Some("connected")) = (it.next(), it.next()) else {
            continue;
        };
        let rest: Vec<&str> = it.collect();
        let primary = rest.first() == Some(&"primary");
        let Some(geom) = rest.iter().find(|t| t.contains('x') && t.contains('+')) else {
            continue; // conectado pero apagado: sin geometría
        };
        let Some((size, pos)) = geom.split_once('+') else { continue };
        let Some((w, h)) = size.split_once('x') else { continue };
        let Some((x, y)) = pos.split_once('+') else { continue };
        let (Ok(w), Ok(h), Ok(x), Ok(y)) = (w.parse(), h.parse(), x.parse(), y.parse()) else {
            continue;
        };
        screens.push(Screen { name: name.to_owned(), x, y, w, h, primary });
    }
    Layout { screens }
}

mod wayland {
    use super::{Layout, Screen};
    use std::collections::BTreeMap;
    use wayland_client::protocol::{wl_output, wl_registry};
    use wayland_client::{delegate_noop, Connection, Dispatch, QueueHandle};
    use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::ZxdgOutputManagerV1;
    use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::{self, ZxdgOutputV1};

    #[derive(Default)]
    pub(super) struct Out {
        pub(super) name: String,
        // Preferente: xdg-output, ya en coordenadas LÓGICAS (escala aplicada),
        // que es como el compositor mapea el dispositivo absoluto.
        pub(super) xdg: Option<(i32, i32, i32, i32)>, // x, y, w, h
        // Fallback (compositores sin xdg-output): wl_output.geometry da la
        // posición y wl_output.mode el tamaño en px FÍSICOS; /scale aproxima
        // lo lógico (exacto sin escalado fraccionario).
        pub(super) wl_pos: Option<(i32, i32)>,
        pub(super) wl_mode: Option<(i32, i32)>,
        pub(super) scale: i32,
    }

    impl Out {
        /// Rect lógico resuelto (x, y, w, h), prefiriendo xdg-output.
        pub(super) fn logical(&self) -> Option<(i32, i32, i32, i32)> {
            if let Some(r) = self.xdg {
                if r.2 > 0 && r.3 > 0 {
                    return Some(r);
                }
            }
            let (x, y) = self.wl_pos?;
            let (w, h) = self.wl_mode?;
            let s = self.scale.max(1);
            Some((x, y, (w / s).max(1), (h / s).max(1)))
        }
    }

    #[derive(Default)]
    struct State {
        mgr: Option<ZxdgOutputManagerV1>,
        wl_outputs: Vec<(u32, wl_output::WlOutput)>,
        outs: BTreeMap<u32, Out>,
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for State {
        fn event(
            state: &mut Self,
            registry: &wl_registry::WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global { name, interface, version } = event {
                match interface.as_str() {
                    "wl_output" => {
                        let o = registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, name);
                        state.wl_outputs.push((name, o));
                        state.outs.entry(name).or_default();
                    }
                    "zxdg_output_manager_v1" => {
                        state.mgr = Some(registry.bind::<ZxdgOutputManagerV1, _, _>(name, version.min(3), qh, ()));
                    }
                    _ => {}
                }
            }
        }
    }

    impl Dispatch<wl_output::WlOutput, u32> for State {
        fn event(
            state: &mut Self,
            _: &wl_output::WlOutput,
            event: wl_output::Event,
            id: &u32,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let o = state.outs.entry(*id).or_default();
            match event {
                // wl_output v4 trae el nombre del conector (HDMI-A-1, eDP-1…);
                // xdg-output también, y cualquiera vale.
                wl_output::Event::Name { name } => {
                    if o.name.is_empty() {
                        o.name = name;
                    }
                }
                // Fallback si no hay xdg-output: posición y tamaño físicos.
                wl_output::Event::Geometry { x, y, .. } => o.wl_pos = Some((x, y)),
                wl_output::Event::Mode { flags, width, height, .. } => {
                    // Solo el modo actual (bit CURRENT); WEnum por compatibilidad
                    let current = match flags {
                        wayland_client::WEnum::Value(f) => f.contains(wl_output::Mode::Current),
                        _ => true,
                    };
                    if current {
                        o.wl_mode = Some((width, height));
                    }
                }
                wl_output::Event::Scale { factor } => o.scale = factor,
                _ => {}
            }
        }
    }

    impl Dispatch<ZxdgOutputV1, u32> for State {
        fn event(
            state: &mut Self,
            _: &ZxdgOutputV1,
            event: zxdg_output_v1::Event,
            id: &u32,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let o = state.outs.entry(*id).or_default();
            let (x, y, w, h) = o.xdg.unwrap_or((0, 0, 0, 0));
            match event {
                zxdg_output_v1::Event::LogicalPosition { x, y } => {
                    o.xdg = Some((x, y, w, h));
                }
                zxdg_output_v1::Event::LogicalSize { width, height } => {
                    o.xdg = Some((x, y, width, height));
                }
                zxdg_output_v1::Event::Name { name } => {
                    if o.name.is_empty() {
                        o.name = name;
                    }
                }
                _ => {}
            }
        }
    }

    delegate_noop!(State: ignore ZxdgOutputManagerV1);

    pub fn detect() -> Option<Layout> {
        let conn = Connection::connect_to_env().ok()?;
        let display = conn.display();
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        let _registry = display.get_registry(&qh, ());
        let mut state = State::default();
        queue.roundtrip(&mut state).ok()?; // globales (wl_output, quizá xdg mgr)
        // xdg-output si el compositor lo ofrece (preferente: da coords lógicas)
        let xdg: Vec<ZxdgOutputV1> = match state.mgr.clone() {
            Some(mgr) => state
                .wl_outputs
                .iter()
                .map(|(id, o)| mgr.get_xdg_output(o, &qh, *id))
                .collect(),
            None => Vec::new(), // sin xdg-output: tiramos de wl_output (fallback)
        };
        // Los eventos de wl_output (geometry/mode/scale) ya llegaron en el
        // primer roundtrip; estos dos recogen la geometría de xdg-output.
        queue.roundtrip(&mut state).ok()?;
        queue.roundtrip(&mut state).ok()?;
        for x in xdg {
            x.destroy();
        }
        for (_, o) in state.wl_outputs.drain(..) {
            o.release();
        }
        let screens = state
            .outs
            .values()
            .filter_map(|o| o.logical().map(|r| (o.name.clone(), r)))
            .enumerate()
            .map(|(i, (name, (x, y, w, h)))| Screen {
                name: if name.is_empty() { format!("Pantalla {}", i + 1) } else { name },
                x,
                y,
                w,
                h,
                primary: false,
            })
            .collect();
        Some(Layout { screens })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XRANDR: &str = "Screen 0: minimum 16 x 16, current 4960 x 2790, maximum 32767 x 32767
DP-1 connected 2400x1350+0+1350 (normal left inverted right x axis y axis) 543mm x 302mm
   2560x1440     59.95*+
DP-2 connected 2400x1350+2400+0 inverted (normal left inverted right x axis y axis) 527mm x 296mm
DP-3 connected primary 2560x1440+2400+1350 (normal left inverted right x axis y axis) 597mm x 336mm
HDMI-1 disconnected (normal left inverted right x axis y axis)
DP-4 connected (normal left inverted right x axis y axis)
";

    #[test]
    fn xrandr_se_parsea_con_primaria_y_apagadas_fuera() {
        let l = parse_xrandr(XRANDR);
        assert_eq!(l.screens.len(), 3, "{:?}", l.screens);
        let p = l.screens.iter().find(|s| s.primary).unwrap();
        assert_eq!((p.name.as_str(), p.x, p.y, p.w, p.h), ("DP-3", 2400, 1350, 2560, 1440));
        assert_eq!(l.bbox(), (0, 0, 4960, 2790));
    }

    #[test]
    fn xrandr_nombres_de_conector_variados() {
        // Cualquier PC: HDMI, eDP (portátil), DVI, VGA, MST (DP-1-2), HDMI-A-1
        let txt = "Screen 0: minimum 320 x 200, current 3840 x 1080, maximum 16384 x 16384
eDP-1 connected primary 1920x1080+0+0 (normal left inverted right x axis y axis) 309mm x 173mm
HDMI-A-1 connected 1920x1080+1920+0 (normal) 530mm x 300mm
DVI-D-1 disconnected (normal left inverted right x axis y axis)
VGA-1 disconnected (normal left inverted right x axis y axis)
";
        let l = parse_xrandr(txt);
        assert_eq!(l.screens.len(), 2, "{:?}", l.screens);
        assert_eq!(l.screens[0].name, "eDP-1");
        assert!(l.screens[0].primary);
        assert_eq!(l.screens[1].name, "HDMI-A-1");
        assert_eq!(l.bbox(), (0, 0, 3840, 1080));
        // el usuario elige HDMI y se confina a ella
        let t = l.target("HDMI-A-1").unwrap();
        assert_eq!((t.x, t.w), (1920, 1920));
        let (norm, _, _) = l.pointing("HDMI-A-1");
        assert_eq!(norm, [0.5, 0.0, 0.5, 1.0]);
    }

    #[test]
    fn xrandr_monitores_apilados_en_vertical_y_con_hueco() {
        // Dos monitores uno encima de otro, distinta anchura (T invertida)
        let txt = "DP-1 connected 2560x1440+0+0 (normal) 600mm x 340mm
HDMI-2 connected primary 1920x1080+320+1440 (normal) 530mm x 300mm
";
        let l = parse_xrandr(txt);
        assert_eq!(l.bbox(), (0, 0, 2560, 2520));
        // pantalla de abajo desplazada: su rect normalizado lo refleja
        let (n, _, _) = l.pointing("HDMI-2");
        let exp = [320.0 / 2560.0, 1440.0 / 2520.0, 1920.0 / 2560.0, 1080.0 / 2520.0];
        for i in 0..4 {
            assert!((n[i] - exp[i]).abs() < 1e-6, "{n:?} vs {exp:?}");
        }
    }

    #[test]
    fn xrandr_vacio_o_todo_desconectado_no_peta() {
        assert!(parse_xrandr("").screens.is_empty());
        assert!(parse_xrandr("HDMI-1 disconnected\nVGA-1 disconnected\n").screens.is_empty());
        // sin pantallas, pointing da identidad sin dividir por cero
        let (norm, _, _) = Layout::default().pointing("");
        assert_eq!(norm, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn fallback_wl_output_resuelve_logico_con_escala() {
        use super::wayland::Out;
        // xdg-output presente: manda (coords lógicas directas)
        let o = Out { name: "DP-1".into(), xdg: Some((0, 0, 2048, 1152)),
                      wl_pos: Some((0, 0)), wl_mode: Some((2560, 1440)), scale: 0 };
        assert_eq!(o.logical(), Some((0, 0, 2048, 1152)));
        // sin xdg-output: wl_output físico / escala 2 = lógico
        let o = Out { name: "HDMI-A-1".into(), xdg: None,
                      wl_pos: Some((1920, 0)), wl_mode: Some((3840, 2160)), scale: 2 };
        assert_eq!(o.logical(), Some((1920, 0, 1920, 1080)));
        // sin escala anunciada (0) = 1
        let o = Out { name: "VGA-1".into(), xdg: None,
                      wl_pos: Some((0, 0)), wl_mode: Some((1024, 768)), scale: 0 };
        assert_eq!(o.logical(), Some((0, 0, 1024, 768)));
        // sin ningún dato: nada (se filtra fuera)
        let o = Out { name: "x".into(), xdg: None, wl_pos: None, wl_mode: None, scale: 1 };
        assert_eq!(o.logical(), None);
    }

    #[test]
    fn todas_las_pantallas_por_defecto_una_sola_si_se_elige() {
        let l = parse_xrandr(XRANDR);
        // "" = todas: rect = escritorio entero, aspect del envolvente
        let (norm, asp, sw) = l.pointing("");
        assert_eq!(norm, [0.0, 0.0, 1.0, 1.0]);
        assert!((asp - 4960.0 / 2790.0).abs() < 1e-6);
        assert_eq!(sw, 4960.0);
        assert!(l.target("").is_none());
        // nombre inexistente → también todas (no una al azar)
        assert!(l.target("NO-EXISTE").is_none());

        // una pantalla concreta: su rect dentro del envolvente
        assert_eq!(l.target("DP-3").unwrap().name, "DP-3");
        let (n, asp3, sw3) = l.pointing("DP-3");
        let exp = [2400.0 / 4960.0, 1350.0 / 2790.0, 2560.0 / 4960.0, 1440.0 / 2790.0];
        for i in 0..4 {
            assert!((n[i] - exp[i]).abs() < 1e-6, "{n:?} vs {exp:?}");
        }
        assert!((asp3 - 2560.0 / 1440.0).abs() < 1e-6);
        assert_eq!(sw3, 2560.0);
    }

    #[test]
    fn una_sola_pantalla_es_identidad() {
        let l = parse_xrandr("eDP-1 connected primary 1920x1080+0+0 (normal) 344mm x 194mm\n");
        let (norm, asp, sw) = l.pointing("");
        assert_eq!(norm, [0.0, 0.0, 1.0, 1.0]);
        assert!((asp - 1920.0 / 1080.0).abs() < 1e-6);
        assert_eq!(sw, 1920.0);
    }
}
