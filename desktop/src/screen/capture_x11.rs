//! Captura en Linux (X11, y XWayland) de la ventana «GamePad View» de Cemu
//! con `GetImage` sobre la propia ventana, redirigida con Composite para que
//! tenga contenido aunque esté tapada. En una sesión Wayland con Cemu nativo
//! no hay ventana X que capturar: se avisa (lanzar Cemu con GDK_BACKEND=x11).

use super::{Capture, RawFrame};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::composite::{ConnectionExt as _, Redirect};
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, ImageFormat, Window};
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
        let root = conn.setup().roots.get(*screen)?.root;
        let atom = |name: &str| conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok().map(|r| r.atom);
        let client_list = atom("_NET_CLIENT_LIST")?;
        let net_name = atom("_NET_WM_NAME")?;
        let utf8 = atom("UTF8_STRING")?;
        let windows: Vec<Window> = conn
            .get_property(false, root, client_list, AtomEnum::WINDOW, 0, u32::MAX)
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .collect();
        for w in windows {
            let class = conn
                .get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| String::from_utf8_lossy(&r.value).to_lowercase())
                .unwrap_or_default();
            if !class.contains("cemu") {
                continue;
            }
            let name = conn
                .get_property(false, w, net_name, utf8, 0, 1024)
                .ok()
                .and_then(|c| c.reply().ok())
                .filter(|r| !r.value.is_empty())
                .or_else(|| {
                    conn.get_property(false, w, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 1024)
                        .ok()
                        .and_then(|c| c.reply().ok())
                })
                .map(|r| String::from_utf8_lossy(&r.value).to_lowercase())
                .unwrap_or_default();
            if name.contains("gamepad") {
                return Some(w);
            }
        }
        None
    }
}

fn no_window_reason() -> String {
    if crate::cemu::running_exe().0 {
        "Abre la vista del GamePad en Cemu (Options → Separate GamePad view)".to_owned()
    } else {
        "Cemu no está abierto".to_owned()
    }
}
