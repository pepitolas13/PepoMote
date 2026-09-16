//! Enlace con el receptor: canal de control TCP (hello/ok/err/ping/mode/pad/
//! notice/bye), socket UDP caliente (INPUT a la cadencia del sensor,
//! PING/PONG para el RTT) y el hilo de paquetes que fusiona las muestras del
//! sensor. Mismo comportamiento que LinkForegroundService + MotionEngine de
//! Android. Un mismo enlace sirve de Wiimote, de Nunchuk (PROTOCOL.md §3) o
//! de GamePad/Pro de Wii U (`cemu`) o Pro Controller (`switch`): cambian el hello y, en cada INPUT,
//! el stick con su flag o el bloque de extensión de 80 bytes.

use crate::bias::GyroBias;
use crate::buttons::Buttons;
use crate::discovery;
use crate::frame::{self, Rotation};
use crate::fusion::Madgwick;
use crate::pacing::{Pacer, State};
use crate::screen::Endpoint;
use crate::sensor::{self, Sample, Source};
use crate::store::{self, Pairing};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::tr;

const MAX_ATTEMPTS: u32 = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// Reconectando: tras este tiempo sin conseguir volver, la conexión se da
/// por perdida.
const RECONNECT_GIVE_UP: Duration = Duration::from_secs(120);

/// Espera antes del intento `attempt` (1 = el primero) de reconexión:
/// 1, 2, 4, 8, 15, 15… s.
pub fn reconnect_delay(attempt: u32) -> Duration {
    const S: [u64; 5] = [1, 2, 4, 8, 15];
    Duration::from_secs(S[(attempt.max(1) as usize - 1).min(S.len() - 1)])
}
/// Tope del protocolo: 250 Hz.
const MIN_PACKET_GAP: Duration = Duration::from_micros(3_900);
const CONTROL_PACKET_GAP: Duration = Duration::from_millis(10);
const STALE_MOTION: Duration = Duration::from_millis(50);
/// Cuánto se muestra un aviso (`notice`) en pantalla.
pub const NOTICE_SECS: u64 = 6;

/// Papel del móvil ante el receptor: mando (ausente en el hello) o Nunchuk
/// de la otra mano (`"role":"nunchuk"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Wiimote,
    Nunchuk,
}

