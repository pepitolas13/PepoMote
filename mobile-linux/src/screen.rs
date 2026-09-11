//! Doble pantalla del Wii U: el receptor captura la ventana «GamePad View»
//! de Cemu, la comprime en JPEG y la manda al móvil que hace de GamePad; aquí
//! se pinta en la zona táctil central de la pantalla GamePad (tocar la imagen
//! = tocar la pantalla del GamePad). PMP v1, cambio aditivo.
//!
//! Transporte: una conexión TCP NUEVA al mismo host y puerto PMP que el canal
//! de control. Primera línea del móvil
//! `{"m":"screen","session_id":…,"w":854,"h":480,"q":70}` (`w`×`h` es el
//! tamaño MÁXIMO: la zona táctil en píxeles físicos, nunca más que el nativo
//! 854×480); el receptor contesta `{"m":"screen","ok":true}` (o `err`, y
//! cierra) y desde ahí solo manda tramas binarias `"PMPS"` + tipo (u8) +
//! longitud (u32 LE) + carga: 1 = JPEG, 2 = estado (texto para el hueco
//! mientras no hay imagen), 3 = keepalive (sin carga, cada 2 s). Control de
//! flujo: un byte `0x01` por cada imagen decodificada y lista para pintar
//! (una en vuelo: la latencia no se acumula). Un receptor antiguo no entiende
//! `screen`: sin respuesta en 3 s se sigue sin pantalla y se reintenta cada
//! 5 s; sin datos en 6 s la conexión está muerta y se reconecta (1 s, luego
//! 5 s). Todo en un hilo propio con búferes reutilizados: la UI solo recoge
//! la última imagen (`take_image`) y la sube a su textura.

use egui::{Color32, ColorImage};
use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

pub const MAGIC: [u8; 4] = *b"PMPS";
/// magic (4) + tipo (1) + longitud (4).
pub const HEADER_LEN: usize = 9;
pub const KIND_JPEG: u8 = 1;
pub const KIND_STATUS: u8 = 2;
pub const KIND_KEEPALIVE: u8 = 3;
/// Confirmación de una imagen decodificada y lista: un byte.
pub const ACK: [u8; 1] = [0x01];
/// Resolución nativa del GamePad: nunca se pide más.
pub const MAX_W: u32 = 854;
pub const MAX_H: u32 = 480;
pub const JPEG_QUALITY: u8 = 70;
/// Una carga mayor no es una trama nuestra: se resincroniza.
const MAX_PAYLOAD: usize = 8 << 20;
/// Tope de la línea de respuesta al `screen`.
const MAX_REPLY: usize = 4096;
const MAX_DIM: usize = 4096;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Sin respuesta al `screen` en este tiempo: receptor sin pantalla.
const REPLY_TIMEOUT: Duration = Duration::from_secs(3);
/// Sin datos (ni keepalive) en este tiempo: conexión muerta.
const DATA_TIMEOUT: Duration = Duration::from_secs(6);
const RETRY_NO_SCREEN: Duration = Duration::from_secs(5);
const RETRY_FIRST: Duration = Duration::from_secs(1);
const RETRY_NEXT: Duration = Duration::from_secs(5);
const READ_CHUNK: usize = 64 * 1024;
/// Imágenes en rotación como máximo (la que sube egui, la que espera en el
/// hueco y la que se decodifica).
const POOL_MAX: usize = 4;

/// A dónde abrir el canal: host y puerto PMP del receptor (los del canal de
/// control) y la sesión del `ok`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub session_id: u32,
}

/// En qué está el canal (para la etiqueta del hueco mientras no hay imagen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Conectando o reconectando.
    Connecting = 0,
    /// El receptor aceptó el `screen`: llegan tramas.
    Streaming = 1,
    /// El receptor no contesta o rechaza (versión sin pantalla): se sigue
    /// sin ella y se reintenta cada 5 s.
    NoScreen = 2,
}

impl Phase {
    fn from_u8(v: u8) -> Phase {
        match v {
            1 => Phase::Streaming,
            2 => Phase::NoScreen,
            _ => Phase::Connecting,
        }
    }
}

/// Tamaño a pedir: el ancho de la zona táctil en píxeles físicos con alto
/// 16:9, sin pasar del nativo 854×480 (más no sirve de nada).
pub fn wanted_size(zone_px: (f32, f32)) -> (u32, u32) {
    let w = if zone_px.0.is_finite() && zone_px.0 >= 1.0 { zone_px.0.round() as u32 } else { MAX_W };
    let w = w.clamp(16, MAX_W);
    let h = ((w * 9 + 8) / 16).clamp(9, MAX_H);
    (w, h)
}

/// Cambio de tamaño que merece reabrir el canal (más de 8 px por eje).
pub fn size_differs(a: (u32, u32), b: (u32, u32)) -> bool {
    a.0.abs_diff(b.0) > 8 || a.1.abs_diff(b.1) > 8
}

