//! Doble pantalla del Wii U: captura la ventana «GamePad View» de Cemu (la
//! segunda pantalla), la comprime en JPEG y la sirve por el canal de pantalla
//! (net/screen.rs) al móvil que hace de GamePad.
//!
//! Un único hilo de captura, vivo solo mientras hay algún móvil suscrito:
//! captura a tope de FPS_MAX, descarta los fotogramas idénticos, reduce a lo
//! que pida el móvil (nunca más de 854×480, la resolución del GamePad) y deja
//! el JPEG más reciente en `latest`; cada cliente manda siempre el último
//! (una imagen en vuelo: la latencia no se acumula aunque el Wi-Fi vaya
//! justo).

#[cfg(windows)]
#[path = "capture_windows.rs"]
mod capture;
#[cfg(target_os = "linux")]
#[path = "capture_x11.rs"]
mod capture;
#[cfg(not(any(windows, target_os = "linux")))]
#[path = "capture_none.rs"]
mod capture;

use crate::state::SharedState;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Resolución nativa de la pantalla del GamePad.
pub const MAX_W: u32 = 854;
pub const MAX_H: u32 = 480;
/// Tope de captura. La pantalla del GamePad a 30 fps ya va fluida y cabe en
/// cualquier Wi-Fi; el control de flujo por confirmación baja de ahí solo.
const FPS_MAX: u32 = 30;
const DEFAULT_QUALITY: u8 = 70;

/// Fotograma capturado: BGRA de 8 bits, filas contiguas.
pub struct RawFrame {
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

/// Resultado de una captura.
pub enum Capture {
    Frame(RawFrame),
    /// La ventana no ha cambiado desde la última captura (captura por eventos).
    Unchanged,
    /// No hay ventana que capturar, con el motivo legible para el móvil.
    NoWindow(String),
}

/// ¿Fotograma de (casi) un solo color? Histograma de una muestra de píxeles
/// (color cuantizado a 4 bits por canal): uniforme si un color se lleva el
/// 96 %. Se mira el color dominante y no el primer píxel porque las esquinas
/// redondeadas de Windows 11 son negras en una ventana toda gris.
pub fn is_uniform(f: &RawFrame) -> bool {
    let mut bins = [0u32; 4096];
    let mut total = 0u32;
    for p in f.bgra.chunks_exact(4).step_by(29) {
        let key = ((p[0] as usize >> 4) << 8) | ((p[1] as usize >> 4) << 4) | (p[2] as usize >> 4);
        bins[key] += 1;
        total += 1;
    }
    if total == 0 {
        return true;
    }
    let max = bins.iter().copied().max().unwrap_or(0);
    max * 100 >= total * 96
}

/// Filtro de capturas fallidas: con la ventana Vulkan de Cemu, tanto
/// PrintWindow como Windows.Graphics.Capture entregan de vez en cuando el
/// fondo gris de la ventana sin el juego, mezclado con fotogramas buenos (a
/// veces varios seguidos). Un fotograma uniforme solo pasa cuando llevamos
/// un rato (CONFIRM) sin ver ninguno bueno —una pantalla en negro/blanco de
/// verdad, con ese retraso— o si el anterior aceptado ya era uniforme.
pub struct BlankFilter {
    created: Instant,
    last_good: Option<Instant>,
    /// Color de fondo de las ventanas del sistema (COLOR_BTNFACE): el de los
    /// fotogramas fallidos, que son la ventana de Cemu sin el juego.
    background: [u8; 3],
}

impl Default for BlankFilter {
    fn default() -> Self {
        Self { created: Instant::now(), last_good: None, background: window_background() }
    }
}

/// Fondo de ventana del sistema, en BGR.
fn window_background() -> [u8; 3] {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Gdi::{GetSysColor, COLOR_BTNFACE};
        let c = unsafe { GetSysColor(COLOR_BTNFACE) }; // 0x00BBGGRR
        return [((c >> 16) & 255) as u8, ((c >> 8) & 255) as u8, (c & 255) as u8];
    }
    #[allow(unreachable_code)]
    [240, 240, 240]
}

impl BlankFilter {
    /// Un uniforme de otro color (negro de carga, blanco de un fundido) pasa
    /// tras este tiempo sin fotogramas buenos.
    const CONFIRM: Duration = Duration::from_millis(400);
    /// El gris de la ventana vacía solo pasa si Cemu lleva así un buen rato
    /// (sin juego cargado): mientras haya juego es siempre un fallo de captura.
    const CONFIRM_BACKGROUND: Duration = Duration::from_secs(3);

