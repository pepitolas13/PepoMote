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
    /// No hay ventana que capturar, con el motivo legible para el móvil.
    NoWindow(String),
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
            status: Mutex::new("Conectando la pantalla…".to_owned()),
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

    pub fn status(&self) -> String {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_status(&self, s: String) {
        let mut st = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if *st != s {
            *st = s.clone();
            drop(st);
            self.changed.notify_all();
            self.shared.lock().unwrap().cemu_screen_status = Some(s);
        }
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

    while hub.clients.load(Ordering::SeqCst) > 0 {
        let t0 = Instant::now();
        match cap.capture() {
            Ok(Capture::Frame(frame)) => {
                let same = prev
                    .as_ref()
                    .is_some_and(|p| p.w == frame.w && p.h == frame.h && p.bgra == frame.bgra);
                if !same {
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
                if fps_window.elapsed() >= Duration::from_secs(1) {
                    let fps = fps_count as f32 / fps_window.elapsed().as_secs_f32();
                    hub.set_status(format!(
                        "Pantalla del GamePad: {fps:.0} fps · {}×{} → {} móvil(es)",
                        last_dims.0,
                        last_dims.1,
                        hub.clients()
                    ));
                    fps_window = Instant::now();
                    fps_count = 0;
                }
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
        let spent = t0.elapsed();
        if spent < period {
            std::thread::sleep(period - spent);
        }
    }
    hub.running.store(false, Ordering::SeqCst);
    hub.set_status("Pantalla del GamePad: sin móvil suscrito".to_owned());
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
    fn hub_espera_fotogramas_nuevos() {
        let hub = ScreenHub::new(crate::state::new_shared());
        assert!(hub.wait_frame(0, Duration::from_millis(20)).is_none());
        *hub.latest.lock().unwrap() = Some(Encoded { id: 1, jpeg: Arc::new(vec![1, 2, 3]) });
        let e = hub.wait_frame(0, Duration::from_millis(20)).unwrap();
        assert_eq!(e.id, 1);
        assert!(hub.wait_frame(1, Duration::from_millis(20)).is_none(), "ya visto");
    }
}