/// Primera línea del móvil.
pub fn request_line(session_id: u32, (w, h): (u32, u32)) -> String {
    let mut s = json!({"m":"screen","session_id":session_id,"w":w,"h":h,"q":JPEG_QUALITY}).to_string();
    s.push('\n');
    s
}

/// Respuesta al `screen`: `Ok` si el receptor va a mandar la pantalla; si
/// no, el motivo (para el log).
pub fn parse_reply(line: &str) -> Result<(), String> {
    let v: Value = serde_json::from_str(line.trim()).map_err(|_| "respuesta ilegible".to_owned())?;
    match v["m"].as_str() {
        Some("screen") if v["ok"].as_bool() == Some(true) => Ok(()),
        Some("err") => Err(v["msg"]
            .as_str()
            .or(v["code"].as_str())
            .unwrap_or("rechazado")
            .to_owned()),
        _ => Err("respuesta inesperada".into()),
    }
}

// ---- Parser de tramas -------------------------------------------------------

/// Qué hay al principio del búfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parsed {
    /// Faltan bytes (de cabecera o de carga): seguir leyendo.
    Incomplete,
    /// Trama completa: tipo y longitud de la carga, que empieza en `HEADER_LEN`.
    Frame { kind: u8, len: usize },
    /// El magic no cuadra (o la longitud es absurda): saltar `skip` bytes,
    /// hasta el siguiente candidato a magic, y volver a mirar.
    Bad { skip: usize },
}

/// `b` empieza como "PMPS" (o es un prefijo suyo).
fn magic_prefix(b: &[u8]) -> bool {
    let n = b.len().min(MAGIC.len());
    b[..n] == MAGIC[..n]
}

/// Bytes a saltar para plantarse en el siguiente candidato a magic (a partir
/// del segundo byte); si no hay ninguno, el búfer entero.
fn resync_skip(buf: &[u8]) -> usize {
    (1..buf.len()).find(|&i| magic_prefix(&buf[i..])).unwrap_or(buf.len())
}

/// Cabecera de la trama al principio de `buf` (puro).
pub fn parse_header(buf: &[u8]) -> Parsed {
    if !magic_prefix(buf) {
        return Parsed::Bad { skip: resync_skip(buf) };
    }
    if buf.len() < HEADER_LEN {
        return Parsed::Incomplete;
    }
    let kind = buf[4];
    let len = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]) as usize;
    if len > MAX_PAYLOAD {
        return Parsed::Bad { skip: resync_skip(buf) };
    }
    if buf.len() < HEADER_LEN + len {
        return Parsed::Incomplete;
    }
    Parsed::Frame { kind, len }
}

/// Una trama completa, prestada del búfer del lector.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame<'a> {
    pub kind: u8,
    pub payload: &'a [u8],
}

/// Acumulador de tramas con un solo búfer reutilizado: lo leído se añade
/// (`feed`), las tramas completas salen prestadas (`next`, sin copiar) y se
/// consumen al pedir la siguiente. Ante bytes que no son una trama, salta al
/// siguiente candidato a magic (`resyncs` los cuenta).
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
    /// Principio de lo aún no consumido.
    pos: usize,
    /// Longitud de la trama devuelta la última vez (se consume en la
    /// siguiente llamada).
    pending: usize,
    pub resyncs: u32,
}

impl FrameReader {
    pub fn feed(&mut self, bytes: &[u8]) {
        self.consume_pending();
        if self.pos >= self.buf.len() {
            self.buf.clear();
        } else if self.pos > 0 {
            self.buf.drain(..self.pos);
        }
        self.pos = 0;
        self.buf.extend_from_slice(bytes);
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.pos = 0;
        self.pending = 0;
    }

    /// Bytes aún sin consumir.
    pub fn unread(&self) -> usize {
        self.buf.len() - self.pos - self.pending
    }

    fn consume_pending(&mut self) {
        self.pos += self.pending;
        self.pending = 0;
    }

    /// Siguiente trama completa; `None` si faltan bytes.
    pub fn next(&mut self) -> Option<Frame<'_>> {
        self.consume_pending();
        loop {
            match parse_header(&self.buf[self.pos..]) {
                Parsed::Incomplete => return None,
                Parsed::Bad { skip } => {
                    self.pos += skip;
                    self.resyncs = self.resyncs.wrapping_add(1);
                }
                Parsed::Frame { kind, len } => {
                    self.pending = HEADER_LEN + len;
                    let start = self.pos + HEADER_LEN;
                    return Some(Frame {
                        kind,
                        payload: &self.buf[start..start + len],
                    });
                }
            }
        }
    }

    /// Una línea de texto (hasta `\n`, sin él, recortada) del principio del
    /// búfer: la respuesta al `screen`, que puede llegar pegada a la primera
    /// trama. `None` si aún no hay salto de línea.
    pub fn take_line(&mut self) -> Option<String> {
        self.consume_pending();
        let rest = &self.buf[self.pos..];
        let nl = rest.iter().position(|b| *b == b'\n')?;
        let line = String::from_utf8_lossy(&rest[..nl]).trim().to_owned();
        self.pos += nl + 1;
        Some(line)
    }
}