    pub fn accept(&mut self, f: &RawFrame) -> bool {
        self.accept_at(f, Instant::now())
    }

    fn is_background(&self, f: &RawFrame) -> bool {
        let center = ((f.h / 2) * f.w + f.w / 2) as usize * 4;
        f.bgra.get(center..center + 3).is_some_and(|p| {
            (0..3).all(|i| (p[i] as i32 - self.background[i] as i32).abs() <= 4)
        })
    }

    pub fn accept_at(&mut self, f: &RawFrame, now: Instant) -> bool {
        if !is_uniform(f) {
            self.last_good = Some(now);
            return true;
        }
        let confirm = if self.is_background(f) { Self::CONFIRM_BACKGROUND } else { Self::CONFIRM };
        now.saturating_duration_since(self.last_good.unwrap_or(self.created)) >= confirm
    }
}

/// JPEG listo para enviar.
#[derive(Clone)]
pub struct Encoded {
    pub id: u64,
    pub jpeg: Arc<Vec<u8>>,
}

pub struct ScreenHub {
    clients: AtomicUsize,
    running: AtomicBool,
    latest: Mutex<Option<Encoded>>,
    changed: Condvar,
    status: Mutex<String>,
    quality: AtomicU8,
    max_w: AtomicU32,
    max_h: AtomicU32,
    shared: SharedState,
}

impl ScreenHub {
    pub fn new(shared: SharedState) -> Arc<Self> {
        Arc::new(Self {
            clients: AtomicUsize::new(0),
            running: AtomicBool::new(false),
            latest: Mutex::new(None),
            changed: Condvar::new(),
            status: Mutex::new(String::new()),
            quality: AtomicU8::new(DEFAULT_QUALITY),
            max_w: AtomicU32::new(MAX_W),
            max_h: AtomicU32::new(MAX_H),
            shared,
        })
    }

    /// Un móvil se suscribe (tamaño máximo y calidad que pide). Arranca el
    /// hilo de captura si no estaba.
    pub fn client_joined(self: &Arc<Self>, w: u32, h: u32, quality: u8) {
        self.max_w.store(w.clamp(64, MAX_W), Ordering::Relaxed);
        self.max_h.store(h.clamp(36, MAX_H), Ordering::Relaxed);
        self.quality.store(quality.clamp(1, 100), Ordering::Relaxed);
        self.clients.fetch_add(1, Ordering::SeqCst);
        if !self.running.swap(true, Ordering::SeqCst) {
            let hub = self.clone();
            let _ = std::thread::Builder::new()
                .name("pmp-screen-capture".into())
                .spawn(move || capture_loop(hub));
        }
    }

    pub fn client_left(&self) {
        let before = self.clients.fetch_sub(1, Ordering::SeqCst);
        if before == 1 {
            // despierta al hilo de captura para que termine, y a los clientes
            self.changed.notify_all();
        }
    }

    pub fn clients(&self) -> usize {
        self.clients.load(Ordering::Relaxed)
    }

