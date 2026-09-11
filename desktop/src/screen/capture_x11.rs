//! Captura en Linux (X11, y XWayland) de la ventana «GamePad View» de Cemu
//! con `GetImage` sobre la propia ventana, redirigida con Composite para que
//! tenga contenido aunque esté tapada. En una sesión Wayland con Cemu nativo
//! no hay ventana X que capturar: se avisa (lanzar Cemu con GDK_BACKEND=x11).

use super::{Capture, RawFrame};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::composite::{ConnectionExt as _, Redirect};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask, ImageFormat, StackMode,
    Window,
};
use x11rb::rust_connection::RustConnection;

pub struct Capturer {
    conn: Option<(RustConnection, usize)>,
    win: Option<Window>,
    last_search: Instant,
    conn_error: Option<String>,
}

impl Capturer {
    pub fn new() -> Self {
        Self { conn: None, win: None, last_search: Instant::now() - Duration::from_secs(10), conn_error: None }
    }

    fn connect(&mut self) -> Result<(), String> {
        if self.conn.is_some() {
            return Ok(());
        }
        if std::env::var_os("DISPLAY").is_none() {
            return Err("Sin sesión X11: en Wayland lanza Cemu con GDK_BACKEND=x11 para la doble pantalla".into());
        }
        match x11rb::connect(None) {
            Ok((c, screen)) => {
                self.conn = Some((c, screen));
                self.conn_error = None;
                Ok(())
            }
            Err(e) => Err(format!("No puedo conectar con X11: {e}")),
        }
    }

    pub fn capture(&mut self) -> Result<Capture, String> {
        self.connect()?;
        if self.win.is_none() {
            if self.last_search.elapsed() < Duration::from_millis(700) {
                return Ok(Capture::NoWindow(no_window_reason()));
            }
            self.last_search = Instant::now();
            self.win = self.find_pad_window();
            if let Some(w) = self.win {
                if let Some((conn, _)) = &self.conn {
                    // con contenido aunque otra ventana la tape
                    let _ = conn.composite_redirect_window(w, Redirect::AUTOMATIC);
                    let _ = conn.flush();
                }
            }
        }
        let Some(win) = self.win else {
            return Ok(Capture::NoWindow(no_window_reason()));
        };
        let (conn, _) = self.conn.as_ref().unwrap();
        let geo = match conn.get_geometry(win).ok().and_then(|c| c.reply().ok()) {
            Some(g) => g,
            None => {
                self.win = None;
                return Ok(Capture::NoWindow(no_window_reason()));
            }
        };
        if geo.width < 8 || geo.height < 8 {
            return Ok(Capture::NoWindow("La ventana GamePad View de Cemu no tiene tamaño".into()));
        }
        let img = match conn
            .get_image(ImageFormat::Z_PIXMAP, win, 0, 0, geo.width, geo.height, !0)
            .ok()
            .and_then(|c| c.reply().ok())
        {
            Some(i) => i,
            None => {
                self.win = None;
                return Ok(Capture::NoWindow("La ventana GamePad View de Cemu no está visible".into()));
            }
        };
        let (w, h) = (geo.width as u32, geo.height as u32);
        let mut bgra = img.data;
        if img.depth != 24 && img.depth != 32 {
            return Err(format!("Profundidad X11 no soportada: {}", img.depth));
        }
        if bgra.len() < (w * h * 4) as usize {
            return Err("Imagen X11 incompleta".into());
        }
        bgra.truncate((w * h * 4) as usize);
        for px in bgra.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        Ok(Capture::Frame(RawFrame { w, h, bgra }))
    }

    /// Ventana cliente con WM_CLASS «Cemu» y título que contiene «GamePad».
    fn find_pad_window(&self) -> Option<Window> {
        let (conn, screen) = self.conn.as_ref()?;
        cemu_windows(conn, *screen).0
    }
}

fn atom(conn: &RustConnection, name: &str) -> Option<Atom> {
    conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok().map(|r| r.atom)
}

fn string_property(conn: &RustConnection, w: Window, prop: impl Into<Atom>, ty: impl Into<Atom>) -> String {
    conn.get_property(false, w, prop, ty, 0, 1024)
        .ok()
        .and_then(|c| c.reply().ok())
        .filter(|r| !r.value.is_empty())
        .map(|r| String::from_utf8_lossy(&r.value).to_lowercase())
        .unwrap_or_default()
}

/// Las ventanas cliente de Cemu (WM_CLASS «cemu»): (GamePad View, principal).
/// La GamePad View lleva «gamepad» en el título; la principal empieza por
/// «cemu».
fn cemu_windows(conn: &RustConnection, screen: usize) -> (Option<Window>, Option<Window>) {
    let Some(root) = conn.setup().roots.get(screen).map(|s| s.root) else {
        return (None, None);
    };
    let (Some(client_list), Some(net_name), Some(utf8)) =
        (atom(conn, "_NET_CLIENT_LIST"), atom(conn, "_NET_WM_NAME"), atom(conn, "UTF8_STRING"))
    else {
        return (None, None);
    };
    let windows: Vec<Window> = conn
        .get_property(false, root, client_list, AtomEnum::WINDOW, 0, u32::MAX)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().map(|v| v.collect()))
        .unwrap_or_default();
    let (mut pad, mut main) = (None, None);
    for w in windows {
        if !string_property(conn, w, AtomEnum::WM_CLASS, AtomEnum::STRING).contains("cemu") {
            continue;
        }
        let mut name = string_property(conn, w, net_name, utf8);
        if name.is_empty() {
            name = string_property(conn, w, AtomEnum::WM_NAME, AtomEnum::STRING);
        }
        if name.contains("gamepad") {
            pad.get_or_insert(w);
        } else if name.starts_with("cemu") {
            main.get_or_insert(w);
        }
    }
    (pad, main)
}

