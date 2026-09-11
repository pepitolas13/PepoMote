//! Captura en Windows de la ventana «GamePad View» de Cemu.
//!
//! Camino principal: Windows.Graphics.Capture (lo que usa OBS para las
//! ventanas con GPU): DWM entrega cada fotograma compuesto de la ventana en
//! cuanto cambia, tapada o no, y siempre completo. Se recorta al área
//! cliente (fuera la barra de título).
//!
//! Respaldo (Windows 10 antiguos o si WGC falla): `PrintWindow(PW_CLIENTONLY
//! | PW_RENDERFULLCONTENT)` sobre un DIB. Funciona, pero con ventanas Vulkan
//! una de cada tres capturas sale en blanco (medido en el equipo del dueño:
//! el filtro de capturas uniformes de screen/mod.rs las descarta).

use super::{Capture, RawFrame};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_SHIFT;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetClientRect, GetMenu, GetWindow, GetWindowLongW, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed, PostMessageW,
    SetWindowLongW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GW_HWNDPREV, HWND_BOTTOM, SWP_HIDEWINDOW,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, WM_CHAR,
    WM_KEYDOWN, WM_KEYUP, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings, MinimumUpdateIntervalSettings,
    SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window as WgcWindow;

/// PW_CLIENTONLY (1) | PW_RENDERFULLCONTENT (2): solo el área cliente, pintada
/// por DWM (sin esto las ventanas con GPU salen negras).
const PW_FLAGS: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(3);

// ---------------------------------------------------------------------------
// Windows.Graphics.Capture
// ---------------------------------------------------------------------------

/// Recorte del fotograma de WGC (que incluye la barra de título) al área
/// cliente, en píxeles del fotograma.
#[derive(Clone, Copy, Default)]
struct Crop {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

/// Lo que comparte el hilo de WGC con el hilo de captura.
#[derive(Default)]
struct WgcShared {
    latest: Mutex<Option<(u64, RawFrame)>>,
    id: AtomicU64,
    closed: AtomicBool,
    crop: Mutex<Crop>,
}

struct WgcHandler {
    shared: Arc<WgcShared>,
    scratch: Vec<u8>,
}

type WgcError = Box<dyn std::error::Error + Send + Sync>;

impl GraphicsCaptureApiHandler for WgcHandler {
    type Flags = Arc<WgcShared>;
    type Error = WgcError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { shared: ctx.flags, scratch: Vec::new() })
    }

    fn on_frame_arrived(&mut self, frame: &mut Frame, _control: InternalCaptureControl) -> Result<(), Self::Error> {
        let (fw, fh) = (frame.width(), frame.height());
        if fw == 0 || fh == 0 {
            return Ok(());
        }
        let c = *self.shared.crop.lock().unwrap_or_else(|e| e.into_inner());
        let (x0, y0, x1, y1) = (c.x0.min(fw), c.y0.min(fh), c.x1.min(fw), c.y1.min(fh));
        let cropped = x1 > x0 + 8 && y1 > y0 + 8;
        let buf = if cropped { frame.buffer_crop(x0, y0, x1, y1)? } else { frame.buffer()? };
        let (w, h) = (buf.width(), buf.height());
        let mut bgra = buf.as_nopadding_buffer(&mut self.scratch).to_vec();
        for px in bgra.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        let id = self.shared.id.fetch_add(1, Ordering::Relaxed) + 1;
        *self.shared.latest.lock().unwrap_or_else(|e| e.into_inner()) = Some((id, RawFrame { w, h, bgra }));
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.shared.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

struct Wgc {
    hwnd: HWND,
    shared: Arc<WgcShared>,
    control: Option<CaptureControl<WgcHandler, WgcError>>,
}

impl Drop for Wgc {
    fn drop(&mut self) {
        if let Some(c) = self.control.take() {
            let _ = c.stop();
        }
    }
}

fn start_wgc(hwnd: HWND) -> Result<Wgc, String> {
    let shared = Arc::new(WgcShared::default());
    let settings = Settings::new(
        WgcWindow::from_raw_hwnd(hwnd.0),
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Exclude,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        shared.clone(),
    );
    let control = WgcHandler::start_free_threaded(settings).map_err(|e| format!("{e:?}"))?;
    Ok(Wgc { hwnd, shared, control: Some(control) })
}

/// Área cliente de la ventana dentro del fotograma de WGC (que cubre el
/// marco visible de la ventana, DWMWA_EXTENDED_FRAME_BOUNDS).
fn client_crop(hwnd: HWND) -> Option<Crop> {
    unsafe {
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc).ok()?;
        let mut pt = POINT { x: 0, y: 0 };
        if !ClientToScreen(hwnd, &mut pt).as_bool() {
            return None;
        }
        let mut ext = RECT::default();
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut ext as *mut RECT as *mut core::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .ok()?;
        let x0 = (pt.x - ext.left).max(0) as u32;
        let y0 = (pt.y - ext.top).max(0) as u32;
        Some(Crop { x0, y0, x1: x0 + rc.right.max(0) as u32, y1: y0 + rc.bottom.max(0) as u32 })
    }
}