    /// Último JPEG con id mayor que `last`, esperando hasta `timeout`.
    pub fn wait_frame(&self, last: u64, timeout: Duration) -> Option<Encoded> {
        let deadline = Instant::now() + timeout;
        let mut latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(e) = latest.as_ref() {
                if e.id > last {
                    return Some(e.clone());
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (guard, _) = self
                .changed
                .wait_timeout(latest, deadline - now)
                .unwrap_or_else(|e| e.into_inner());
            latest = guard;
        }
    }

    /// Problema actual para el móvil ("" = hay imágenes, nada que decir).
    /// El móvil oculta la imagen al recibir un estado: aquí solo van motivos
    /// de que NO haya imagen, nunca los fps.
    pub fn status(&self) -> String {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_status(&self, s: String) {
        let mut st = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if *st != s {
            *st = s.clone();
            drop(st);
            self.changed.notify_all();
            if !s.is_empty() {
                self.shared.lock().unwrap().cemu_screen_status = Some(s);
            }
        }
    }

    /// Solo para la ventana del PC (fps, tamaño).
    fn set_ui_status(&self, s: String) {
        self.shared.lock().unwrap().cemu_screen_status = Some(s);
    }
}

/// Reduce el fotograma para que quepa en max_w × max_h conservando la
/// relación de aspecto (sin ampliar nunca). Devuelve (w, h, bgra).
pub fn fit(frame: &RawFrame, max_w: u32, max_h: u32) -> (u32, u32, std::borrow::Cow<'_, [u8]>) {
    use fast_image_resize::{images::Image, FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
    if frame.w <= max_w && frame.h <= max_h {
        return (frame.w, frame.h, std::borrow::Cow::Borrowed(&frame.bgra));
    }
    let scale = (max_w as f32 / frame.w as f32).min(max_h as f32 / frame.h as f32);
    let dw = ((frame.w as f32 * scale).round() as u32).max(1);
    let dh = ((frame.h as f32 * scale).round() as u32).max(1);
    let mut src_buf = frame.bgra.clone();
    let Ok(src) = Image::from_slice_u8(frame.w, frame.h, &mut src_buf, PixelType::U8x4) else {
        return (frame.w, frame.h, std::borrow::Cow::Borrowed(&frame.bgra));
    };
    let mut dst = Image::new(dw, dh, PixelType::U8x4);
    let mut resizer = Resizer::new();
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
    if resizer.resize(&src, &mut dst, Some(&opts)).is_err() {
        return (frame.w, frame.h, std::borrow::Cow::Borrowed(&frame.bgra));
    }
    (dw, dh, std::borrow::Cow::Owned(dst.into_vec()))
}

/// BGRA → JPEG baseline 4:2:0.
pub fn encode_jpeg(bgra: &[u8], w: u32, h: u32, quality: u8) -> Result<Vec<u8>, String> {
    use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
    let mut out = Vec::with_capacity(bgra.len() / 8);
    let mut enc = Encoder::new(&mut out, quality);
    enc.set_sampling_factor(SamplingFactor::F_2_2);
    enc.encode(bgra, w as u16, h as u16, ColorType::Bgra)
        .map_err(|e| format!("JPEG: {e}"))?;
    Ok(out)
}

fn capture_loop(hub: Arc<ScreenHub>) {
    let mut cap = capture::Capturer::new();
    let mut prev: Option<RawFrame> = None;
    let mut id: u64 = 0;
    let period = Duration::from_secs_f32(1.0 / FPS_MAX as f32);
    let mut fps_window = Instant::now();
    let mut fps_count = 0u32;
    let mut last_dims = (0u32, 0u32);
    let mut blank = BlankFilter::default();

    while hub.clients.load(Ordering::SeqCst) > 0 {
        let t0 = Instant::now();
        match cap.capture() {
            Ok(Capture::Frame(frame)) => {
                let same = prev
                    .as_ref()
                    .is_some_and(|p| p.w == frame.w && p.h == frame.h && p.bgra == frame.bgra);
                // hay imagen: al móvil no se le manda ningún estado
                hub.set_status(String::new());
                if !same && blank.accept(&frame) {
                    let (w, h, data) = fit(
                        &frame,
                        hub.max_w.load(Ordering::Relaxed),
                        hub.max_h.load(Ordering::Relaxed),
                    );
                    match encode_jpeg(&data, w, h, hub.quality.load(Ordering::Relaxed)) {
                        Ok(jpeg) => {
                            id += 1;
                            *hub.latest.lock().unwrap_or_else(|e| e.into_inner()) =
                                Some(Encoded { id, jpeg: Arc::new(jpeg) });
                            hub.changed.notify_all();
                            fps_count += 1;
                            last_dims = (w, h);
                        }
                        Err(e) => hub.set_status(e),
                    }
                    prev = Some(frame);
                }
            }
            Ok(Capture::Unchanged) => {
                // captura por eventos: nada nuevo desde la última vez
                hub.set_status(String::new());
            }
            Ok(Capture::NoWindow(reason)) => {
                prev = None;
                hub.set_status(reason);
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => {
                prev = None;
                hub.set_status(e);
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        if last_dims != (0, 0) && fps_window.elapsed() >= Duration::from_secs(1) {
            let fps = fps_count as f32 / fps_window.elapsed().as_secs_f32();
            hub.set_ui_status(format!(
                "Pantalla del GamePad: {fps:.0} fps · {}×{} → {} móvil(es)",
                last_dims.0,
                last_dims.1,
                hub.clients()
            ));
            fps_window = Instant::now();
            fps_count = 0;
        }
        let spent = t0.elapsed();
        if spent < period {
            std::thread::sleep(period - spent);
        }
    }
    hub.running.store(false, Ordering::SeqCst);
    hub.set_status(String::new());
    hub.set_ui_status("Pantalla del GamePad: sin móvil suscrito".to_owned());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32) -> RawFrame {
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        for (i, px) in bgra.chunks_mut(4).enumerate() {
            let x = (i as u32 % w) as u8;
            px[0] = x; // B
            px[1] = 128; // G
            px[2] = 255 - x; // R
            px[3] = 255;
        }
        RawFrame { w, h, bgra }
    }

    #[test]
    fn jpeg_de_un_fotograma_pequeno() {
        let f = frame(64, 36);
        let jpeg = encode_jpeg(&f.bgra, f.w, f.h, 70).unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8]), "SOI");
        assert!(jpeg.ends_with(&[0xFF, 0xD9]), "EOI");
        assert!(jpeg.len() < f.bgra.len() / 2);
    }

    #[test]
    fn fit_reduce_solo_si_hace_falta_y_conserva_el_aspecto() {
        let f = frame(655, 368);
        let (w, h, _) = fit(&f, MAX_W, MAX_H);
        assert_eq!((w, h), (655, 368), "cabe: no se toca");
        let big = frame(1280, 720);
        let (w, h, data) = fit(&big, MAX_W, MAX_H);
        assert_eq!((w, h), (853, 480));
        assert_eq!(data.len(), (853 * 480 * 4) as usize);
        let (w, h, _) = fit(&big, 427, 480);
        assert_eq!((w, h), (427, 240), "manda el ancho");
    }

    #[test]
    fn capturas_en_blanco_aisladas_se_descartan() {
        let game = frame(64, 36);
        let white = RawFrame { w: 64, h: 36, bgra: vec![255u8; 64 * 36 * 4] };
        assert!(!is_uniform(&game));
        assert!(is_uniform(&white));
        // el fondo gris de una ventana de Windows 11 con las esquinas
        // redondeadas en negro: uniforme (lo que fallaba con PrintWindow/WGC)
        let mut gray = RawFrame { w: 854, h: 480, bgra: vec![240u8; 854 * 480 * 4] };
        for y in 0..12usize {
            for x in 0..12usize {
                for (cx, cy) in [(x, y), (853 - x, y), (x, 479 - y), (853 - x, 479 - y)] {
                    let i = (cy * 854 + cx) * 4;
                    gray.bgra[i] = 0;
                    gray.bgra[i + 1] = 0;
                    gray.bgra[i + 2] = 0;
                }
            }
        }
        assert!(is_uniform(&gray), "gris con esquinas negras");
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let bg = [240, 240, 240];
        let mut f = BlankFilter { created: t0, last_good: None, background: bg };
        assert!(f.accept_at(&game, at(0)));
        assert!(!f.accept_at(&white, at(33)), "blanco suelto entre dos buenos: fuera");
        assert!(f.accept_at(&game, at(66)));
        // varios blancos seguidos pero poco después de un bueno: fuera todos
        assert!(!f.accept_at(&white, at(100)));
        assert!(!f.accept_at(&white, at(133)));
        assert!(!f.accept_at(&white, at(166)));
        assert!(f.accept_at(&game, at(200)));
        // 400 ms sin nada bueno: es una pantalla en blanco de verdad
        assert!(!f.accept_at(&white, at(500)));
        assert!(f.accept_at(&white, at(601)), "uniforme sostenido: pasa");
        assert!(f.accept_at(&white, at(634)), "y sigue pasando mientras dure");
        assert!(f.accept_at(&game, at(700)));
        assert!(!f.accept_at(&white, at(733)), "vuelta al juego: el siguiente blanco suelto se descarta otra vez");
        // el gris de la ventana vacía (fondo del sistema) es un fallo de captura
        // aunque la pantalla lleve segundos quieta: solo pasa tras 3 s sin juego
        assert!(f.accept_at(&game, at(1000)));
        assert!(!f.accept_at(&gray, at(2500)), "gris de ventana 1,5 s después del último bueno: fuera");
        assert!(!f.accept_at(&gray, at(3900)));
        assert!(f.accept_at(&gray, at(4100)), "3 s sin juego: Cemu sin juego cargado, se enseña");
        // arrancar en negro (carga) también exige los 400 ms
        let black = RawFrame { w: 64, h: 36, bgra: vec![0u8; 64 * 36 * 4] };
        let mut g = BlankFilter { created: t0, last_good: None, background: bg };
        assert!(!g.accept_at(&black, at(100)));
        assert!(g.accept_at(&black, at(450)));
    }

    #[test]
    fn hub_espera_fotogramas_nuevos() {
        let hub = ScreenHub::new(crate::state::new_shared());
        assert!(hub.wait_frame(0, Duration::from_millis(20)).is_none());
        *hub.latest.lock().unwrap() = Some(Encoded { id: 1, jpeg: Arc::new(vec![1, 2, 3]) });
        let e = hub.wait_frame(0, Duration::from_millis(20)).unwrap();
        assert_eq!(e.id, 1);
        assert!(hub.wait_frame(1, Duration::from_millis(20)).is_none(), "ya visto");
    }
}
