//! Doble pantalla en macOS: captura la ventana «GamePad View» de Cemu con
//! CGWindowList (Quartz). Exige «Grabación de pantalla»: sin ese permiso ni
//! siquiera se ven los títulos de las ventanas, y macOS solo lo aplica al
//! reiniciar la app. La GamePad View sigue visible en el Mac (mover ventanas
//! ajenas exigiría más permisos): se captura aunque otra ventana la tape,
//! pero no si está minimizada.

use super::{Capture, RawFrame};
use crate::tr;
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_graphics::image::CGImage;
use core_graphics::window::{
    self, kCGNullWindowID, kCGWindowImageBoundsIgnoreFraming, kCGWindowImageNominalResolution,
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionAll, kCGWindowListOptionIncludingWindow, CGWindowID,
};
use std::time::{Duration, Instant};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGImageGetBitmapInfo(image: core_graphics::sys::CGImageRef) -> u32;
}

/// CGBitmapInfo de la imagen (el crate no lo expone; el puntero sí, por `ForeignType`).
fn bitmap_info(img: &CGImage) -> u32 {
    use foreign_types::ForeignType;
    unsafe { CGImageGetBitmapInfo(img.as_ptr()) }
}

const ALPHA_INFO_MASK: u32 = 0x1F;
const ALPHA_PREMULTIPLIED_LAST: u32 = 1;
const ALPHA_LAST: u32 = 3;
const ALPHA_NONE_SKIP_LAST: u32 = 5;
const BYTE_ORDER_MASK: u32 = 0x7000;
const BYTE_ORDER_32_LITTLE: u32 = 2 << 12;

/// Ventanas de Cemu: la GamePad View, la principal y si hay alguna.
struct CemuWindows {
    pad: Option<CGWindowID>,
    #[allow(dead_code)]
    main: Option<CGWindowID>,
    any: bool,
}

/// Recorre las ventanas del sistema. Dueño «cemu…», capa 0 (ni menús ni
/// tooltips); la GamePad View es la que lleva «gamepad» en el título (sin el
/// permiso de Grabación de pantalla los títulos vienen vacíos).
fn cemu_windows() -> CemuWindows {
    let mut out = CemuWindows { pad: None, main: None, any: false };
    let Some(list) = window::copy_window_info(kCGWindowListOptionAll | kCGWindowListExcludeDesktopElements, kCGNullWindowID)
    else {
        return out;
    };
    for i in 0..list.len() {
        let Some(item) = list.get(i) else { continue };
        let dict: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
        let string = |k: &str| {
            dict.find(CFString::new(k))
                .and_then(|v| v.downcast::<CFString>())
                .map(|s| s.to_string())
        };
        let number = |k: &str| {
            dict.find(CFString::new(k))
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_i64())
        };
        let owner = string("kCGWindowOwnerName").unwrap_or_default().to_lowercase();
        if !owner.starts_with("cemu") {
            continue;
        }
        if number("kCGWindowLayer").unwrap_or(0) != 0 {
            continue;
        }
        let Some(id) = number("kCGWindowNumber") else { continue };
        out.any = true;
        let title = string("kCGWindowName").unwrap_or_default().to_lowercase();
        if title.contains("gamepad") {
            out.pad.get_or_insert(id as CGWindowID);
        } else if title.starts_with("cemu") {
            out.main.get_or_insert(id as CGWindowID);
        }
    }
    out
}

/// Píxeles de CGImage (32 bpp, `stride` bytes por fila) → BGRA contiguo con
/// alfa 255. Quartz puede entregar ARGB en big-endian (orden por defecto) o
/// BGRA en little-endian (lo habitual en Apple Silicon), con el alfa primero
/// o último: `info` es el CGBitmapInfo de la imagen.
pub(crate) fn to_bgra(bytes: &[u8], w: usize, h: usize, stride: usize, info: u32) -> Vec<u8> {
    let alpha = info & ALPHA_INFO_MASK;
    let alpha_last = matches!(alpha, ALPHA_PREMULTIPLIED_LAST | ALPHA_LAST | ALPHA_NONE_SKIP_LAST);
    let little = info & BYTE_ORDER_MASK == BYTE_ORDER_32_LITTLE;
    // Orden en memoria de (b, g, r) según el formato lógico y el endianness
    let (ib, ig, ir) = match (alpha_last, little) {
        (false, true) => (0, 1, 2),  // ARGB lógico, little-endian: B G R A
        (false, false) => (3, 2, 1), // ARGB big-endian: A R G B
        (true, true) => (1, 2, 3),   // RGBA lógico, little-endian: A B G R
        (true, false) => (2, 1, 0),  // RGBA big-endian: R G B A
    };
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = &bytes[y * stride..y * stride + w * 4];
        for px in row.chunks_exact(4) {
            out.extend_from_slice(&[px[ib], px[ig], px[ir], 255]);
        }
    }
    out
}