// ---------------------------------------------------------------------------
// Respaldo: PrintWindow
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Capturer
// ---------------------------------------------------------------------------

pub struct Capturer {
    hwnd: Option<HWND>,
    last_search: Instant,
    dib: Option<Dib>,
    wgc: Option<Wgc>,
    /// WGC no arranca en este equipo: PrintWindow para siempre.
    wgc_broken: bool,
    last_wgc_id: u64,
}

// HWND/HDC son manejadores opacos: el hilo de captura es el único que los usa.
unsafe impl Send for Capturer {}

impl Capturer {
    pub fn new() -> Self {
        Self {
            hwnd: None,
            last_search: Instant::now() - Duration::from_secs(10),
            dib: None,
            wgc: None,
            wgc_broken: std::env::var_os("PEPOMOTE_NO_WGC").is_some(),
            last_wgc_id: 0,
        }
    }

    pub fn capture(&mut self) -> Result<Capture, String> {
        if let Some(h) = self.hwnd {
            if unsafe { !IsWindow(h).as_bool() || !IsWindowVisible(h).as_bool() } {
                self.hwnd = None;
            }
        }
        if self.hwnd.is_none() {
            self.wgc = None;
            if self.last_search.elapsed() < Duration::from_millis(700) {
                return Ok(Capture::NoWindow(no_window_reason()));
            }
            self.last_search = Instant::now();
            self.hwnd = find_pad_window();
            if self.hwnd.is_none() {
                return Ok(Capture::NoWindow(no_window_reason()));
            }
        }
        let hwnd = self.hwnd.unwrap();
        if unsafe { IsIconic(hwnd).as_bool() } {
            // Minimizada no se puede capturar (ni WGC ni PrintWindow): se
            // restaura sin activarla y se manda al fondo, que no estorbe
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            return Ok(Capture::NoWindow("La ventana GamePad View de Cemu estaba minimizada: la abro al fondo".into()));
        }
        if !self.wgc_broken {
            return self.capture_wgc(hwnd);
        }
        self.capture_print_window(hwnd)
    }

    fn capture_wgc(&mut self, hwnd: HWND) -> Result<Capture, String> {
        if self.wgc.as_ref().is_some_and(|w| w.hwnd != hwnd) {
            self.wgc = None;
        }
        if self.wgc.is_none() {
            match start_wgc(hwnd) {
                Ok(w) => {
                    self.wgc = Some(w);
                    self.last_wgc_id = 0;
                }
                Err(e) => {
                    // Sin Windows.Graphics.Capture (Windows 10 antiguo): PrintWindow
                    eprintln!("[screen] WGC no disponible ({e}); uso PrintWindow");
                    self.wgc_broken = true;
                    return self.capture_print_window(hwnd);
                }
            }
        }
        let wgc = self.wgc.as_mut().unwrap();
        if let Some(c) = client_crop(hwnd) {
            *wgc.shared.crop.lock().unwrap_or_else(|e| e.into_inner()) = c;
        }
        if wgc.shared.closed.load(Ordering::SeqCst) || wgc.control.as_ref().is_some_and(|c| c.is_finished()) {
            // la ventana se cerró (o el hilo de WGC murió): a buscarla de nuevo
            self.wgc = None;
            self.hwnd = None;
            return Ok(Capture::NoWindow(no_window_reason()));
        }
        let latest = wgc.shared.latest.lock().unwrap_or_else(|e| e.into_inner()).take();
        match latest {
            Some((id, frame)) if id > self.last_wgc_id => {
                self.last_wgc_id = id;
                Ok(Capture::Frame(frame))
            }
            _ => Ok(Capture::Unchanged),
        }
    }

