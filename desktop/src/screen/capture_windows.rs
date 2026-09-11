//! Captura en Windows de la ventana «GamePad View» de Cemu con
//! `PrintWindow(PW_CLIENTONLY | PW_RENDERFULLCONTENT)`: DWM pinta el
//! contenido compuesto de la ventana (incluida la salida Vulkan/OpenGL de
//! Cemu) en un DIB, tapada o no. Verificado en el equipo del dueño: 8 ms por
//! fotograma a 655×368.

use super::{Capture, RawFrame};
use std::time::{Duration, Instant};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetMenu, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible,
};

/// PW_CLIENTONLY (1) | PW_RENDERFULLCONTENT (2): solo el área cliente, pintada
/// por DWM (sin esto las ventanas con GPU salen negras).
const PW_FLAGS: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(3);

struct Dib {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    w: u32,
    h: u32,
}

impl Dib {
    fn new(w: u32, h: u32) -> Option<Self> {
        unsafe {
            let screen = GetDC(HWND(std::ptr::null_mut()));
            let dc = CreateCompatibleDC(screen);
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32), // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0, // BI_RGB
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
            let _ = ReleaseDC(HWND(std::ptr::null_mut()), screen);
            let Ok(bmp) = bmp else {
                let _ = DeleteDC(dc);
                return None;
            };
            if bits.is_null() {
                let _ = DeleteObject(HGDIOBJ(bmp.0));
                let _ = DeleteDC(dc);
                return None;
            }
            let old = SelectObject(dc, HGDIOBJ(bmp.0));
            Some(Self { dc, bmp, old, bits: bits as *mut u8, w, h })
        }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.dc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            let _ = DeleteDC(self.dc);
        }
    }
}

pub struct Capturer {
    hwnd: Option<HWND>,
    last_search: Instant,
    dib: Option<Dib>,
}

// HWND/HDC son manejadores opacos: el hilo de captura es el único que los usa.
unsafe impl Send for Capturer {}

impl Capturer {
    pub fn new() -> Self {
        Self { hwnd: None, last_search: Instant::now() - Duration::from_secs(10), dib: None }
    }

    pub fn capture(&mut self) -> Result<Capture, String> {
        if let Some(h) = self.hwnd {
            if unsafe { !IsWindow(h).as_bool() || !IsWindowVisible(h).as_bool() } {
                self.hwnd = None;
            }
        }
        if self.hwnd.is_none() {
            if self.last_search.elapsed() < Duration::from_millis(700) {
                return Ok(Capture::NoWindow(no_window_reason()));
            }
            self.last_search = Instant::now();
            self.hwnd = find_pad_window();
            let Some(_) = self.hwnd else {
                return Ok(Capture::NoWindow(no_window_reason()));
            };
        }
        let hwnd = self.hwnd.unwrap();
        if unsafe { IsIconic(hwnd).as_bool() } {
            return Ok(Capture::NoWindow("La ventana GamePad View de Cemu está minimizada".into()));
        }
        let mut rect = RECT::default();
        unsafe { GetClientRect(hwnd, &mut rect) }.map_err(|e| format!("GetClientRect: {e}"))?;
        let (w, h) = ((rect.right - rect.left).max(0) as u32, (rect.bottom - rect.top).max(0) as u32);
        if w < 8 || h < 8 {
            return Ok(Capture::NoWindow("La ventana GamePad View de Cemu no tiene tamaño".into()));
        }
        if self.dib.as_ref().is_none_or(|d| d.w != w || d.h != h) {
            self.dib = Dib::new(w, h);
        }
        let Some(dib) = self.dib.as_ref() else {
            return Err("No puedo crear el bitmap de captura".into());
        };
        let ok = unsafe { PrintWindow(hwnd, dib.dc, PW_FLAGS) };
        if !ok.as_bool() {
            self.hwnd = None;
            return Ok(Capture::NoWindow("Cemu no deja capturar la ventana GamePad View".into()));
        }
        unsafe {
            let _ = GdiFlush();
        }
        let len = (w * h * 4) as usize;
        let mut bgra = vec![0u8; len];
        unsafe { std::ptr::copy_nonoverlapping(dib.bits, bgra.as_mut_ptr(), len) };
        for px in bgra.as_chunks_mut::<4>().0 {
            px[3] = 255; // el alfa del DIB no significa nada
        }
        Ok(Capture::Frame(RawFrame { w, h, bgra }))
    }
}

fn no_window_reason() -> String {
    if crate::cemu::running_exe().0 {
        "Abre la vista del GamePad en Cemu (Options → Separate GamePad view)".to_owned()
    } else {
        "Cemu no está abierto".to_owned()
    }
}

/// Nombre del ejecutable de un proceso (en minúsculas), si se puede saber.
fn exe_name(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(|s| s.to_lowercase())
    }
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let list = &mut *(lparam.0 as *mut Vec<(HWND, String, u32)>);
    if IsWindowVisible(hwnd).as_bool() {
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf) as usize;
        let title = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        list.push((hwnd, title, pid));
    }
    BOOL(1)
}

/// La ventana «GamePad View» de Cemu: una ventana visible del proceso
/// Cemu.exe cuyo título contiene «GamePad» («GamePad View - FPS: 60.00»
/// mientras juega); si la traducción no lo lleva, la ventana de Cemu que no
/// tiene barra de menú (la principal sí).
pub fn find_pad_window() -> Option<HWND> {
    let mut list: Vec<(HWND, String, u32)> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut list as *mut _ as isize));
    }
    let mut pid_cache: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();
    let cemu: Vec<&(HWND, String, u32)> = list
        .iter()
        .filter(|(_, _, pid)| {
            *pid_cache
                .entry(*pid)
                .or_insert_with(|| exe_name(*pid).is_some_and(|n| n.starts_with("cemu") && n.ends_with(".exe")))
        })
        .collect();
    if let Some((h, _, _)) = cemu.iter().find(|(_, t, _)| t.to_lowercase().contains("gamepad")) {
        return Some(*h);
    }
    cemu.iter()
        .find(|(h, _, _)| unsafe { GetMenu(*h).0.is_null() })
        .map(|(h, _, _)| *h)
}