pub struct Capturer {
    win: Option<CGWindowID>,
    searched: Instant,
}

impl Capturer {
    pub fn new() -> Self {
        Self { win: None, searched: Instant::now() - Duration::from_secs(10) }
    }

    pub fn capture(&mut self) -> Result<Capture, String> {
        if !crate::macos::screen_capture_allowed() {
            crate::macos::screen_capture_request_once();
            return Ok(Capture::NoWindow(tr!("screen.mac_permission").to_owned()));
        }
        if self.win.is_none() {
            if self.searched.elapsed() < Duration::from_millis(700) {
                return Ok(Capture::NoWindow(tr!("screen.open_view").to_owned()));
            }
            self.searched = Instant::now();
            let w = cemu_windows();
            self.win = w.pad;
            if self.win.is_none() {
                let why = if w.any { tr!("screen.open_view") } else { tr!("screen.cemu_closed") };
                return Ok(Capture::NoWindow(why.to_owned()));
            }
        }
        let Some(id) = self.win else {
            return Ok(Capture::NoWindow(tr!("screen.cemu_closed").to_owned()));
        };
        // CGRectNull: el rect de la propia ventana
        let null_rect = CGRect::new(&CGPoint::new(f64::INFINITY, f64::INFINITY), &CGSize::new(0.0, 0.0));
        let Some(img) = window::create_image(
            null_rect,
            kCGWindowListOptionIncludingWindow,
            id,
            kCGWindowImageBoundsIgnoreFraming | kCGWindowImageNominalResolution,
        ) else {
            // cerrada o minimizada: se vuelve a buscar
            self.win = None;
            return Ok(Capture::NoWindow(tr!("screen.not_visible").to_owned()));
        };
        let (w, h) = (img.width(), img.height());
        if w < 8 || h < 8 {
            return Ok(Capture::NoWindow(tr!("screen.no_size").to_owned()));
        }
        if img.bits_per_pixel() != 32 {
            return Err(format!("formato de imagen inesperado ({} bpp)", img.bits_per_pixel()));
        }
        let info = bitmap_info(&img);
        let stride = img.bytes_per_row();
        let data = img.data();
        let bytes = data.bytes();
        if bytes.len() < (h - 1) * stride + w * 4 {
            return Err("la captura viene incompleta".to_owned());
        }
        let bgra = to_bgra(bytes, w, h, stride, info);
        Ok(Capture::Frame(RawFrame { w: w as u32, h: h as u32, bgra }))
    }
}

/// No se esconde la GamePad View: sigue visible en el Mac.
pub struct Minder;

impl Minder {
    pub fn new() -> Self {
        Self
    }

    pub fn hide(&mut self) -> bool {
        false
    }

    pub fn release(&mut self) {}
}

/// El texto para Cemu va por el inyector del SO (Cemu tiene que estar en
/// primer plano), como en X11.
pub fn type_text(_text: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convierte_a_bgra_en_los_cuatro_ordenes() {
        // un píxel (R=1, G=2, B=3, A=9) en cada formato, con una fila de relleno (stride 8)
        let cases: [(u32, [u8; 4]); 4] = [
            (2 | BYTE_ORDER_32_LITTLE, [3, 2, 1, 9]), // ARGB little (kCGImageAlphaPremultipliedFirst): B G R A
            (2, [9, 1, 2, 3]),                        // ARGB big: A R G B
            (1 | BYTE_ORDER_32_LITTLE, [9, 3, 2, 1]), // RGBA little (PremultipliedLast): A B G R
            (1, [1, 2, 3, 9]),                        // RGBA big: R G B A
        ];
        for (info, px) in cases {
            let bytes = [px[0], px[1], px[2], px[3], 0, 0, 0, 0];
            assert_eq!(to_bgra(&bytes, 1, 1, 8, info), vec![3, 2, 1, 255], "info {info:#x}");
        }
        // dos filas con stride mayor que la anchura útil
        let bytes = [3, 2, 1, 9, 7, 7, 7, 7, 6, 5, 4, 9, 7, 7, 7, 7];
        assert_eq!(to_bgra(&bytes, 1, 2, 8, 2 | BYTE_ORDER_32_LITTLE), vec![3, 2, 1, 255, 6, 5, 4, 255]);
    }
}