    fn capture_print_window(&mut self, hwnd: HWND) -> Result<Capture, String> {
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

/// Ventanas visibles de nivel superior del proceso Cemu.exe: (ventana, título).
fn cemu_toplevels() -> Vec<(HWND, String)> {
    let mut list: Vec<(HWND, String, u32)> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut list as *mut _ as isize));
    }
    let mut pid_cache: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();
    list.into_iter()
        .filter(|(_, _, pid)| {
            *pid_cache
                .entry(*pid)
                .or_insert_with(|| exe_name(*pid).is_some_and(|n| n.starts_with("cemu") && n.ends_with(".exe")))
        })
        .map(|(h, t, _)| (h, t))
        .collect()
}

/// Las dos ventanas de Cemu: (GamePad View, principal).
///
/// La GamePad View es la ventana cuyo título contiene «GamePad» («GamePad
/// View - FPS: 60.00» mientras juega); si la traducción no lo lleva, la que
/// no tiene barra de menú. La principal es la que empieza por «Cemu» (con el
/// juego cargado: «Cemu 2.6 - FPS: … - Nombre del juego»), si no la que tiene
/// barra de menú.
fn cemu_windows() -> (Option<HWND>, Option<HWND>) {
    let wins = cemu_toplevels();
    let has_menu = |h: &HWND| unsafe { !GetMenu(*h).0.is_null() };
    let pad = wins
        .iter()
        .find(|(_, t)| t.to_lowercase().contains("gamepad"))
        .or_else(|| wins.iter().find(|(h, _)| !has_menu(h)))
        .map(|(h, _)| *h);
    let main = wins
        .iter()
        .filter(|(h, _)| Some(*h) != pad)
        .find(|(_, t)| t.to_lowercase().starts_with("cemu"))
        .or_else(|| wins.iter().filter(|(h, _)| Some(*h) != pad).find(|(h, _)| has_menu(h)))
        .map(|(h, _)| *h);
    (pad, main)
}

/// La ventana «GamePad View» de Cemu.
pub fn find_pad_window() -> Option<HWND> {
    cemu_windows().0
}

// ---------------------------------------------------------------------------
// Ventana escondida en el PC
// ---------------------------------------------------------------------------

/// Tamaño exterior natural de la GamePad View (854×480 de área cliente más
/// marco y barra de título); si Cemu la abre más grande se respeta.
const NATURAL_W: i32 = 854 + 16;
const NATURAL_H: i32 = 480 + 39;

/// ¿`upper` está por encima de `lower` en el orden Z? (se sube desde `lower`)
fn is_above(upper: HWND, lower: HWND) -> bool {
    let mut h = lower;
    for _ in 0..4096 {
        h = match unsafe { GetWindow(h, GW_HWNDPREV) } {
            Ok(h) if !h.0.is_null() => h,
            _ => return false,
        };
        if h == upper {
            return true;
        }
    }
    false
}