/// Una trama en el cable (pruebas y servidor falso).
#[cfg(test)]
pub fn encode_frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(HEADER_LEN + payload.len());
    v.extend_from_slice(&MAGIC);
    v.push(kind);
    v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

// ---- Cliente ---------------------------------------------------------------

/// Lo que comparten el hilo y la UI.
struct Shared {
    /// Última imagen decodificada, a la espera de que la UI la recoja.
    image: Mutex<Option<Arc<ColorImage>>>,
    /// Avisa al hilo cuando la UI recoge la imagen (confirmación en espera).
    taken: Condvar,
    /// Última línea de estado (tipo 2).
    status: Mutex<Option<Arc<str>>>,
    /// Hay imagen vigente que enseñar (se apaga con un estado tipo 2 y al
    /// morir la conexión: el receptor ya no tiene nada que mandar).
    showing: AtomicBool,
    phase: AtomicU8,
    /// Imágenes por segundo (bits de un f32), media de 1 s.
    fps: AtomicU32,
}

impl Shared {
    fn new() -> Self {
        Self {
            image: Mutex::new(None),
            taken: Condvar::new(),
            status: Mutex::new(None),
            showing: AtomicBool::new(false),
            phase: AtomicU8::new(Phase::Connecting as u8),
            fps: AtomicU32::new(0),
        }
    }

    fn phase(&self) -> Phase {
        Phase::from_u8(self.phase.load(Ordering::Relaxed))
    }

    fn set_phase(&self, p: Phase) {
        self.phase.store(p as u8, Ordering::Relaxed);
    }

    /// Deja la imagen para la UI. `true` si la anterior seguía sin recoger
    /// (la UI no está pintando).
    fn publish(&self, img: Arc<ColorImage>) -> bool {
        let stalled = self.image.lock().unwrap().replace(img).is_some();
        self.showing.store(true, Ordering::Relaxed);
        stalled
    }

    fn set_status(&self, text: &[u8]) {
        let t = String::from_utf8_lossy(text);
        let t = t.trim();
        *self.status.lock().unwrap() = if t.is_empty() { None } else { Some(Arc::from(t)) };
        self.showing.store(false, Ordering::Relaxed);
    }

    /// Sesión nueva: lo de la anterior ya no vale.
    fn reset_stream(&self) {
        *self.status.lock().unwrap() = None;
        self.showing.store(false, Ordering::Relaxed);
        self.fps.store(0, Ordering::Relaxed);
    }

    /// Espera a que la UI recoja la imagen publicada. `false` si hay que parar.
    fn wait_taken(&self, stop: &AtomicBool) -> bool {
        let mut g = self.image.lock().unwrap();
        while g.is_some() {
            if stop.load(Ordering::Relaxed) {
                return false;
            }
            g = self.taken.wait_timeout(g, Duration::from_millis(100)).unwrap().0;
        }
        true
    }
}

/// Canal de la pantalla del GamePad en su hilo. Se para (flag + cierre del
/// socket, el hilo termina solo) al soltarlo o con `stop`.
pub struct Client {
    endpoint: Endpoint,
    size: (u32, u32),
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    socket: Arc<Mutex<Option<TcpStream>>>,
}