/// Esconde la GamePad View detrás de la ventana principal de Cemu (mejor
/// esfuerzo: el gestor de ventanas decide si atiende a la petición de
/// apilado; la captura con Composite sigue aunque esté tapada) y la saca de
/// la barra de tareas (_NET_WM_STATE_SKIP_TASKBAR).
pub struct Minder {
    conn: Option<(RustConnection, usize)>,
    styled: Option<Window>,
    ticks: u32,
}

impl Minder {
    pub fn new() -> Self {
        Self { conn: None, styled: None, ticks: 0 }
    }

    fn skip_taskbar(&self, pad: Window, on: bool) {
        let Some((conn, screen)) = self.conn.as_ref() else {
            return;
        };
        let Some(root) = conn.setup().roots.get(*screen).map(|s| s.root) else {
            return;
        };
        let (Some(state), Some(taskbar), Some(pager)) = (
            atom(conn, "_NET_WM_STATE"),
            atom(conn, "_NET_WM_STATE_SKIP_TASKBAR"),
            atom(conn, "_NET_WM_STATE_SKIP_PAGER"),
        ) else {
            return;
        };
        // 1 = _NET_WM_STATE_ADD, 0 = _NET_WM_STATE_REMOVE; origen 2 = otra aplicación
        let ev = ClientMessageEvent::new(32, pad, state, [u32::from(on), taskbar, pager, 2, 0]);
        let _ = conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, ev);
        let _ = conn.flush();
    }

    /// Un paso: deja la ventana escondida si se puede. true si lo está.
    pub fn hide(&mut self) -> bool {
        if self.conn.is_none() {
            if std::env::var_os("DISPLAY").is_none() {
                return false;
            }
            self.conn = x11rb::connect(None).ok();
        }
        let Some((conn, screen)) = self.conn.as_ref() else {
            return false;
        };
        let (Some(pad), Some(main)) = cemu_windows(conn, *screen) else {
            self.styled = None;
            return false;
        };
        let Some(root) = conn.setup().roots.get(*screen).map(|s| s.root) else {
            return false;
        };
        let geometry = |w: Window| conn.get_geometry(w).ok().and_then(|c| c.reply().ok());
        let origin = |w: Window| conn.translate_coordinates(w, root, 0, 0).ok().and_then(|c| c.reply().ok());
        let (Some(mg), Some(mo), Some(pg), Some(po)) = (geometry(main), origin(main), geometry(pad), origin(pad)) else {
            return false;
        };
        let main_rc = [
            i32::from(mo.dst_x),
            i32::from(mo.dst_y),
            i32::from(mo.dst_x) + i32::from(mg.width),
            i32::from(mo.dst_y) + i32::from(mg.height),
        ];
        let pad_rc = [
            i32::from(po.dst_x),
            i32::from(po.dst_y),
            i32::from(po.dst_x) + i32::from(pg.width),
            i32::from(po.dst_y) + i32::from(pg.height),
        ];
        let natural = (i32::from(pg.width).max(super::MAX_W as i32), i32::from(pg.height).max(super::MAX_H as i32));
        let Some(want) = super::tuck_rect(main_rc, natural) else {
            return false;
        };
        self.ticks = self.ticks.wrapping_add(1);
        // El apilado no se puede leer de forma fiable: se repite de vez en cuando
        if !super::rect_inside(pad_rc, main_rc) || self.ticks % 4 == 1 {
            let aux = ConfigureWindowAux::new()
                .x(want[0])
                .y(want[1])
                .width(want[2] as u32)
                .height(want[3] as u32)
                .sibling(main)
                .stack_mode(StackMode::BELOW);
            let _ = conn.configure_window(pad, &aux);
            let _ = conn.flush();
        }
        if self.styled != Some(pad) {
            self.skip_taskbar(pad, true);
            self.styled = Some(pad);
        }
        true
    }

    /// Al salir del modo Wii U: la ventana vuelve a la barra de tareas.
    pub fn release(&mut self) {
        if let Some(pad) = self.styled.take() {
            self.skip_taskbar(pad, false);
        }
    }
}

/// En Linux el texto va por uinput (ventana con el foco): no hay camino
/// directo a Cemu.
pub fn type_text(_text: &str) -> bool {
    false
}

fn no_window_reason() -> String {
    if crate::cemu::running_exe().0 {
        "Abre la vista del GamePad en Cemu (Options → Separate GamePad view)".to_owned()
    } else {
        "Cemu no está abierto".to_owned()
    }
}