/// Oculta y vuelve a mostrar sin tocar el orden Z ni el foco (la barra de
/// tareas solo relee los estilos de la ventana al mostrarla).
unsafe fn reshow(hwnd: HWND) {
    let _ = SetWindowPos(hwnd, HWND::default(), 0, 0, 0, 0, SWP_HIDEWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    let _ = SetWindowPos(hwnd, HWND::default(), 0, 0, 0, 0, SWP_SHOWWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
}

/// Esconde la GamePad View de Cemu mientras un móvil hace de GamePad: dentro
/// del rectángulo de la ventana principal, justo debajo de ella en el orden Z
/// y fuera de la barra de tareas y de Alt+Tab (WS_EX_TOOLWINDOW). La captura
/// sigue: Windows.Graphics.Capture compone la ventana aunque esté tapada
/// (fuera de la pantalla, transparente o minimizada no: medido).
pub struct Minder {
    /// Ventana a la que se le quitó la barra de tareas (para devolvérsela).
    styled: Option<HWND>,
    /// Tamaño exterior natural de la ventana (antes de encogerla para que quepa).
    natural: Option<(HWND, i32, i32)>,
}

impl Minder {
    pub fn new() -> Self {
        Self { styled: None, natural: None }
    }

    /// Un paso: deja la ventana escondida si se puede. true si lo está.
    pub fn hide(&mut self) -> bool {
        let (Some(pad), Some(main)) = cemu_windows() else {
            self.styled = None;
            self.natural = None;
            return false;
        };
        unsafe {
            if IsIconic(main).as_bool() {
                // Cemu minimizado: el usuario está a otra cosa; no se toca
                return false;
            }
            if IsIconic(pad).as_bool() || IsZoomed(pad).as_bool() {
                let _ = ShowWindow(pad, SW_SHOWNOACTIVATE);
            }
            if self.styled != Some(pad) {
                let ex = GetWindowLongW(pad, GWL_EXSTYLE) as u32;
                if ex & WS_EX_TOOLWINDOW.0 == 0 {
                    SetWindowLongW(pad, GWL_EXSTYLE, ((ex | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0) as i32);
                    reshow(pad);
                }
                self.styled = Some(pad);
            }
            let (mut m, mut p) = (RECT::default(), RECT::default());
            if GetWindowRect(main, &mut m).is_err() || GetWindowRect(pad, &mut p).is_err() {
                return false;
            }
            let natural = match self.natural {
                Some((h, w, hh)) if h == pad => (w, hh),
                _ => {
                    let n = ((p.right - p.left).max(NATURAL_W), (p.bottom - p.top).max(NATURAL_H));
                    self.natural = Some((pad, n.0, n.1));
                    n
                }
            };
            let main_rc = [m.left, m.top, m.right, m.bottom];
            let Some(want) = super::tuck_rect(main_rc, natural) else {
                return false;
            };
            if !super::rect_inside([p.left, p.top, p.right, p.bottom], main_rc) || !is_above(main, pad) {
                let _ = SetWindowPos(pad, main, want[0], want[1], want[2], want[3], SWP_NOACTIVATE);
            }
            true
        }
    }

    /// Al salir del modo Wii U: la ventana vuelve a la barra de tareas (se
    /// queda donde está).
    pub fn release(&mut self) {
        let Some(pad) = self.styled.take() else {
            return;
        };
        self.natural = None;
        unsafe {
            if !IsWindow(pad).as_bool() {
                return;
            }
            let ex = GetWindowLongW(pad, GWL_EXSTYLE) as u32;
            if ex & WS_EX_TOOLWINDOW.0 != 0 {
                SetWindowLongW(pad, GWL_EXSTYLE, (ex & !WS_EX_TOOLWINDOW.0) as i32);
                reshow(pad);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Teclado: texto del móvil al teclado en pantalla de Cemu
// ---------------------------------------------------------------------------

unsafe extern "system" fn child_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let list = &mut *(lparam.0 as *mut Vec<(HWND, i64)>);
    if IsWindowVisible(hwnd).as_bool() {
        let mut rc = RECT::default();
        if GetClientRect(hwnd, &mut rc).is_ok() {
            list.push((hwnd, rc.right as i64 * rc.bottom as i64));
        }
    }
    BOOL(1)
}

/// El lienzo de render de una ventana de Cemu (donde van las teclas): su
/// hijo visible más grande.
fn canvas_of(frame: HWND) -> Option<HWND> {
    let mut kids: Vec<(HWND, i64)> = Vec::new();
    unsafe {
        let _ = EnumChildWindows(frame, Some(child_cb), LPARAM(&mut kids as *mut _ as isize));
    }
    kids.into_iter().max_by_key(|(_, area)| *area).map(|(h, _)| h)
}

/// Teclea en Cemu: WM_CHAR al lienzo de la GamePad View (o al de la
/// principal). Su teclado en pantalla solo atiende a teclas (13 = aceptar,
/// 8 = borrar, el resto texto), no a toques. false si no hay Cemu a la vista.
pub fn type_text(text: &str) -> bool {
    let (pad, main) = cemu_windows();
    let Some(canvas) = pad.and_then(canvas_of).or_else(|| main.and_then(canvas_of)) else {
        return false;
    };
    unsafe {
        // Un keydown que Cemu no procesa: wxWidgets descarta el WM_CHAR que
        // sigue a un keydown «procesado» (p. ej. F11 sin WM_CHAR después)
        let _ = PostMessageW(canvas, WM_KEYDOWN, WPARAM(VK_SHIFT.0 as usize), LPARAM(0));
        let _ = PostMessageW(canvas, WM_KEYUP, WPARAM(VK_SHIFT.0 as usize), LPARAM(0xC000_0000u32 as isize));
        for c in text.chars() {
            let c = match c {
                '\n' => '\r',
                '\u{7f}' => '\u{8}',
                '\r' => continue,
                c => c,
            };
            let mut units = [0u16; 2];
            for u in c.encode_utf16(&mut units) {
                let _ = PostMessageW(canvas, WM_CHAR, WPARAM(*u as usize), LPARAM(1));
            }
        }
    }
    true
}