impl Role {
    fn parse(s: Option<&str>) -> Option<Role> {
        match s {
            Some("nunchuk") => Some(Role::Nunchuk),
            Some("wiimote") => Some(Role::Wiimote),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Disconnected,
    Connecting,
    Connected {
        pc_name: String,
        mode: String,
        /// El último `mode` lo decidió el PC por su cuenta (modo automático al
        /// abrir o cerrar un emulador): no fue respuesta a nadie.
        mode_by_pc: bool,
        slot: u8,
        /// Jugador 1..4 al que pertenece esta sesión (`ok.player`; si el
        /// receptor no lo manda, slot+1).
        player: u8,
        /// Lo que el receptor dice que somos (un receptor antiguo ignora el
        /// hello y nos trata de mando).
        role: Role,
        rtt_ms: Option<f32>,
        /// El receptor anuncia `modes` con "cemu": sabe de Wii U (1.3+).
        supports_cemu: bool,
        supports_switch: bool,
        /// El receptor anuncia `modes` con "retroarch" (receptor del PC con el mando en red).
        supports_retroarch: bool,
        /// Tipo de mando efectivo: "gamepad", "pro" o "wiimote" en Wii U; "pro" en
        /// Switch; "retropad", "nes" o "gun" en RetroArch.
        pad: String,
        /// Aviso transitorio (del receptor o local) y cuándo llegó.
        notice: Option<(String, Instant)>,
        /// Cuántos `mode` (eco o difusión) y cuántos ecos `pad` han llegado
        /// en esta sesión: así la UI sabe si su petición ya tuvo respuesta.
        mode_seq: u32,
        pad_seq: u32,
        /// El receptor confirmó el Nunchuk en el mismo móvil (`ok.nunchuk`
        /// o eco de `nunchuk`).
        own_nunchuk: bool,
        /// El receptor confirmó «solo pantalla» (`ok.screen_only` o eco de
        /// `screen_only`); `None` = receptor anterior a 1.6, que no lo conoce.
        screen_only: Option<bool>,
    },
    Failed {
        code: String,
        msg: String,
    },
    /// La sesión se cayó y el enlace la rehace solo (intento `attempt`,
    /// espera creciente, dos minutos como mucho). Las pantallas del mando se
    /// quedan y el modo y el tipo de mando se reponen al volver.
    Reconnecting {
        pc_name: String,
        attempt: u32,
    },
}

impl Status {
    /// Extended input is only enabled after the receiver confirms support.
    pub fn ext_confirmed(&self) -> bool {
        matches!(self, Self::Connected { mode, role: Role::Wiimote, pad, supports_cemu, supports_switch, supports_retroarch, .. }
            if (mode == "cemu" && *supports_cemu && matches!(pad.as_str(), "gamepad" | "pro"))
            || (mode == "switch" && *supports_switch && is_switch_pad(pad))
            || (mode == "retroarch" && *supports_retroarch && pad == "retropad"))
    }

    /// Aviso aún vigente (menos de `NOTICE_SECS` desde que llegó).
    pub fn live_notice(&self) -> Option<&str> {
        match self {
            Status::Connected { notice: Some((text, at)), .. } if at.elapsed() < Duration::from_secs(NOTICE_SECS) => {
                Some(text)
            }
            _ => None,
        }
    }
}

pub struct Link {
    status: Arc<Mutex<Status>>,
    writer: Arc<Mutex<Option<TcpStream>>>,
    stop: Arc<AtomicBool>,
    sensor_hz: Arc<AtomicU32>,
    /// Host, puerto y sesión del `ok`: por donde abrir otros canales
    /// (la pantalla del GamePad).
    endpoint: Arc<Mutex<Option<Endpoint>>>,
}

impl Link {
    /// Conecta en segundo plano. `pending_mode` se manda nada más recibir `ok`.
    /// Con `Role::Nunchuk` el hello lo declara y cada INPUT lleva el stick.
    pub fn connect(
        pairing: Pairing,
        buttons: Arc<Buttons>,
        source: Box<dyn Source>,
        pending_mode: Option<String>,
        role: Role,
        own_nunchuk: bool,
        screen_only: bool,
        receiver_notices: bool,
    ) -> Link {
        let status = Arc::new(Mutex::new(Status::Connecting));
        let writer: Arc<Mutex<Option<TcpStream>>> = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let sensor_hz = Arc::new(AtomicU32::new(0));
        let endpoint = Arc::new(Mutex::new(None));
        {
            let ctx = Ctx {
                status: status.clone(),
                writer: writer.clone(),
                stop: stop.clone(),
                sensor_hz: sensor_hz.clone(),
                endpoint: endpoint.clone(),
                target: Arc::new(Mutex::new(None)),
                buttons,
                role,
                own_nunchuk,
                screen_only,
                receiver_notices,
            };
            std::thread::Builder::new()
                .name("pepomote-control".into())
                .spawn(move || control_thread(pairing, source, pending_mode, ctx))
                .expect("hilo control");
        }
        Link {
            status,
            writer,
            stop,
            sensor_hz,
            endpoint,
        }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    /// A dónde abrir el canal de la pantalla del GamePad (mismo host y
    /// puerto que el control, sesión del `ok`); `None` hasta el `ok`.
    pub fn screen_endpoint(&self) -> Option<Endpoint> {
        self.endpoint.lock().unwrap().clone()
    }

    pub fn sensor_hz(&self) -> f32 {
        f32::from_bits(self.sensor_hz.load(Ordering::Relaxed))
    }

    pub fn send_mode(&self, mode: &str) {
        send_json(&self.writer, &json!({"m":"mode","mode":mode}));
    }

    /// Elegir GamePad/Mando de Wii en Cemu, o Pro Controller en Switch. El receptor contesta con el eco del tipo efectivo; uno
    /// antiguo no contesta y no pasa nada.
    pub fn send_pad(&self, pad: &str) {
        send_json(&self.writer, &json!({"m":"pad","pad":pad}));
    }

    /// Teclado del móvil → teclado en pantalla de Cemu o Switch. Sin respuesta; un receptor antiguo lo ignora.
    pub fn send_text(&self, text: &str) {
        send_json(&self.writer, &text_message(text));
    }

    /// Nunchuk en el mismo móvil (modo Dolphin), encendido o apagado; el
    /// receptor lo confirma con el eco (uno antiguo lo ignora).
    pub fn send_nunchuk(&self, own: bool) {
        send_json(&self.writer, &json!({"m":"nunchuk","own":own}));
    }

    /// Modo Wii U: el móvil solo como pantalla táctil (pantalla completa), sí
    /// o no; el receptor lo confirma con el eco (uno antiguo lo ignora).
    pub fn send_screen_only(&self, on: bool) {
        send_json(&self.writer, &json!({"m":"screen_only","on":on}));
    }

    /// Modo RetroArch: tecla rápida (`save_state`, `rewind`…; PROTOCOL.md §3).
    /// Las de mantener llevan `down` true al pulsar y false al soltar; las de
    /// un toque, solo true. Un receptor antiguo la ignora.
    pub fn send_hotkey(&self, name: &str, down: bool) {
        send_json(&self.writer, &hotkey_message(name, down));
    }

    /// Aviso local (mismo banner que un `notice` del receptor).
    pub fn notify(&self, text: &str) {
        if let Status::Connected { notice, .. } = &mut *self.status.lock().unwrap() {
            *notice = Some((text.to_owned(), Instant::now()));
        }
    }

    pub fn disconnect(&self) {
        self.stop.store(true, Ordering::Relaxed);
        send_json(&self.writer, &json!({"m":"bye"}));
        if let Some(w) = self.writer.lock().unwrap().take() {
            let _ = w.shutdown(Shutdown::Both);
        }
        *self.status.lock().unwrap() = Status::Disconnected;
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct Ctx {
    status: Arc<Mutex<Status>>,
    writer: Arc<Mutex<Option<TcpStream>>>,
    stop: Arc<AtomicBool>,
    sensor_hz: Arc<AtomicU32>,
    endpoint: Arc<Mutex<Option<Endpoint>>>,
    /// A dónde van los INPUT ahora mismo (None entre sesiones: no se envía).
    target: Arc<Mutex<Option<Target>>>,
    buttons: Arc<Buttons>,
    role: Role,
    /// Mando con Nunchuk en el mismo móvil (ajuste): va en el hello.
    own_nunchuk: bool,
    /// Solo pantalla (Wii U, pantalla completa; ajuste): va en el hello.
    screen_only: bool,
    /// Ajuste «Avisos del PC en pantalla»: apagado, los `notice` se ignoran.
    receiver_notices: bool,
}

/// Socket UDP y sesión de la conexión viva.
#[derive(Clone)]
struct Target {
    udp: Arc<UdpSocket>,
    session_id: u32,
    /// El receptor anunció `"tilt":true` en el `ok`: entiende el apuntado por
    /// inclinación (INPUT flags bit4). Sin eso el bit no se manda nunca (un
    /// receptor anterior o el servidor Android lo descartarían).
    supports_tilt: bool,
}

/// Cómo terminó una sesión de control.
enum End {
    /// `disconnect()`: el usuario se va.
    Stopped,
    /// `err` del PC (token malo, ocupado…): no hay reintento que valga.
    Rejected,
    /// Nunca llegó el `ok`: desconectado, como siempre.
    NeverOk,
    /// Se cayó con la sesión viva: reconectar reponiendo modo y tipo de mando.
    Dropped { mode: String, pad: String },
}

/// `hello` de sesión (PROTOCOL.md §3): `role` solo si somos Nunchuk; un
/// mando no lo manda (compatibilidad con receptores anteriores). El tipo de
/// mando en Wii U no va aquí: se elige ya en la sesión con `pad`. Un mando
/// con Nunchuk en el mismo móvil lo anuncia con `"nunchuk":"own"`.
fn hello(token: &str, role: Role, own_nunchuk: bool, screen_only: bool) -> Value {
    let mut v = json!({"m":"hello","pv":1,"token":token,"name":device_name(),"model":"Linux móvil"});
    if role == Role::Nunchuk {
        v["role"] = json!("nunchuk");
    } else if own_nunchuk {
        v["nunchuk"] = json!("own");
    }
    // Solo pantalla (Wii U): un receptor antiguo lo ignora (y no lo confirma)
    if role == Role::Wiimote && screen_only {
        v["screen_only"] = json!(true);
    }
    v
}

/// Texto para el teclado en pantalla de Cemu (PROTOCOL.md §3): la cadena tal
/// cual en UTF-8; `"\n"` = Intro (aceptar) y `"\u{8}"` = borrar un carácter,
/// escapados como cualquier JSON.
pub fn text_message(text: &str) -> Value {
    json!({"m":"text","text":text})
}

/// Jugador (1..4) que anuncia el `ok`; si falta o no vale, slot+1.
fn player_of(ok: &Value, slot: u8) -> u8 {
    ok["player"]
        .as_u64()
        .filter(|p| (1..=4).contains(p))
        .map(|p| p as u8)
        .unwrap_or(slot.saturating_add(1))
}

/// El receptor sabe de Wii U si su `ok.modes` incluye "cemu".
fn supports_cemu(ok: &Value) -> bool {
    ok["modes"]
        .as_array()
        .is_some_and(|m| m.iter().any(|v| v.as_str() == Some("cemu")))
}

fn supports_switch(ok: &Value) -> bool {
    ok["modes"].as_array().is_some_and(|m| m.iter().any(|v| v.as_str() == Some("switch")))
}

fn supports_retroarch(ok: &Value) -> bool {
    ok["modes"].as_array().is_some_and(|m| m.iter().any(|v| v.as_str() == Some("retroarch")))
}

pub fn is_switch_pad(pad: &str) -> bool {
    pad == "pro"
}

/// Mandos de RetroArch (mensaje `pad`): el apaisado de dos sticks, el de NES y la pistola.
pub fn is_retro_pad(pad: &str) -> bool {
    matches!(pad, "retropad" | "nes" | "gun")
}

fn valid_pad(s: Option<&str>) -> Option<&str> {
    s.filter(|p| matches!(*p, "gamepad" | "pro" | "wiimote") || is_switch_pad(p) || is_retro_pad(p))
}

/// `{"m":"hotkey","name":…,"down":…}`: una tecla rápida de RetroArch.
pub fn hotkey_message(name: &str, down: bool) -> Value {
    json!({"m":"hotkey","name":name,"down":down})
}

#[cfg(test)]
#[test]
fn switch_pad_vocabulary_normalizes_to_pro() {
    for pad in ["pro", "joycons", "joycon_side", "joycon_r"] {
        assert_eq!(pad_of(&json!({"mode":"switch", "pad":pad}), 0), "pro");
    }
    assert_eq!(pad_of(&json!({"mode":"switch"}), 0), "pro");
    assert_eq!(pad_of(&json!({"mode":"cemu", "pad":"wiimote"}), 0), "wiimote");
    assert_eq!(pad_of(&json!({"mode":"cemu", "pad":"gamepad"}), 0), "gamepad");
}

/// Tipo de mando efectivo en modo Wii U que anuncia el `ok`; si falta
/// (receptor antiguo, que nunca confirmará `cemu`), el que tocaría: GamePad
/// para el jugador 1 y Pro para los demás.
fn pad_of(ok: &Value, slot: u8) -> String {
    if ok["mode"] == "switch" { return "pro".to_owned(); }
    if ok["mode"] == "retroarch" {
        return ok["pad"].as_str().filter(|p| is_retro_pad(p)).unwrap_or("retropad").to_owned();
    }
    valid_pad(ok["pad"].as_str())
        .unwrap_or(if ok["mode"] == "switch" || slot != 0 { "pro" } else { "gamepad" })
        .to_owned()
}

/// Mensajes que solo actualizan el estado de una sesión ya conectada: `mode`
/// (eco o difusión: se aplica igual), `pad` (eco del tipo efectivo),
/// `nunchuk` (eco del Nunchuk propio) y `notice`. Cualquier otro se ignora
/// (devuelve `false`).
fn apply_update(st: &mut Status, msg: &Value, now: Instant) -> bool {
    let Status::Connected { mode, mode_by_pc, pad, player, notice, mode_seq, pad_seq, own_nunchuk, screen_only, .. } = st else {
        return false;
    };
    match msg["m"].as_str() {
        Some("nunchuk") => {
            *own_nunchuk = msg["own"].as_bool().unwrap_or(false);
            true
        }
        Some("screen_only") => {
            *screen_only = Some(msg["on"].as_bool().unwrap_or(false));
            true
        }
        Some("mode") => {
            *mode = msg["mode"].as_str().unwrap_or("pointer").to_owned();
            if mode == "switch" { *pad = "pro".to_owned(); }
            // RetroArch solo conoce sus tres mandos; cualquier otro nombre es el RetroPad
            if mode == "retroarch" && !is_retro_pad(pad) { *pad = "retropad".to_owned(); }
            *mode_by_pc = msg["by"].as_str() == Some("pc");
            *mode_seq = mode_seq.wrapping_add(1);
            true
        }
        Some("pad") => {
            if mode == "switch" {
                *pad = "pro".to_owned();
            } else if mode == "retroarch" {
                *pad = msg["pad"].as_str().filter(|p| is_retro_pad(p)).unwrap_or("retropad").to_owned();
            } else if let Some(p) = valid_pad(msg["pad"].as_str()) {
                *pad = p.to_owned();
            }
            if let Some(p) = msg["player"].as_u64().filter(|p| (1..=4).contains(p)) {
                *player = p as u8;
            }
            *pad_seq = pad_seq.wrapping_add(1);
            true
        }
        Some("notice") => {
            if let Some(t) = msg["text"].as_str().map(str::trim).filter(|t| !t.is_empty()) {
                *notice = Some((t.to_owned(), now));
            }
            true
        }
        _ => false,
    }
}

fn send_json(writer: &Mutex<Option<TcpStream>>, v: &Value) -> bool {
    let mut guard = writer.lock().unwrap();
    let Some(w) = guard.as_mut() else {
        return false;
    };
    let mut s = v.to_string();
    s.push('\n');
    w.write_all(s.as_bytes()).is_ok()
}

fn device_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "Linux".into())
}

pub(crate) fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    (host, port)
        .to_socket_addrs()
        .map_err(|e| tr!("link.bad_addr", host, port, e))?
        .next()
        .ok_or_else(|| tr!("link.bad_addr_short", host, port))
}

/// Emparejamiento por código (bloqueante, ≤ 5 s): hello con `code`, el `ok`
/// trae el token definitivo. `fallback_name` si el receptor no manda nombre.
pub fn pair(host: &str, port: u16, code: &str, fallback_name: &str) -> Result<Pairing, String> {
    let addr = resolve(host, port)?;
    let mut s = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| tr!("link.unreachable", host, port, e))?;
    let _ = s.set_nodelay(true);
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let hello = json!({"m":"hello","pv":1,"code":code.trim(),"probe":true,"name":device_name(),"model":"Linux móvil"});
    let mut line = hello.to_string();
    line.push('\n');
    s.write_all(line.as_bytes()).map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(s.try_clone().map_err(|e| e.to_string())?);
    let mut resp = String::new();
    reader
        .read_line(&mut resp)
        .map_err(|e| tr!("link.no_reply", e))?;
    let v: Value = serde_json::from_str(resp.trim()).map_err(|_| tr!("link.unreadable").to_owned())?;
    let _ = s.write_all(b"{\"m\":\"bye\"}\n");
    let _ = s.shutdown(Shutdown::Both);
    match v["m"].as_str() {
        Some("ok") => {
            let token = v["token"]
                .as_str()
                .ok_or_else(|| tr!("link.no_token").to_owned())?
                .to_owned();
            Ok(Pairing {
                host: host.to_owned(),
                port,
                token,
                pc_name: v["name"].as_str().unwrap_or(fallback_name).to_owned(),
            })
        }
        Some("err") => Err(v["msg"].as_str().unwrap_or(tr!("link.rejected")).to_owned()),
        _ => Err(tr!("link.unexpected").to_owned()),
    }
}

fn control_thread(mut pairing: Pairing, source: Box<dyn Source>, pending_mode: Option<String>, ctx: Ctx) {
    let mut source = Some(source);
    let mut pending_mode = pending_mode;
    let mut restore_pad: Option<String> = None;
    // (cuándo se cayó, intentos) mientras se reconecta
    let mut reconnecting: Option<(Instant, u32)> = None;
    loop {
        let Some(stream) = connect_stream(&mut pairing, &ctx, &mut reconnecting) else {
            break;
        };
        match session(&pairing, stream, &mut source, pending_mode.take(), restore_pad.take(), &ctx) {
            End::Stopped | End::Rejected | End::NeverOk => break,
            End::Dropped { mode, pad } => {
                crate::app::log_line(&format!("enlace con {} caído: reconectando", pairing.pc_name));
                pending_mode = Some(mode).filter(|m| m != "pointer");
                restore_pad = Some(pad).filter(|p| p == "wiimote" || is_switch_pad(p));
                reconnecting = Some((Instant::now(), 0));
                *ctx.status.lock().unwrap() = Status::Reconnecting { pc_name: pairing.pc_name.clone(), attempt: 1 };
            }
        }
    }
    // final: también los hilos del sensor y de paquetes
    ctx.stop.store(true, Ordering::Relaxed);
}

/// Abre el TCP del control. Primera vez: 3 intentos seguidos, como siempre.
/// Reconectando: espera creciente entre intentos hasta rendirse a los dos
/// minutos; entre uno y otro se busca el PC por si cambió de IP. None = ya
/// está puesto el estado final (Failed) o el usuario se fue.
fn connect_stream(pairing: &mut Pairing, ctx: &Ctx, reconnecting: &mut Option<(Instant, u32)>) -> Option<TcpStream> {
    let mut attempt = 0;
    loop {
        if ctx.stop.load(Ordering::Relaxed) {
            return None;
        }
        attempt += 1;
        if let Some((since, tries)) = reconnecting.as_mut() {
            *tries += 1;
            if since.elapsed() > RECONNECT_GIVE_UP {
                *ctx.status.lock().unwrap() = Status::Failed {
                    code: "io".into(),
                    msg: tr!("link.lost", pairing.pc_name),
                };
                return None;
            }
            *ctx.status.lock().unwrap() = Status::Reconnecting { pc_name: pairing.pc_name.clone(), attempt: *tries };
            // espera troceada: «Salir» no debe esperar 15 s
            let until = Instant::now() + reconnect_delay(*tries);
            while Instant::now() < until {
                if ctx.stop.load(Ordering::Relaxed) {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let result = resolve(&pairing.host, pairing.port)
            .and_then(|a| TcpStream::connect_timeout(&a, CONNECT_TIMEOUT).map_err(|e| e.to_string()));
        match result {
            Ok(s) => return Some(s),
            Err(e) => {
                if reconnecting.is_none() && (attempt >= MAX_ATTEMPTS || ctx.stop.load(Ordering::Relaxed)) {
                    *ctx.status.lock().unwrap() = Status::Failed {
                        code: "io".into(),
                        msg: tr!("link.unreachable_pc", pairing.pc_name, pairing.host, pairing.port, e),
                    };
                    return None;
                }
                relocate(pairing);
                if reconnecting.is_none() {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
    }
}

/// Buscar el PC por nombre por si cambió de IP (DHCP, otra Wi-Fi) y
/// actualizar el emparejamiento solo. El token no cambia: vive en el PC.
fn relocate(pairing: &mut Pairing) {
    if let Some(r) = discovery::scan(Duration::from_millis(1200))
        .into_iter()
        .find(|r| r.name == pairing.pc_name && (r.host != pairing.host || r.port != pairing.port))
    {
        pairing.host = r.host;
        pairing.port = r.port;
        store::save(pairing);
    }
}

/// Una sesión de control sobre `stream`: hello, latido, y los mensajes del
/// receptor hasta que la conexión se acabe. `pending_mode` y `restore_pad`
/// se mandan nada más llegar el `ok`.
fn session(
    pairing: &Pairing,
    stream: TcpStream,
    source: &mut Option<Box<dyn Source>>,
    pending_mode: Option<String>,
    restore_pad: Option<String>,
    ctx: &Ctx,
) -> End {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(7)));
    let Ok(read_half) = stream.try_clone() else {
        *ctx.status.lock().unwrap() = Status::Failed {
            code: "io".into(),
            msg: "socket".into(),
        };
        return End::Rejected;
    };
    let mut reader = BufReader::new(read_half);
    *ctx.writer.lock().unwrap() = Some(stream);

    send_json(&ctx.writer, &hello(&pairing.token, ctx.role, ctx.own_nunchuk, ctx.screen_only));

    // Latido TCP 1 Hz (muere con el writer)
    {
        let writer = ctx.writer.clone();
        let stop = ctx.stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(1));
                if !send_json(&writer, &json!({"m":"ping","t":sensor::now_us()})) {
                    break;
                }
            }
        });
    }

    let session_stop = Arc::new(AtomicBool::new(false));
    let mut pending_mode = pending_mode;
    let mut restore_pad = restore_pad;
    let mut had_ok = false;
    let mut line = String::new();
    loop {
        if ctx.stop.load(Ordering::Relaxed) {
            break;
        }
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        match msg["m"].as_str() {
            Some("ok") => {
                had_ok = true;
                let session_id = msg["session_id"].as_u64().unwrap_or(0) as u32;
                let udp_port = msg["udp_port"].as_u64().map(|p| p as u16).unwrap_or(pairing.port);
                let mode = msg["mode"].as_str().unwrap_or("pointer").to_owned();
                let slot = msg["slot"].as_u64().unwrap_or(0) as u8;
                let player = player_of(&msg, slot);
                let role = Role::parse(msg["role"].as_str()).unwrap_or(ctx.role);
                let mut pc_name = pairing.pc_name.clone();
                if let Some(name) = msg["name"].as_str() {
                    if name != pairing.pc_name {
                        pc_name = name.to_owned();
                        store::save(&Pairing { pc_name: pc_name.clone(), ..pairing.clone() });
                    }
                }
                *ctx.status.lock().unwrap() = Status::Connected {
                    pc_name,
                    mode,
                    mode_by_pc: false,
                    slot,
                    player,
                    role,
                    rtt_ms: None,
                    supports_cemu: supports_cemu(&msg),
                    supports_switch: supports_switch(&msg),
                    supports_retroarch: supports_retroarch(&msg),
                    pad: pad_of(&msg, slot),
                    notice: None,
                    mode_seq: 0,
                    pad_seq: 0,
                    own_nunchuk: msg["nunchuk"].as_str() == Some("own"),
                    screen_only: msg["screen_only"].as_bool(),
                };
                // la pantalla del GamePad va por otra conexión TCP al mismo
                // puerto PMP que este control (el `udp_port` del ok es ese
                // mismo puerto), con la sesión del ok
                *ctx.endpoint.lock().unwrap() = Some(Endpoint {
                    host: pairing.host.clone(),
                    port: pairing.port,
                    session_id,
                });
                start_session_udp(&pairing.host, udp_port, session_id, msg["tilt"].as_bool() == Some(true), &session_stop, ctx);
                if let Some(src) = source.take() {
                    start_hot_path(src, ctx);
                }
                if let Some(m) = pending_mode.take() {
                    send_json(&ctx.writer, &json!({"m":"mode","mode":m}));
                }
                if let Some(p) = restore_pad.take() {
                    send_json(&ctx.writer, &json!({"m":"pad","pad":p}));
                }
            }
            Some("err") => {
                *ctx.status.lock().unwrap() = Status::Failed {
                    code: msg["code"].as_str().unwrap_or("err").to_owned(),
                    msg: msg["msg"].as_str().unwrap_or(tr!("link.rejected_by_pc")).to_owned(),
                };
                break;
            }
            Some("ping") => {
                send_json(&ctx.writer, &json!({"m":"pong","t":msg["t"]}));
            }
            // mode (eco o difundido), pad, notice… y lo desconocido se ignora
            _ => {
                // Ajuste «Avisos del PC en pantalla»: apagado, los notice ni se guardan
                if msg["m"].as_str() != Some("notice") || ctx.receiver_notices {
                    apply_update(&mut ctx.status.lock().unwrap(), &msg, Instant::now());
                }
            }
        }
    }

    // Fin de la sesión: nada más sale por este socket
    session_stop.store(true, Ordering::Relaxed);
    ctx.target.lock().unwrap().take();
    ctx.writer.lock().unwrap().take();
    ctx.buttons.release_all();
    ctx.endpoint.lock().unwrap().take();
    let mut st = ctx.status.lock().unwrap();
    if ctx.stop.load(Ordering::Relaxed) {
        if !matches!(*st, Status::Failed { .. }) {
            *st = Status::Disconnected;
        }
        return End::Stopped;
    }
    if matches!(*st, Status::Failed { .. }) {
        return End::Rejected;
    }
    if let (true, Status::Connected { mode, pad, .. }) = (had_ok, &*st) {
        return End::Dropped { mode: mode.clone(), pad: pad.clone() };
    }
    *st = Status::Disconnected;
    End::NeverOk
}

/// UDP de la sesión: socket conectado al receptor (lo usa el hilo de
/// paquetes mientras `target` lo diga) y oyente de PING/PONG, que muere con
/// la sesión.
fn start_session_udp(host: &str, port: u16, session_id: u32, supports_tilt: bool, session_stop: &Arc<AtomicBool>, ctx: &Ctx) {
    let Ok(udp) = UdpSocket::bind("0.0.0.0:0") else {
        return;
    };
    if udp.connect((host, port)).is_err() {
        return;
    }
    let _ = udp.set_read_timeout(Some(Duration::from_millis(500)));
    let udp = Arc::new(udp);
    *ctx.target.lock().unwrap() = Some(Target { udp: udp.clone(), session_id, supports_tilt });

    // Oyente: eco de PING y RTT de nuestros PING
    let stop = ctx.stop.clone();
    let session_stop = session_stop.clone();
    let status = ctx.status.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 128];
        while !stop.load(Ordering::Relaxed) && !session_stop.load(Ordering::Relaxed) {
            let Ok(n) = udp.recv(&mut buf) else { continue };
            match pmp::parse(&buf[..n]) {
                Some(pmp::Packet::Ping { session_id, t_us }) => {
                    let _ = udp.send(&pmp::build_pong(session_id, t_us));
                }
                Some(pmp::Packet::Pong { session_id: sid, t_us }) if sid == session_id => {
                    let rtt = sensor::now_us().saturating_sub(t_us) as f32 / 1000.0;
                    if rtt < 5000.0 {
                        if let Status::Connected { rtt_ms, .. } = &mut *status.lock().unwrap() {
                            *rtt_ms = Some(rtt);
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

/// Sensor → hilo de paquetes (fusión + INPUT), una vez por enlace: entre
/// sesiones (reconectando) siguen vivos y simplemente no envían.
fn start_hot_path(source: Box<dyn Source>, ctx: &Ctx) {
    let (tx, rx) = mpsc::channel::<Sample>();
    {
        let stop = ctx.stop.clone();
        std::thread::Builder::new()
            .name("pepomote-sensor".into())
            .spawn(move || source.run(tx, stop))
            .expect("hilo sensor");
    }
    {
        let stop = ctx.stop.clone();
        let buttons = ctx.buttons.clone();
        let sensor_hz = ctx.sensor_hz.clone();
        let target = ctx.target.clone();
        let role = ctx.role;
        std::thread::Builder::new()
            .name("pepomote-packets".into())
            .spawn(move || packet_loop(target, rx, buttons, stop, sensor_hz, role))
            .expect("hilo paquetes");
    }
}

/// INPUT a partir del estado fusionado y de lo que tiene el dedo. Como
/// GamePad/Pro de Wii U (`buttons.is_gamepad()`, que la UI solo activa con
/// `cemu` o `switch` confirmado): 80 bytes con FLAG_EXT (+FLAG_TOUCH con dedo
/// en la pantalla táctil), stick izquierdo en 6-7, derecho y táctil en la
/// extensión, y los sensores remapeados al marco apaisado según el giro. Si
/// Switch nunca lleva táctil. Si no hay modo extendido, 72 bytes como siempre: el Nunchuk lleva su stick con FLAG_STICK_VALID
/// y el mando manda 0,0.
/// `tilt`: pedir apuntado por inclinación (flags bit4: sin giroscopio real y
/// con un receptor que lo entiende); los sensores van igual.
#[allow(clippy::too_many_arguments)]
fn packet_from_state(
    st: &State,
    buttons: &Buttons,
    role: Role,
    now: Instant,
    session_id: u32,
    seq: u32,
    battery_pct: u8,
    tilt: bool,
) -> pmp::InputPacket {
    let quat_flag = (if st.quat_valid { pmp::FLAG_QUAT_VALID } else { 0 }) | (if tilt { pmp::FLAG_TILT } else { 0 });
    let base = pmp::InputPacket {
        session_id,
        seq,
        t_sensor_us: st.t_us,
        quat: st.quat,
        gyro: st.gyro,
        accel: st.accel,
        buttons: buttons.wire_at(now),
        recenter_count: buttons.recenter_count(),
        battery_pct,
        touch_scroll_dy: buttons.drain_scroll(),
        ..Default::default()
    };
    if buttons.is_gamepad() && role == Role::Wiimote {
        let (quat, gyro, accel) = frame::remap(st.quat, st.gyro, st.accel, Rotation::from_u8(buttons.rotation()));
        let (stick_x, stick_y) = buttons.stick();
        let (stick_rx, stick_ry) = buttons.stick2();
        let (touch_x, touch_y, touch_down) = if buttons.is_switch() { (0, 0, false) } else { buttons.touch() };
        let mut flags = quat_flag | pmp::FLAG_STICK_VALID | pmp::FLAG_EXT;
        if touch_down {
            flags |= pmp::FLAG_TOUCH;
        }
        return pmp::InputPacket {
            flags,
            quat,
            gyro,
            accel,
            stick_x,
            stick_y,
            stick_rx,
            stick_ry,
            touch_x,
            touch_y,
            ..base
        };
    }
    let (flags, (stick_x, stick_y)) = match role {
        Role::Nunchuk => (quat_flag | pmp::FLAG_STICK_VALID, buttons.stick()),
        Role::Wiimote => (quat_flag, (0, 0)),
    };
    pmp::InputPacket {
        flags,
        stick_x,
        stick_y,
        ..base
    }
}

fn packet_loop(
    target: Arc<Mutex<Option<Target>>>,
    rx: mpsc::Receiver<Sample>,
    buttons: Arc<Buttons>,
    stop: Arc<AtomicBool>,
    sensor_hz: Arc<AtomicU32>,
    role: Role,
) {
    let mut fusion = Madgwick::new(0.1);
    let mut bias = GyroBias::new();
    let mut pacer = Pacer::new();
    let epoch = Instant::now();
    let wall_us = |now: Instant| now.duration_since(epoch).as_micros() as u64;
    let mut seq: u32 = 0;
    let mut last_t_us: Option<u64> = None;
    let mut next_send = Instant::now();
    let mut source_closed = false;
    let mut last_packet_t_us = 0;
    let mut has_rotation = false;
    let mut gyro_available = false;
    let mut last_sample_at: Option<Instant> = None;
    let mut last_ping = Instant::now();
    let mut battery = Battery::new();
    let mut hz_window = Instant::now();
    let mut hz_count: u32 = 0;
    let mut bias_logged = false;
    let mut delay_logged: u64 = 0;

    while !stop.load(Ordering::Relaxed) {
        // 1) Muestras: se procesan todas las que lleguen hasta que toque enviar
        let wait = next_send.saturating_duration_since(Instant::now());
        let incoming = if source_closed {
            std::thread::sleep(wait);
            Err(mpsc::RecvTimeoutError::Timeout)
        } else {
            rx.recv_timeout(wait)
        };
        match incoming {
            Ok(sample) => {
                let now = Instant::now();
                last_sample_at = Some(now);
                let dt = last_t_us
                    .map(|t| sample.t_us.saturating_sub(t) as f32 / 1e6)
                    .filter(|dt| (1e-5..0.25).contains(dt))
                    .unwrap_or(0.005);
                last_t_us = Some(sample.t_us);
                gyro_available = sample.gyro_valid;
                let gyro = if sample.gyro_valid {
                    let corrected = bias.correct(sample.t_us, sample.gyro, sample.accel);
                    fusion.update(corrected, sample.accel, dt);
                    has_rotation = true;
                    corrected
                } else {
                    // Missing hardware is not a negative bias measurement.
                    [0.0; 3]
                };
                pacer.push(
                    State {
                        t_us: sample.t_us,
                        quat: fusion.quat(),
                        quat_valid: has_rotation,
                        gyro,
                        accel: sample.accel,
                    },
                    wall_us(now),
                );
                hz_count += 1;
                if hz_window.elapsed() >= Duration::from_secs(1) {
                    sensor_hz.store((hz_count as f32 / hz_window.elapsed().as_secs_f32()).to_bits(), Ordering::Relaxed);
                    hz_window = Instant::now();
                    hz_count = 0;
                    // diagnóstico en mobile.log: bias adoptado y ráfagas detectadas
                    if !bias_logged && bias.settled() {
                        bias_logged = true;
                        let b = bias.bias();
                        crate::app::log_line(&format!("bias gyro asentado: [{:.4} {:.4} {:.4}] rad/s", b[0], b[1], b[2]));
                    }
                    let d = pacer.delay_us();
                    if d.abs_diff(delay_logged) >= 5_000 {
                        delay_logged = d;
                        crate::app::log_line(&format!("entrega del sensor a ráfagas: reproducción con {} ms de retardo", d / 1000));
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => source_closed = true,
        }

        // 2) Envío a ritmo fijo (tope del protocolo), interpolado con el
        //    reloj de pared: las ráfagas del sensor no llegan al receptor
        let now = Instant::now();
        if stop.load(Ordering::Relaxed) { break; }
        if now < next_send { continue; }
        // SSC may deliver 80 ms bursts. Its real samples remain valid while
        // the pacer is playing the buffered interval, plus the normal grace.
        let stale_after = STALE_MOTION + Duration::from_micros(pacer.delay_us());
        let idle = last_sample_at.is_none_or(|at| now.duration_since(at) >= stale_after);
        next_send = now + if idle || !gyro_available { CONTROL_PACKET_GAP } else { MIN_PACKET_GAP };
        // sin sesión (reconectando): se fusiona igual, pero no se envía
        let Some(t) = target.lock().unwrap().clone() else {
            continue;
        };
        if idle { sensor_hz.store(0f32.to_bits(), Ordering::Relaxed); }
        let mut st = pacer.output(wall_us(now)).unwrap_or(State {
            t_us: sensor::now_us(),
            quat: fusion.quat(),
            quat_valid: has_rotation,
            gyro: [0.0; 3],
            accel: [0.0; 3],
        });
        if idle {
            st.gyro = [0.0; 3];
            st.accel = [0.0; 3];
        }
        if !gyro_available {
            // An accel-only sample must not interpolate an old angular velocity
            // or an old orientation after the real gyro becomes unavailable.
            st.gyro = [0.0; 3];
            st.quat = fusion.quat();
        }
        // Controls have their own clock even when the last sensor timestamp
        // freezes. Preserve the source time domain across stall/resume.
        let clock_us = last_t_us.zip(last_sample_at)
            .map(|(t, at)| t.saturating_add(now.duration_since(at).as_micros() as u64))
            .unwrap_or_else(sensor::now_us);
        st.t_us = clock_us.max(last_packet_t_us + 1);
        last_packet_t_us = st.t_us;
        seq = seq.wrapping_add(1);
        // Inclinación: sin giroscopio real (has_rotation es pegajoso: una
        // muestra suelta sin gyro del SSC no es que se haya ido) y solo si el
        // receptor la anunció
        let packet = packet_from_state(&st, &buttons, role, now, t.session_id, seq, battery.pct(), t.supports_tilt && !has_rotation);
        let _ = t.udp.send(&pmp::build_input(&packet));

        if last_ping.elapsed() >= Duration::from_secs(1) {
            last_ping = Instant::now();
            let _ = t.udp.send(&pmp::build_ping(t.session_id, sensor::now_us()));
        }
    }
}

/// Batería por /sys/class/power_supply (leída cada 5 s).
struct Battery {
    pct: u8,
    read_at: Option<Instant>,
}

impl Battery {
    fn new() -> Self {
        Self {
            pct: 100,
            read_at: None,
        }
    }

    fn pct(&mut self) -> u8 {
        if self.read_at.is_some_and(|t| t.elapsed() < Duration::from_secs(5)) {
            return self.pct;
        }
        self.read_at = Some(Instant::now());
        if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
            for e in entries.flatten() {
                let dir = e.path();
                let is_battery = std::fs::read_to_string(dir.join("type"))
                    .map(|t| t.trim() == "Battery")
                    .unwrap_or(false);
                if !is_battery {
                    continue;
                }
                if let Some(v) = std::fs::read_to_string(dir.join("capacity"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u8>().ok())
                {
                    self.pct = v.min(100);
                    break;
                }
            }
        }
        self.pct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercise the real packet worker/socket with either a silent, dead or
    /// stopped sensor channel. No sensor callbacks are needed for controls.
    fn fallback_packets(disconnect_source: bool, stop_readings: bool) {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver.set_read_timeout(Some(Duration::from_millis(250))).unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.connect(receiver.local_addr().unwrap()).unwrap();
        let target = Arc::new(Mutex::new(Some(Target { udp: Arc::new(sender), session_id: 7, supports_tilt: false })));
        let buttons = Arc::new(Buttons::new());
        buttons.set_gamepad(true);
        buttons.set_switch(true);
        buttons.set_rotation(2);
        let stop = Arc::new(AtomicBool::new(false));
        let hz = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::channel();
        if stop_readings {
            tx.send(Sample { t_us: sensor::now_us(), gyro_valid: true, gyro: [1.0, 2.0, 3.0], accel: [0.0, 0.0, 9.8] }).unwrap();
        }
        let source = if disconnect_source { drop(tx); None } else { Some(tx) };
        let worker = {
            let (target, buttons, stop, hz) = (target.clone(), buttons.clone(), stop.clone(), hz.clone());
            std::thread::spawn(move || packet_loop(target, rx, buttons, stop, hz, Role::Wiimote))
        };
        let receive = || {
            let mut bytes = [0; 128];
            loop {
                let size = receiver.recv(&mut bytes).expect("input packets must continue without sensor callbacks");
                if let Some(pmp::Packet::Input(packet)) = pmp::parse(&bytes[..size]) { break packet; }
            }
        };
        // Stop/join even when an assertion fails, so no worker leaks into another test.
        let result = std::panic::catch_unwind(|| {
            let mut previous = receive();
            assert_eq!(previous.session_id, 7);
            let before = Instant::now();
            for _ in 0..8 { previous = receive(); }
            assert!(before.elapsed() < Duration::from_millis(250), "fallback must be responsive, not a 1 Hz keepalive");
            if !stop_readings {
                assert_eq!(previous.gyro, [0.0; 3]);
                assert_eq!(previous.accel, [0.0; 3]);
                assert_eq!(previous.quat, [1.0, 0.0, 0.0, 0.0]);
                assert_eq!(previous.flags & pmp::FLAG_QUAT_VALID, 0);
            }
            buttons.set(pmp::BTN_A, true);
            buttons.set_stick(110, -45);
            buttons.set_stick2(-35, 90);
            let mut down = receive();
            for _ in 0..4 { if down.buttons & pmp::BTN_A != 0 { break; } down = receive(); }
            assert_ne!(down.buttons & pmp::BTN_A, 0);
            assert_eq!((down.stick_x, down.stick_y, down.stick_rx, down.stick_ry), (110, -45, -35, 90));
            buttons.set(pmp::BTN_A, false);
            let mut released = receive();
            for _ in 0..20 { if released.buttons & pmp::BTN_A == 0 { break; } released = receive(); }
            assert_eq!(released.buttons & pmp::BTN_A, 0);
            assert!(released.t_sensor_us > down.t_sensor_us);
            if stop_readings {
                // A frozen angular velocity must never keep rotating the controller.
                for _ in 0..12 { previous = receive(); }
                assert_eq!(previous.gyro, [0.0; 3]);
                assert_eq!(previous.accel, [0.0; 3]);
                assert_eq!(f32::from_bits(hz.load(Ordering::Relaxed)), 0.0);
            }
            // The same worker survives a control-session replacement.
            target.lock().unwrap().as_mut().unwrap().session_id = 8;
            let mut resumed = receive();
            for _ in 0..4 { if resumed.session_id == 8 { break; } resumed = receive(); }
            assert_eq!(resumed.session_id, 8);
        });
        stop.store(true, Ordering::Relaxed);
        drop(source);
        worker.join().unwrap();
        if let Err(error) = result { std::panic::resume_unwind(error); }
    }

    #[test]
    fn no_sensor_readings_still_send_buttons_sticks_and_releases() { fallback_packets(false, false); }

    #[test]
    fn failed_sensor_source_still_sends_buttons_sticks_and_releases() { fallback_packets(true, false); }

    #[test]
    fn stopped_sensor_readings_clear_stale_motion_and_keep_controls_live() { fallback_packets(false, true); }

    /// Sin giroscopio real (solo acelerómetro) el emisor pide apuntado por
    /// inclinación (flags bit4), pero solo si el receptor lo anunció en el
    /// `ok`; y en cuanto aparece un gyro de verdad el bit se apaga y no
    /// vuelve aunque el SSC cuele una muestra suelta sin gyro.
    fn tilt_packets(supports_tilt: bool) {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver.set_read_timeout(Some(Duration::from_millis(250))).unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.connect(receiver.local_addr().unwrap()).unwrap();
        let target = Arc::new(Mutex::new(Some(Target { udp: Arc::new(sender), session_id: 5, supports_tilt })));
        let buttons = Arc::new(Buttons::new());
        let stop = Arc::new(AtomicBool::new(false));
        let hz = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::channel();
        let worker = {
            let (target, buttons, stop, hz) = (target.clone(), buttons.clone(), stop.clone(), hz.clone());
            std::thread::spawn(move || packet_loop(target, rx, buttons, stop, hz, Role::Wiimote))
        };
        let receive = || {
            let mut bytes = [0; 128];
            loop {
                let size = receiver.recv(&mut bytes).expect("input packets must keep flowing");
                if let Some(pmp::Packet::Input(packet)) = pmp::parse(&bytes[..size]) { break packet; }
            }
        };
        let accel_only = |t_us| Sample { t_us, gyro_valid: false, gyro: [0.0; 3], accel: [0.0, 0.0, 9.8] };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let t0 = sensor::now_us();
            for i in 0..8 {
                tx.send(accel_only(t0 + i * 5_000)).unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
            let mut p = receive();
            for _ in 0..40 {
                if (p.flags & pmp::FLAG_TILT != 0) == supports_tilt && p.accel[2] > 9.0 { break; }
                p = receive();
            }
            assert!(p.accel[2] > 9.0, "el acelerómetro llega tal cual");
            assert_eq!(p.flags & pmp::FLAG_QUAT_VALID, 0, "sin gyro no hay quaternion");
            if supports_tilt {
                assert_ne!(p.flags & pmp::FLAG_TILT, 0, "sin gyro real se pide inclinación");
            } else {
                assert_eq!(p.flags & pmp::FLAG_TILT, 0, "sin `tilt` en el ok el bit no sale nunca");
            }
            // aparece el gyro de verdad: fuera la inclinación
            let t1 = sensor::now_us();
            for i in 0..8 {
                tx.send(Sample { t_us: t1 + i * 5_000, gyro_valid: true, gyro: [0.0, 0.0, 0.1], accel: [0.0, 0.0, 9.8] }).unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
            let mut p = receive();
            for _ in 0..40 {
                if p.flags & pmp::FLAG_QUAT_VALID != 0 { break; }
                p = receive();
            }
            assert_ne!(p.flags & pmp::FLAG_QUAT_VALID, 0, "con gyro hay quaternion");
            assert_eq!(p.flags & pmp::FLAG_TILT, 0, "con gyro real, sin inclinación");
            // una muestra suelta sin gyro (SSC) no la reactiva: el gyro real es pegajoso
            tx.send(accel_only(sensor::now_us())).unwrap();
            std::thread::sleep(Duration::from_millis(30));
            for _ in 0..4 {
                let p = receive();
                assert_eq!(p.flags & pmp::FLAG_TILT, 0, "una muestra sin gyro no es que el gyro se haya ido");
            }
        }));
        stop.store(true, Ordering::Relaxed);
        drop(tx);
        worker.join().unwrap();
        if let Err(error) = result { std::panic::resume_unwind(error); }
    }

    #[test]
    fn accel_only_sensor_requests_tilt_when_the_receiver_supports_it() { tilt_packets(true); }

    #[test]
    fn accel_only_sensor_never_requests_tilt_from_an_old_receiver() { tilt_packets(false); }

    #[test]
    fn ssc_bursts_keep_buffered_motion_until_the_pacer_has_played_it() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver.set_read_timeout(Some(Duration::from_millis(250))).unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.connect(receiver.local_addr().unwrap()).unwrap();
        let target = Arc::new(Mutex::new(Some(Target { udp: Arc::new(sender), session_id: 9, supports_tilt: false })));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let burst = |tx: &mpsc::Sender<Sample>, batch: u64| {
            for i in 0..16 {
                tx.send(Sample {
                    t_us: 2_000_000 + batch * 80_000 + i * 5_000,
                    gyro_valid: true, gyro: [0.0, 0.0, 0.5], accel: [0.0, 0.0, 9.8],
                }).unwrap();
            }
        };
        burst(&tx, 0);
        let worker = {
            let stop = stop.clone();
            std::thread::spawn(move || packet_loop(target, rx, Arc::new(Buttons::new()), stop, Arc::new(AtomicU32::new(0)), Role::Wiimote))
        };
        let producer = std::thread::spawn(move || {
            for batch in 1..4 {
                std::thread::sleep(Duration::from_millis(80));
                burst(&tx, batch);
            }
            tx
        });
        let result = std::panic::catch_unwind(|| {
            let started = Instant::now();
            let mut inputs = 0;
            let mut wire = [0; 128];
            while started.elapsed() < Duration::from_millis(290) {
                let size = receiver.recv(&mut wire).unwrap();
                if let Some(pmp::Packet::Input(packet)) = pmp::parse(&wire[..size]) {
                    assert!((packet.gyro[2] - 0.5).abs() < 0.001, "80 ms sensor batches still have buffered real gyro: {packet:?}");
                    assert!((packet.accel[2] - 9.8).abs() < 0.001);
                    inputs += 1;
                }
            }
            assert!(inputs >= 10, "packet timing also depends on the host timer resolution");
        });
        let source = producer.join().unwrap();
        stop.store(true, Ordering::Relaxed);
        drop(source);
        worker.join().unwrap();
        if let Err(error) = result { std::panic::resume_unwind(error); }
    }

    #[test]
    fn switch_capability_and_extension_are_confirmed_together() {
        assert!(supports_switch(&json!({"modes":["pointer","dolphin","cemu","switch"]})));
        for ok in [json!({}), json!({"modes":"switch"}), json!({"modes":["cemu"]})] {
            assert!(!supports_switch(&ok));
        }
        let mut status = connected();
        apply_update(&mut status, &json!({"m":"mode","mode":"switch"}), Instant::now());
        apply_update(&mut status, &json!({"m":"pad","pad":"joycon_r","half":"right"}), Instant::now());
        assert!(status.ext_confirmed());
        if let Status::Connected { supports_switch, .. } = &mut status { *supports_switch = false; }
        assert!(!status.ext_confirmed());
        if let Status::Connected { supports_switch, role, .. } = &mut status { *supports_switch = true; *role = Role::Nunchuk; }
        assert!(!status.ext_confirmed());
        assert!(!Status::Connecting.ext_confirmed());
    }

    #[test]
    fn retired_switch_assignments_normalize_to_independent_pro() {
        let mut status = connected();
        let now = Instant::now();
        apply_update(&mut status, &json!({"m":"mode","mode":"switch"}), now);
        for old in ["joycons", "joycon_side", "joycon_r", "pro"] {
            apply_update(&mut status, &json!({"m":"pad","pad":old,"half":"left","side":"right","player":3}), now);
            assert!(matches!(&status, Status::Connected { pad, player: 3, .. } if pad == "pro"));
        }
        apply_update(&mut status, &json!({"m":"mode","mode":"cemu"}), now);
        apply_update(&mut status, &json!({"m":"pad","pad":"wiimote"}), now);
        assert!(matches!(&status, Status::Connected { pad, .. } if pad == "wiimote"));
    }

    #[test]
    fn player_number_updates_with_pad_echo() {
        let mut status = connected();
        let now = Instant::now();
        apply_update(&mut status, &json!({"m":"pad","pad":"pro","player":2}), now);
        assert!(matches!(status, Status::Connected { player: 2, .. }));
        apply_update(&mut status, &json!({"m":"pad","pad":"pro"}), now);
        assert!(matches!(status, Status::Connected { player: 2, .. }), "old receivers can omit player");
        apply_update(&mut status, &json!({"m":"pad","pad":"pro","player":0}), now);
        assert!(matches!(status, Status::Connected { player: 2, .. }));
    }

    #[test]
    fn switch_wire_has_capture_both_sticks_and_no_cemu_touch() {
        let b = Buttons::new();
        b.set_gamepad(true);
        b.set_switch(true);
        b.set_rotation(2);
        b.set(pmp::BTN_SCREEN, true);
        b.set_stick(100, -50);
        b.set_stick2(-30, 120);
        b.set_touch(0x8000, 0x4000, true); // stale Cemu touch must never leak
        let p = packet_from_state(&wiiu_state(), &b, Role::Wiimote, Instant::now(), 123, 1, 90, false);
        let wire = pmp::build_input(&p);
        assert_eq!(wire.len(), 80);
        assert_eq!(&wire[6..8], &[100, (-50_i8) as u8]);
        assert_eq!(&wire[72..74], &[(-30_i8) as u8, 120]);
        assert_eq!(&wire[74..80], &[0; 6]);
        assert_eq!(p.flags & pmp::FLAG_TOUCH, 0);
        assert_eq!(p.buttons, 1 << 28);
        assert_eq!(p.quat, wiiu_state().quat, "an unknown rotation preserves the phone frame");
        b.set_stick(0, 0);
        let p = packet_from_state(&wiiu_state(), &b, Role::Wiimote, Instant::now(), 123, 2, 90, false);
        assert_eq!((p.stick_x, p.stick_y), (0, 0));
        assert_eq!((p.stick_rx, p.stick_ry), (-30, 120), "the right stick uses the extension block");
    }

    #[test]
    fn hello_declara_el_papel_solo_en_el_nunchuk() {
        let w = hello("tok", Role::Wiimote, false, false);
        assert_eq!(w["m"], "hello");
        assert_eq!(w["pv"], 1);
        assert_eq!(w["token"], "tok");
        assert!(w.get("role").is_none(), "un mando no manda role (receptores anteriores)");
        assert!(w.get("pad").is_none(), "el tipo de mando de Wii U se elige en la sesión, no en el hello");
        assert!(w.get("nunchuk").is_none(), "sin Nunchuk propio no se manda nada (receptores anteriores)");
        let n = hello("tok", Role::Nunchuk, true, true);
        assert_eq!(n["role"], "nunchuk");
        assert_eq!(n["token"], "tok");
        assert!(n.get("pad").is_none());
        assert!(n.get("nunchuk").is_none(), "un Nunchuk (rol) nunca lleva Nunchuk propio");
        let own = hello("tok", Role::Wiimote, true, false);
        assert_eq!(own["nunchuk"], "own");
        assert!(own.get("role").is_none());
    }

    #[test]
    fn mensaje_de_texto_para_cemu() {
        let m = text_message("Link");
        assert_eq!(m["m"], "text");
        assert_eq!(m["text"], "Link");
        assert_eq!(text_message("Zelda\n").to_string(), r#"{"m":"text","text":"Zelda\n"}"#, "Intro escapado como JSON normal");
        assert_eq!(text_message("\u{8}").to_string(), r#"{"m":"text","text":"\b"}"#, "borrar: U+0008 escapado");
        // el receptor lo lee con serde_json: ida y vuelta intacta
        let back: Value = serde_json::from_str(&text_message("ñ \u{8}\n").to_string()).unwrap();
        assert_eq!(back["text"].as_str(), Some("ñ \u{8}\n"));
        assert_eq!(text_message("").to_string(), r#"{"m":"text","text":""}"#);
    }

    #[test]
    fn jugador_del_ok_o_slot_mas_uno() {
        assert_eq!(player_of(&json!({"m":"ok","slot":3,"player":1}), 3), 1, "Nunchuk del jugador 1 en el slot 3");
        assert_eq!(player_of(&json!({"m":"ok","slot":1}), 1), 2, "receptor sin player: slot+1");
        assert_eq!(player_of(&json!({"m":"ok","player":0}), 0), 1, "fuera de 1..4: se ignora");
        assert_eq!(player_of(&json!({"m":"ok","player":9}), 2), 3);
        assert_eq!(player_of(&json!({"m":"ok","player":"2"}), 0), 1, "tipo malo: se ignora");
    }

    #[test]
    fn papel_que_anuncia_el_receptor() {
        assert_eq!(Role::parse(Some("nunchuk")), Some(Role::Nunchuk));
        assert_eq!(Role::parse(Some("wiimote")), Some(Role::Wiimote));
        assert_eq!(Role::parse(Some("otro")), None);
        assert_eq!(Role::parse(None), None);
    }

    #[test]
    fn modos_y_tipo_de_mando_del_ok() {
        let new = json!({"m":"ok","modes":["pointer","dolphin","cemu"],"pad":"pro","slot":1});
        assert!(supports_cemu(&new));
        assert_eq!(pad_of(&new, 1), "pro");
        let old = json!({"m":"ok","mode":"pointer","slot":0});
        assert!(!supports_cemu(&old), "sin modes: receptor anterior a 1.3");
        assert!(!supports_cemu(&json!({"m":"ok","modes":["pointer","dolphin"]})));
        assert!(!supports_cemu(&json!({"m":"ok","modes":"cemu"})), "tipo malo: se ignora");
        assert_eq!(pad_of(&old, 0), "gamepad", "jugador 1 sin pad: GamePad");
        assert_eq!(pad_of(&old, 2), "pro", "los demás: Pro");
        assert_eq!(pad_of(&json!({"m":"ok","pad":"raro"}), 0), "gamepad", "pad desconocido: como si faltara");
        assert_eq!(pad_of(&json!({"m":"ok","pad":"wiimote"}), 0), "wiimote", "el receptor manda");
    }

    fn connected() -> Status {
        Status::Connected {
            pc_name: "PC".into(),
            mode: "pointer".into(),
            mode_by_pc: false,
            slot: 0,
            player: 1,
            role: Role::Wiimote,
            rtt_ms: None,
            supports_cemu: true,
            supports_switch: true,
            supports_retroarch: true,
            pad: "gamepad".into(),
            notice: None,
            mode_seq: 0,
            pad_seq: 0,
            own_nunchuk: false,
            screen_only: None,
        }
    }

    #[test]
    fn retroarch_capability_pads_and_hotkeys() {
        assert!(supports_retroarch(&json!({"modes":["pointer","dolphin","cemu","switch","retroarch"]})));
        for ok in [json!({}), json!({"modes":"retroarch"}), json!({"modes":["cemu","switch"]})] {
            assert!(!supports_retroarch(&ok));
        }
        // el ok con RetroArch trae uno de sus tres mandos; otra cosa es el RetroPad
        assert_eq!(pad_of(&json!({"mode":"retroarch","pad":"nes"}), 0), "nes");
        assert_eq!(pad_of(&json!({"mode":"retroarch","pad":"gun"}), 2), "gun");
        assert_eq!(pad_of(&json!({"mode":"retroarch","pad":"pro"}), 0), "retropad");
        assert_eq!(pad_of(&json!({"mode":"retroarch"}), 1), "retropad");
        // ecos en modo RetroArch
        let mut st = connected();
        let now = Instant::now();
        apply_update(&mut st, &json!({"m":"mode","mode":"retroarch"}), now);
        assert!(matches!(&st, Status::Connected { pad, .. } if pad == "retropad"), "al entrar, RetroPad");
        apply_update(&mut st, &json!({"m":"pad","pad":"gun","player":2}), now);
        assert!(matches!(&st, Status::Connected { pad, player: 2, .. } if pad == "gun"));
        apply_update(&mut st, &json!({"m":"pad","pad":"wiimote"}), now);
        assert!(matches!(&st, Status::Connected { pad, .. } if pad == "retropad"), "un mando ajeno es el RetroPad");
        assert!(st.ext_confirmed(), "RetroPad confirmado: 80 bytes");
        apply_update(&mut st, &json!({"m":"pad","pad":"nes"}), now);
        assert!(!st.ext_confirmed(), "el mando de NES va en 72 bytes");
        if let Status::Connected { supports_retroarch, pad, .. } = &mut st { *supports_retroarch = false; *pad = "retropad".into(); }
        assert!(!st.ext_confirmed(), "sin RetroArch en el receptor nunca hay 80 bytes");
        // el mensaje de tecla rápida
        assert_eq!(hotkey_message("save_state", true), json!({"m":"hotkey","name":"save_state","down":true}));
        assert_eq!(hotkey_message("rewind", false)["down"], false);
    }

    #[test]
    fn hello_anuncia_solo_pantalla_solo_en_el_mando() {
        let v = hello("tok", Role::Wiimote, false, true);
        assert_eq!(v["screen_only"], json!(true));
        assert!(v.get("nunchuk").is_none());
        let n = hello("tok", Role::Nunchuk, false, true);
        assert!(n.get("screen_only").is_none(), "un Nunchuk nunca es solo pantalla");
        let w = hello("tok", Role::Wiimote, false, false);
        assert!(w.get("screen_only").is_none(), "apagado: ausente, como un móvil anterior");
    }

    #[test]
    fn el_eco_de_solo_pantalla_actualiza_la_sesion() {
        let mut st = connected();
        assert!(matches!(&st, Status::Connected { screen_only: None, .. }), "sin el campo en el ok: None (receptor antiguo)");
        let now = std::time::Instant::now();
        assert!(apply_update(&mut st, &json!({"m":"screen_only","on":true}), now));
        assert!(matches!(&st, Status::Connected { screen_only: Some(true), .. }));
        assert!(apply_update(&mut st, &json!({"m":"screen_only","on":false}), now));
        assert!(matches!(&st, Status::Connected { screen_only: Some(false), .. }));
    }

    #[test]
    fn mode_pad_y_notice_actualizan_la_sesion_y_lo_demas_se_ignora() {
        let mut st = connected();
        let now = Instant::now();
        assert!(apply_update(&mut st, &json!({"m":"mode","mode":"cemu"}), now), "difundido sin pedirlo: se aplica");
        assert!(matches!(&st, Status::Connected { mode, mode_seq: 1, pad_seq: 0, .. } if mode == "cemu"));
        assert!(apply_update(&mut st, &json!({"m":"mode","mode":"cemu"}), now));
        assert!(matches!(&st, Status::Connected { mode_seq: 2, .. }), "cada eco cuenta, aunque repita el modo");
        assert!(apply_update(&mut st, &json!({"m":"mode","mode":"dolphin","by":"pc"}), now));
        assert!(
            matches!(&st, Status::Connected { mode, mode_by_pc: true, mode_seq: 3, .. } if mode == "dolphin"),
            "difusión del PC (modo automático): se aplica y se recuerda quién lo decidió"
        );
        assert!(apply_update(&mut st, &json!({"m":"mode","mode":"cemu"}), now));
        assert!(matches!(&st, Status::Connected { mode_by_pc: false, mode_seq: 4, .. }), "un eco normal lo deja en falso");
        assert!(apply_update(&mut st, &json!({"m":"pad","pad":"wiimote"}), now));
        assert!(matches!(&st, Status::Connected { pad, pad_seq: 1, .. } if pad == "wiimote"));
        assert!(apply_update(&mut st, &json!({"m":"pad","pad":"nada"}), now));
        assert!(matches!(&st, Status::Connected { pad, pad_seq: 2, .. } if pad == "wiimote"), "eco inválido: se conserva el tipo, pero cuenta como respuesta");
        assert!(apply_update(&mut st, &json!({"m":"notice","text":" Cemu está abierto "}), now));
        assert!(matches!(&st, Status::Connected { notice: Some((t, at)), .. } if t == "Cemu está abierto" && *at == now));
        assert_eq!(st.live_notice(), Some("Cemu está abierto"));
        let before = st.clone();
        assert!(!apply_update(&mut st, &json!({"m":"desconocido","x":1}), now), "m desconocido: no rompe nada");
        assert!(!apply_update(&mut st, &json!({"sin":"m"}), now));
        assert_eq!(st, before);
        assert!(apply_update(&mut st, &json!({"m":"notice","text":""}), now));
        assert_eq!(st, before, "aviso vacío: nada que mostrar, se conserva el anterior");
        let mut off = Status::Disconnected;
        assert!(!apply_update(&mut off, &json!({"m":"mode","mode":"cemu"}), now), "sin sesión no hay nada que actualizar");
        assert_eq!(off, Status::Disconnected);
    }

    #[test]
    fn el_aviso_caduca() {
        let mut st = connected();
        let old = Instant::now() - Duration::from_secs(NOTICE_SECS + 1);
        assert!(apply_update(&mut st, &json!({"m":"notice","text":"viejo"}), old));
        assert_eq!(st.live_notice(), None);
        assert_eq!(Status::Connecting.live_notice(), None);
    }

    fn from_hex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Estado del contrato (vector dorado input_wiiu): sticks (100,−50) y
    /// (−30,120), táctil (0x8000,0x4000), A|X|Y|L|R|ZL|ZR|L3|R3|Mic|Pantalla,
    /// recentrado 2.
    fn wiiu_buttons() -> Buttons {
        let b = Buttons::new();
        b.set_gamepad(true);
        b.set_stick(100, -50);
        b.set_stick2(-30, 120);
        b.set_touch(0x8000, 0x4000, true);
        for bit in [
            pmp::BTN_A,
            pmp::BTN_X,
            pmp::BTN_Y,
            pmp::BTN_L,
            pmp::BTN_R,
            pmp::BTN_ZL,
            pmp::BTN_ZR,
            pmp::BTN_STICK_L,
            pmp::BTN_STICK_R,
            pmp::BTN_MIC,
            pmp::BTN_SCREEN,
        ] {
            b.set(bit, true);
        }
        b.bump_recenter();
        b.bump_recenter();
        b
    }

    fn wiiu_state() -> State {
        State {
            t_us: 5_000_000,
            quat: [1.0, 0.0, 0.0, 0.0],
            quat_valid: true,
            gyro: [0.0; 3],
            accel: [0.0, 0.0, 9.5],
        }
    }

    #[test]
    fn gamepad_construye_los_80_bytes_del_vector_dorado() {
        let golden = from_hex(include_str!("../../protocol/vectors/input_wiiu.hex"));
        assert_eq!(golden.len(), pmp::INPUT_EXT_LEN);
        let b = wiiu_buttons();
        // borde superior a la izquierda (por defecto): los sensores se remapean
        let p = packet_from_state(&wiiu_state(), &b, Role::Wiimote, Instant::now(), 0xAABBCCDD, 11, 66, false);
        assert_eq!(p.flags, 0x0F, "QUAT|STICK|EXT|TOUCH");
        assert_eq!(p.buttons, 0x1FF8_0001);
        assert_eq!((p.stick_x, p.stick_y), (100, -50));
        assert_eq!((p.stick_rx, p.stick_ry), (-30, 120));
        assert_eq!((p.touch_x, p.touch_y), (0x8000, 0x4000));
        assert_eq!(p.accel, [0.0, 0.0, 9.5], "plano: la gravedad no cambia con el giro");
        let q = p.quat;
        let h = std::f32::consts::FRAC_1_SQRT_2;
        assert!((q[0] - h).abs() < 1e-6 && q[1] == 0.0 && q[2] == 0.0 && (q[3] + h).abs() < 1e-6, "{q:?}");
        let out = pmp::build_input(&p);
        assert_eq!(out.len(), pmp::INPUT_EXT_LEN);
        // todo menos el quaternion (remapeado) coincide byte a byte con el vector
        assert_eq!(&out[..24], &golden[..24], "cabecera, flags, stick izquierdo, sesión, seq, t");
        assert_eq!(&out[40..], &golden[40..], "gyro, accel, botones, recentrado, batería, scroll y extensión");
        assert_eq!(&out[72..80], &[0xE2, 0x78, 0x00, 0x80, 0x00, 0x40, 0x00, 0x00]);
        // sin giro conocido (valor fuera de 0/1) no se remapea: paridad TOTAL
        b.set_rotation(7);
        let p = packet_from_state(&wiiu_state(), &b, Role::Wiimote, Instant::now(), 0xAABBCCDD, 11, 66, false);
        assert_eq!(pmp::build_input(&p), golden);
        assert_eq!(pmp::parse(&golden), Some(pmp::Packet::Input(p)));
    }

    #[test]
    fn gamepad_gira_los_sensores_segun_el_ajuste() {
        let b = wiiu_buttons();
        let st = State {
            t_us: 1,
            quat: [1.0, 0.0, 0.0, 0.0],
            quat_valid: true,
            gyro: [1.0, 0.0, 0.0],
            accel: [0.0, 1.0, 0.0],
        };
        b.set_rotation(1);
        let p = packet_from_state(&st, &b, Role::Wiimote, Instant::now(), 1, 1, 100, false);
        assert_eq!(p.gyro, [0.0, -1.0, 0.0], "derecha: (gy, −gx, gz)");
        assert_eq!(p.accel, [1.0, 0.0, 0.0]);
        assert!((p.quat[3] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        b.set_rotation(0);
        let p = packet_from_state(&st, &b, Role::Wiimote, Instant::now(), 1, 1, 100, false);
        assert_eq!(p.gyro, [0.0, 1.0, 0.0], "izquierda: (−gy, gx, gz)");
        assert_eq!(p.accel, [-1.0, 0.0, 0.0]);
        assert!((p.quat[3] + std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    }

    #[test]
    fn sin_dedo_en_la_tactil_cae_flag_touch() {
        let b = wiiu_buttons();
        b.set_touch(0x8000, 0x4000, false);
        let p = packet_from_state(&wiiu_state(), &b, Role::Wiimote, Instant::now(), 1, 1, 100, false);
        assert_eq!(p.flags, pmp::FLAG_QUAT_VALID | pmp::FLAG_STICK_VALID | pmp::FLAG_EXT);
        assert_eq!(pmp::build_input(&p).len(), pmp::INPUT_EXT_LEN, "sigue siendo un paquete de 80 bytes");
    }

    #[test]
    fn fuera_del_gamepad_72_bytes_como_siempre() {
        let b = wiiu_buttons();
        b.set_gamepad(false);
        let st = wiiu_state();
        let p = packet_from_state(&st, &b, Role::Wiimote, Instant::now(), 1, 1, 100, false);
        assert_eq!(p.flags, pmp::FLAG_QUAT_VALID, "mando: sin stick ni extensión");
        assert_eq!((p.stick_x, p.stick_y), (0, 0), "un mando manda 0,0 aunque el stick tenga valor");
        assert_eq!((p.stick_rx, p.stick_ry, p.touch_x, p.touch_y), (0, 0, 0, 0));
        assert_eq!(p.quat, st.quat, "sin remapeo");
        assert_eq!(pmp::build_input(&p).len(), pmp::INPUT_LEN);
        let n = packet_from_state(&st, &b, Role::Nunchuk, Instant::now(), 1, 1, 100, false);
        assert_eq!(n.flags, pmp::FLAG_QUAT_VALID | pmp::FLAG_STICK_VALID, "Nunchuk: stick en 6-7");
        assert_eq!((n.stick_x, n.stick_y), (100, -50));
        assert_eq!(pmp::build_input(&n).len(), pmp::INPUT_LEN);
        // un Nunchuk nunca es GamePad aunque el atómico quede puesto
        b.set_gamepad(true);
        let n = packet_from_state(&st, &b, Role::Nunchuk, Instant::now(), 1, 1, 100, false);
        assert_eq!(pmp::build_input(&n).len(), pmp::INPUT_LEN);
    }

    #[test]
    fn la_espera_de_reconexion_crece_y_se_queda_en_15_s() {
        let s: Vec<u64> = (1..=7).map(|a| reconnect_delay(a).as_secs()).collect();
        assert_eq!(s, [1, 2, 4, 8, 15, 15, 15]);
        assert_eq!(reconnect_delay(0).as_secs(), 1, "fuera de rango: como el primero");
    }
}