impl Client {
    /// Abre el canal en segundo plano; `ctx` para repintar al llegar cada imagen.
    pub fn start(endpoint: Endpoint, size: (u32, u32), ctx: egui::Context) -> Client {
        let shared = Arc::new(Shared::new());
        let stop = Arc::new(AtomicBool::new(false));
        let socket: Arc<Mutex<Option<TcpStream>>> = Arc::new(Mutex::new(None));
        {
            let (endpoint, shared, stop, socket) = (endpoint.clone(), shared.clone(), stop.clone(), socket.clone());
            std::thread::Builder::new()
                .name("pepomote-screen".into())
                .spawn(move || run(endpoint, size, shared, socket, stop, ctx))
                .expect("hilo pantalla");
        }
        Client {
            endpoint,
            size,
            shared,
            stop,
            socket,
        }
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Tamaño máximo pedido.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// La imagen pendiente de subir a la textura, si la hay (una vez).
    pub fn take_image(&self) -> Option<Arc<ColorImage>> {
        let img = self.shared.image.lock().unwrap().take();
        if img.is_some() {
            self.shared.taken.notify_all();
        }
        img
    }

    /// La última imagen sigue vigente (no la ha invalidado un estado ni la
    /// caída de la conexión).
    pub fn showing(&self) -> bool {
        self.shared.showing.load(Ordering::Relaxed)
    }

    /// Última línea de estado del receptor (tipo 2).
    pub fn status(&self) -> Option<Arc<str>> {
        self.shared.status.lock().unwrap().clone()
    }

    pub fn phase(&self) -> Phase {
        self.shared.phase()
    }

    /// Imágenes por segundo (media de 1 s; 0 si no llegan).
    pub fn fps(&self) -> f32 {
        f32::from_bits(self.shared.fps.load(Ordering::Relaxed))
    }

    /// Texto para el hueco mientras no hay imagen.
    pub fn placeholder(&self) -> Arc<str> {
        self.status().unwrap_or_else(|| {
            Arc::from(match self.phase() {
                Phase::NoScreen => "El PC no envía la pantalla",
                _ => "Conectando la pantalla…",
            })
        })
    }

    pub fn stop(self) {
        drop(self);
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(s) = self.socket.lock().unwrap().take() {
            let _ = s.shutdown(Shutdown::Both);
        }
        self.shared.taken.notify_all();
    }
}

enum Outcome {
    Stopped,
    /// El receptor no manda la pantalla (no contesta, rechaza o cierra).
    NoScreen(String),
    /// La conexión murió (o no se pudo abrir): reconectar.
    Died(String),
}

/// Búferes del hilo, reutilizados de trama en trama.
struct Io {
    chunk: Vec<u8>,
    reader: FrameReader,
    dec: Decoder,
}

fn is_timeout(e: &std::io::Error) -> bool {
    matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

fn sleep_unless_stopped(stop: &AtomicBool, d: Duration) {
    let end = Instant::now() + d;
    while !stop.load(Ordering::Relaxed) {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        std::thread::sleep(left.min(Duration::from_millis(50)));
    }
}

fn run(
    endpoint: Endpoint,
    size: (u32, u32),
    shared: Arc<Shared>,
    socket: Arc<Mutex<Option<TcpStream>>>,
    stop: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    let mut io = Io {
        chunk: vec![0u8; READ_CHUNK],
        reader: FrameReader::default(),
        dec: Decoder::default(),
    };
    let mut retry = RETRY_FIRST;
    while !stop.load(Ordering::Relaxed) {
        let outcome = session(&endpoint, size, &shared, &socket, &stop, &ctx, &mut io, &mut retry);
        if let Some(s) = socket.lock().unwrap().take() {
            let _ = s.shutdown(Shutdown::Both);
        }
        shared.showing.store(false, Ordering::Relaxed);
        shared.fps.store(0, Ordering::Relaxed);
        if stop.load(Ordering::Relaxed) {
            // parar cierra el socket: ese "fin" no es una caída
            break;
        }
        let wait = match outcome {
            Outcome::Stopped => break,
            Outcome::NoScreen(why) => {
                if shared.phase() != Phase::NoScreen {
                    crate::app::log_line(&format!("pantalla del GamePad: sin pantalla ({why}); se reintenta cada 5 s"));
                }
                shared.set_phase(Phase::NoScreen);
                RETRY_NO_SCREEN
            }
            Outcome::Died(why) => {
                crate::app::log_line(&format!("pantalla del GamePad: {why}; reconectando en {} s", retry.as_secs()));
                shared.set_phase(Phase::Connecting);
                let w = retry;
                retry = RETRY_NEXT;
                w
            }
        };
        ctx.request_repaint();
        sleep_unless_stopped(&stop, wait);
    }
}

/// Una conexión: `screen`, respuesta y tramas hasta que muera o haya que parar.
#[allow(clippy::too_many_arguments)]
fn session(
    ep: &Endpoint,
    size: (u32, u32),
    shared: &Shared,
    socket: &Mutex<Option<TcpStream>>,
    stop: &AtomicBool,
    ctx: &egui::Context,
    io: &mut Io,
    retry: &mut Duration,
) -> Outcome {
    let addr = match crate::link::resolve(&ep.host, ep.port) {
        Ok(a) => a,
        Err(e) => return Outcome::Died(e),
    };
    let mut stream = match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
        Ok(s) => s,
        Err(e) => return Outcome::Died(format!("no llego a {}:{}: {e}", ep.host, ep.port)),
    };
    if stop.load(Ordering::Relaxed) {
        return Outcome::Stopped;
    }
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(REPLY_TIMEOUT));
    if let Ok(c) = stream.try_clone() {
        *socket.lock().unwrap() = Some(c);
    }
    if stream.write_all(request_line(ep.session_id, size).as_bytes()).is_err() {
        return Outcome::NoScreen("el PC cerró la conexión".into());
    }

    // La respuesta, sin BufReader: lo que venga detrás ya son tramas
    io.reader.clear();
    let line = loop {
        if let Some(l) = io.reader.take_line() {
            break l;
        }
        if io.reader.unread() > MAX_REPLY {
            return Outcome::NoScreen("respuesta ilegible".into());
        }
        match stream.read(&mut io.chunk) {
            Ok(0) => return Outcome::NoScreen("el PC cerró sin contestar".into()),
            Ok(n) => io.reader.feed(&io.chunk[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) if is_timeout(&e) => return Outcome::NoScreen("sin respuesta en 3 s".into()),
            Err(_) if stop.load(Ordering::Relaxed) => return Outcome::Stopped,
            Err(e) => return Outcome::NoScreen(e.to_string()),
        }
    };
    if let Err(why) = parse_reply(&line) {
        return Outcome::NoScreen(why);
    }

    // En marcha: a partir de aquí solo tramas
    *retry = RETRY_FIRST;
    shared.reset_stream();
    shared.set_phase(Phase::Streaming);
    ctx.request_repaint();
    crate::app::log_line(&format!("pantalla del GamePad: en marcha ({}×{} máx.)", size.0, size.1));
    let _ = stream.set_read_timeout(Some(DATA_TIMEOUT));
    let mut fps = FpsMeter::new();
    loop {
        // tramas ya acumuladas (la respuesta pudo venir pegada a la primera)
        while let Some(f) = io.reader.next() {
            match f.kind {
                KIND_JPEG => {
                    match io.dec.decode(f.payload) {
                        Ok(img) => {
                            let stalled = shared.publish(img);
                            ctx.request_repaint();
                            fps.tick(shared);
                            // Si la UI no recogía la anterior, no está
                            // pintando (app en segundo plano: el compositor
                            // no la refresca): la confirmación espera a que
                            // recoja esta, y el receptor no manda más hasta
                            // que la app vuelve.
                            if stalled && !shared.wait_taken(stop) {
                                return Outcome::Stopped;
                            }
                        }
                        Err(e) => io.dec.log_once(&e),
                    }
                    // se confirma siempre (también una imagen ilegible): si no,
                    // el receptor se quedaría esperando
                    if stream.write_all(&ACK).is_err() {
                        return Outcome::Died("no puedo confirmar la imagen".into());
                    }
                }
                KIND_STATUS => {
                    shared.set_status(f.payload);
                    ctx.request_repaint();
                }
                // el keepalive ya refrescó el temporizador al leerlo
                KIND_KEEPALIVE => {}
                // un tipo futuro (cambio aditivo): se ignora
                _ => {}
            }
        }
        if stop.load(Ordering::Relaxed) {
            return Outcome::Stopped;
        }
        match stream.read(&mut io.chunk) {
            Ok(0) => return Outcome::Died("el PC cerró la pantalla".into()),
            Ok(n) => io.reader.feed(&io.chunk[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) if is_timeout(&e) => return Outcome::Died("6 s sin datos".into()),
            Err(_) if stop.load(Ordering::Relaxed) => return Outcome::Stopped,
            Err(e) => return Outcome::Died(e.to_string()),
        }
        fps.idle(shared);
    }
}

/// Media de imágenes por segundo en ventanas de 1 s.
struct FpsMeter {
    window: Instant,
    count: u32,
}

impl FpsMeter {
    fn new() -> Self {
        Self {
            window: Instant::now(),
            count: 0,
        }
    }

    fn tick(&mut self, shared: &Shared) {
        self.count += 1;
        self.idle(shared);
    }

    /// Cierra la ventana si ya ha pasado 1 s (también sin imágenes: así el
    /// contador cae a 0 cuando dejan de llegar).
    fn idle(&mut self, shared: &Shared) {
        let el = self.window.elapsed();
        if el >= Duration::from_secs(1) {
            shared.fps.store((self.count as f32 / el.as_secs_f32()).to_bits(), Ordering::Relaxed);
            self.window = Instant::now();
            self.count = 0;
        }
    }
}

/// Decodificador JPEG con sus búferes: el RGB de zune y las imágenes egui,
/// reutilizados (solo se reserva si cambian las dimensiones o si egui aún
/// tiene todas ocupadas).
#[derive(Default)]
struct Decoder {
    rgb: Vec<u8>,
    /// Imágenes en rotación; se reutiliza la que nadie más tenga (egui suelta
    /// la suya al subirla a la textura).
    pool: Vec<Arc<ColorImage>>,
    logged: bool,
}

impl Decoder {
    fn decode(&mut self, jpeg: &[u8]) -> Result<Arc<ColorImage>, String> {
        let options = DecoderOptions::default()
            .jpeg_set_out_colorspace(ColorSpace::RGB)
            .set_max_width(MAX_DIM)
            .set_max_height(MAX_DIM);
        let mut dec = JpegDecoder::new_with_options(jpeg, options);
        dec.decode_headers().map_err(|e| e.to_string())?;
        let (w, h) = dec.dimensions().ok_or("sin dimensiones")?;
        let n = dec
            .output_buffer_size()
            .filter(|n| *n == w * h * 3)
            .ok_or("tamaño imposible")?;
        if self.rgb.len() < n {
            self.rgb.resize(n, 0);
        }
        dec.decode_into(&mut self.rgb[..n]).map_err(|e| e.to_string())?;
        let i = self.free_slot();
        let img = Arc::get_mut(&mut self.pool[i]).expect("imagen sin otros dueños");
        img.size = [w, h];
        img.pixels.resize(w * h, Color32::BLACK);
        let (rgb, _) = self.rgb[..n].as_chunks::<3>();
        for (px, c) in img.pixels.iter_mut().zip(rgb) {
            *px = Color32::from_rgb(c[0], c[1], c[2]);
        }
        Ok(self.pool[i].clone())
    }

    /// Índice de una imagen que solo tenemos nosotros; si todas están en uso,
    /// una nueva (soltando la más antigua si ya hay demasiadas).
    fn free_slot(&mut self) -> usize {
        if let Some(i) = self.pool.iter_mut().position(|a| Arc::get_mut(a).is_some()) {
            return i;
        }
        if self.pool.len() >= POOL_MAX {
            self.pool.remove(0);
        }
        self.pool.push(Arc::new(ColorImage {
            size: [0, 0],
            pixels: Vec::new(),
        }));
        self.pool.len() - 1
    }

    fn log_once(&mut self, e: &str) {
        if !self.logged {
            self.logged = true;
            crate::app::log_line(&format!("pantalla del GamePad: imagen ilegible ({e}); se sigue con las siguientes"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    /// JPEG de 16×9 (4:4:4, calidad 90): mitad izquierda roja (220,40,40),
    /// derecha azul (40,60,220).
    const TINY_JPEG: &str = "
        ff d8 ff e0 00 10 4a 46 49 46 00 01 01 00 00 01 00 01 00 00 ff db 00 43 00 03 02 02 03 02 02 03
        03 03 03 04 03 03 04 05 08 05 05 04 04 05 0a 07 07 06 08 0c 0a 0c 0c 0b 0a 0b 0b 0d 0e 12 10 0d
        0e 11 0e 0b 0b 10 16 10 11 13 14 15 15 15 0c 0f 17 18 16 14 18 12 14 15 14 ff db 00 43 01 03 04
        04 05 04 05 09 05 05 09 14 0d 0b 0d 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14
        14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 14 ff c0
        00 11 08 00 09 00 10 03 01 11 00 02 11 01 03 11 01 ff c4 00 15 00 01 01 00 00 00 00 00 00 00 00
        00 00 00 00 00 00 06 07 ff c4 00 14 10 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ff c4
        00 16 01 01 01 01 00 00 00 00 00 00 00 00 00 00 00 00 00 09 07 08 ff c4 00 14 11 01 00 00 00 00
        00 00 00 00 00 00 00 00 00 00 00 00 ff da 00 0c 03 01 00 02 11 03 11 00 3f 00 92 25 ed e0 0a 4b
        46 91 d0 d2 25 a0 a4 b4 69 3f ff d9";

    fn tiny_jpeg() -> Vec<u8> {
        TINY_JPEG
            .split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).unwrap())
            .collect()
    }

    fn near(c: Color32, rgb: [u8; 3]) -> bool {
        [c.r(), c.g(), c.b()].iter().zip(rgb).all(|(a, b)| a.abs_diff(b) <= 12)
    }

    #[test]
    fn cabecera_magic_tipo_y_longitud() {
        let f = encode_frame(KIND_JPEG, b"abc");
        assert_eq!(&f[..4], b"PMPS");
        assert_eq!(f[4], 1);
        assert_eq!(&f[5..9], &[3, 0, 0, 0], "longitud u32 little-endian");
        assert_eq!(parse_header(&f), Parsed::Frame { kind: 1, len: 3 });
        assert_eq!(parse_header(&[]), Parsed::Incomplete, "vacío: a esperar");
        assert_eq!(parse_header(b"PMP"), Parsed::Incomplete, "prefijo del magic: a esperar");
        assert_eq!(parse_header(b"PMPS\x01\x03\x00"), Parsed::Incomplete, "cabecera a medias");
        assert_eq!(parse_header(&f[..HEADER_LEN + 2]), Parsed::Incomplete, "carga a medias");
        assert_eq!(parse_header(&encode_frame(KIND_KEEPALIVE, &[])), Parsed::Frame { kind: 3, len: 0 });
        assert_eq!(parse_header(b"XXXXXXXX"), Parsed::Bad { skip: 8 }, "sin candidato: se tira todo");
        assert_eq!(parse_header(b"XPMPS\x03"), Parsed::Bad { skip: 1 }, "hasta el siguiente magic");
        assert_eq!(parse_header(b"PMPX\x00P"), Parsed::Bad { skip: 5 }, "una P suelta al final es candidata");
        let mut huge = encode_frame(KIND_JPEG, b"");
        huge[5..9].copy_from_slice(&(MAX_PAYLOAD as u32 + 1).to_le_bytes());
        assert_eq!(parse_header(&huge), Parsed::Bad { skip: 9 }, "longitud absurda: no es nuestra");
        assert_eq!(ACK, [0x01]);
    }

    #[test]
    fn tramas_partidas_en_varios_read() {
        let mut r = FrameReader::default();
        let f = encode_frame(KIND_JPEG, &[9u8; 40]);
        for b in &f[..f.len() - 1] {
            r.feed(std::slice::from_ref(b));
            assert!(r.next().is_none(), "sin el último byte no hay trama");
        }
        r.feed(&f[f.len() - 1..]);
        let got = r.next().expect("trama completa");
        assert_eq!(got.kind, KIND_JPEG);
        assert_eq!(got.payload, &[9u8; 40][..]);
        assert!(r.next().is_none());
        assert_eq!(r.resyncs, 0);
        assert_eq!(r.unread(), 0, "consumida al pedir la siguiente");
        // la cabecera en un read y la carga en otro
        r.feed(&f[..HEADER_LEN]);
        assert!(r.next().is_none());
        r.feed(&f[HEADER_LEN..]);
        assert_eq!(r.next().unwrap().payload.len(), 40);
    }

    #[test]
    fn magic_incorrecto_resincroniza() {
        let mut r = FrameReader::default();
        let mut bytes = b"basura PM".to_vec();
        bytes.extend(encode_frame(KIND_STATUS, "Cemu no está abierto".as_bytes()));
        bytes.extend(b"XX");
        bytes.extend(encode_frame(KIND_KEEPALIVE, &[]));
        r.feed(&bytes);
        let f = r.next().expect("tras la basura, la trama");
        assert_eq!(f.kind, KIND_STATUS);
        assert_eq!(std::str::from_utf8(f.payload).unwrap(), "Cemu no está abierto");
        assert_eq!(r.next().map(|f| f.kind), Some(KIND_KEEPALIVE), "y la siguiente tras los dos bytes sueltos");
        assert!(r.next().is_none());
        assert!(r.resyncs >= 2, "cada tropiezo cuenta: {}", r.resyncs);
    }

    #[test]
    fn keepalive_sin_carga_y_varias_seguidas() {
        let mut r = FrameReader::default();
        let mut bytes = encode_frame(KIND_JPEG, &tiny_jpeg());
        bytes.extend(encode_frame(KIND_KEEPALIVE, &[]));
        bytes.extend(encode_frame(KIND_STATUS, b"Esperando"));
        bytes.extend(encode_frame(KIND_JPEG, b"segunda"));
        r.feed(&bytes);
        let kinds: Vec<(u8, usize)> = std::iter::from_fn(|| r.next().map(|f| (f.kind, f.payload.len()))).collect();
        assert_eq!(kinds, vec![(1, tiny_jpeg().len()), (3, 0), (2, 9), (1, 7)]);
        assert_eq!(r.unread(), 0);
        r.feed(&encode_frame(7, b"?"));
        assert_eq!(r.next().map(|f| f.kind), Some(7), "un tipo futuro se entrega tal cual (y el hilo lo ignora)");
    }

    #[test]
    fn linea_de_respuesta_pegada_a_la_primera_trama() {
        let mut r = FrameReader::default();
        let mut bytes = b"{\"m\":\"screen\",\"ok\":true}\r\n".to_vec();
        bytes.extend(encode_frame(KIND_KEEPALIVE, &[]));
        r.feed(&bytes[..10]);
        assert_eq!(r.take_line(), None, "sin salto de línea aún");
        r.feed(&bytes[10..]);
        assert_eq!(r.take_line().as_deref(), Some("{\"m\":\"screen\",\"ok\":true}"));
        assert_eq!(r.next().map(|f| f.kind), Some(KIND_KEEPALIVE), "lo de detrás de la línea son tramas");
        r.clear();
        assert_eq!(r.unread(), 0);
        assert!(r.next().is_none());
    }

    #[test]
    fn peticion_y_respuesta() {
        let line = request_line(0xAABB, (640, 360));
        assert!(line.ends_with('\n'));
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["m"], "screen");
        assert_eq!(v["session_id"], 0xAABB);
        assert_eq!(v["w"], 640);
        assert_eq!(v["h"], 360);
        assert_eq!(v["q"], 70);
        assert_eq!(parse_reply("{\"m\":\"screen\",\"ok\":true}\n"), Ok(()));
        assert_eq!(parse_reply("{\"m\":\"screen\",\"ok\":false}"), Err("respuesta inesperada".into()));
        assert_eq!(parse_reply("{\"m\":\"err\",\"code\":\"bad_session\",\"msg\":\"sesión desconocida\"}"), Err("sesión desconocida".into()));
        assert_eq!(parse_reply("{\"m\":\"err\",\"code\":\"bad_session\"}"), Err("bad_session".into()));
        assert_eq!(parse_reply("{\"m\":\"ok\"}"), Err("respuesta inesperada".into()), "un receptor antiguo no contesta screen");
        assert_eq!(parse_reply("nada"), Err("respuesta ilegible".into()));
    }

    #[test]
    fn tamano_que_se_pide() {
        assert_eq!(wanted_size((2000.0, 1125.0)), (854, 480), "nunca más que el nativo");
        assert_eq!(wanted_size((854.0, 480.4)), (854, 480));
        assert_eq!(wanted_size((600.0, 337.5)), (600, 338), "zona pequeña: su ancho real, alto 16:9");
        assert_eq!(wanted_size((300.6, 169.0)), (301, 169));
        assert_eq!(wanted_size((0.0, 0.0)), (854, 480), "sin zona: el nativo");
        assert_eq!(wanted_size((f32::NAN, 1.0)), (854, 480));
        assert!(!size_differs((640, 360), (646, 364)));
        assert!(size_differs((640, 360), (660, 371)));
    }

    #[test]
    fn decodifica_el_jpeg_reutilizando_las_imagenes() {
        let mut d = Decoder::default();
        let a = d.decode(&tiny_jpeg()).expect("JPEG válido");
        assert_eq!(a.size, [16, 9]);
        assert_eq!(a.pixels.len(), 16 * 9);
        assert!(near(a.pixels[0], [220, 40, 40]), "arriba a la izquierda rojo: {:?}", a.pixels[0]);
        assert!(near(a.pixels[15], [40, 60, 220]), "arriba a la derecha azul: {:?}", a.pixels[15]);
        assert!(near(a.pixels[8 * 16 + 15], [40, 60, 220]), "abajo a la derecha azul");
        // mientras alguien tiene la imagen (egui subiéndola), la siguiente es otra
        let b = d.decode(&tiny_jpeg()).unwrap();
        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(d.pool.len(), 2);
        // sueltas las dos, se reutiliza la primera libre: nada nuevo
        drop(a);
        drop(b);
        let c = d.decode(&tiny_jpeg()).unwrap();
        assert_eq!(d.pool.len(), 2, "sin reservar por trama");
        assert!(d.pool.iter().any(|p| Arc::ptr_eq(p, &c)));
        assert!(d.decode(b"no es un jpeg").is_err());
        assert!(d.decode(&tiny_jpeg()[..100]).is_err(), "truncado");
    }

    fn wait_for<T>(mut f: impl FnMut() -> Option<T>) -> T {
        let t0 = Instant::now();
        loop {
            if let Some(v) = f() {
                return v;
            }
            assert!(t0.elapsed() < Duration::from_secs(8), "tiempo agotado");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn servidor_falso_ok_jpeg_keepalive_y_ack() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
            let mut reader = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let hello: Value = serde_json::from_str(line.trim()).unwrap();
            // ok pegado a la primera imagen y a un keepalive, en un solo envío
            let mut out = b"{\"m\":\"screen\",\"ok\":true}\n".to_vec();
            out.extend(encode_frame(KIND_JPEG, &tiny_jpeg()));
            out.extend(encode_frame(KIND_KEEPALIVE, &[]));
            s.write_all(&out).unwrap();
            // la confirmación de esa imagen: un byte 0x01
            let mut ack = [0u8; 1];
            s.read_exact(&mut ack).unwrap();
            // después, un estado: la imagen deja de valer
            s.write_all(&encode_frame(KIND_STATUS, "Cemu no está abierto".as_bytes())).unwrap();
            // y al parar el cliente, el socket se cierra
            let mut rest = [0u8; 8];
            let closed = matches!(s.read(&mut rest), Ok(0) | Err(_));
            (hello, ack[0], closed)
        });

        let client = Client::start(
            Endpoint {
                host: "127.0.0.1".into(),
                port,
                session_id: 77,
            },
            (640, 360),
            egui::Context::default(),
        );
        let img = wait_for(|| client.take_image());
        assert_eq!(img.size, [16, 9]);
        assert!(near(img.pixels[0], [220, 40, 40]));
        assert_eq!(client.phase(), Phase::Streaming);
        let status = wait_for(|| client.status());
        assert_eq!(&*status, "Cemu no está abierto");
        assert!(!client.showing(), "un estado tipo 2 invalida la imagen");
        assert_eq!(&*client.placeholder(), "Cemu no está abierto");
        client.stop();
        let (hello, ack, closed) = server.join().unwrap();
        assert_eq!(hello["m"], "screen");
        assert_eq!(hello["session_id"], 77);
        assert_eq!(hello["w"], 640);
        assert_eq!(hello["h"], 360);
        assert_eq!(hello["q"], 70);
        assert_eq!(ack, 0x01);
        assert!(closed, "al parar, el servidor ve el cierre");
    }

    #[test]
    fn receptor_antiguo_que_no_contesta_es_sin_pantalla() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            // no acepta hasta que se lo digan: mientras, el cliente espera la
            // respuesta (conectando); luego lee la línea y cierra sin decir nada
            go_rx.recv().unwrap();
            let (s, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(s);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            line
        });
        let client = Client::start(
            Endpoint {
                host: "127.0.0.1".into(),
                port,
                session_id: 1,
            },
            (854, 480),
            egui::Context::default(),
        );
        assert_eq!(client.phase(), Phase::Connecting);
        assert_eq!(&*client.placeholder(), "Conectando la pantalla…");
        go_tx.send(()).unwrap();
        wait_for(|| (client.phase() == Phase::NoScreen).then_some(()));
        assert!(!client.showing());
        assert_eq!(&*client.placeholder(), "El PC no envía la pantalla");
        assert!(server.join().unwrap().contains("\"m\":\"screen\""));
        client.stop();
    }
}
