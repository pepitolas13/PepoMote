//! Enlace con el receptor: canal de control TCP (hello/ok/err/ping/mode/pad/
//! notice/bye), socket UDP caliente (INPUT a la cadencia del sensor,
//! PING/PONG para el RTT) y el hilo de paquetes que fusiona las muestras del
//! sensor. Mismo comportamiento que LinkForegroundService + MotionEngine de
//! Android. Un mismo enlace sirve de Wiimote, de Nunchuk (PROTOCOL.md §3) o
//! de GamePad/Pro de Wii U (modo `cemu`): cambian el hello y, en cada INPUT,
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

const MAX_ATTEMPTS: u32 = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// Tope del protocolo: 250 Hz.
const MIN_PACKET_GAP: Duration = Duration::from_micros(3_900);
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
        /// Tipo de mando efectivo en modo Wii U: "gamepad", "pro" o "wiimote".
        pad: String,
        /// Aviso transitorio (del receptor o local) y cuándo llegó.
        notice: Option<(String, Instant)>,
        /// Cuántos `mode` (eco o difusión) y cuántos ecos `pad` han llegado
        /// en esta sesión: así la UI sabe si su petición ya tuvo respuesta.
        mode_seq: u32,
        pad_seq: u32,
    },
    Failed {
        code: String,
        msg: String,
    },
}

impl Status {
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
                buttons,
                role,
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

    /// Elegir Mando Wii (`"wiimote"`) o volver a GamePad/Pro (`"gamepad"`)
    /// en modo Wii U. El receptor contesta con el eco del tipo efectivo; uno
    /// antiguo no contesta y no pasa nada.
    pub fn send_pad(&self, pad: &str) {
        send_json(&self.writer, &json!({"m":"pad","pad":pad}));
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
    buttons: Arc<Buttons>,
    role: Role,
}

/// `hello` de sesión (PROTOCOL.md §3): `role` solo si somos Nunchuk; un
/// mando no lo manda (compatibilidad con receptores anteriores). El tipo de
/// mando en Wii U no va aquí: se elige ya en la sesión con `pad`.
fn hello(token: &str, role: Role) -> Value {
    let mut v = json!({"m":"hello","pv":1,"token":token,"name":device_name(),"model":"Linux móvil"});
    if role == Role::Nunchuk {
        v["role"] = json!("nunchuk");
    }
    v
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

fn valid_pad(s: Option<&str>) -> Option<&str> {
    s.filter(|p| matches!(*p, "gamepad" | "pro" | "wiimote"))
}

/// Tipo de mando efectivo en modo Wii U que anuncia el `ok`; si falta
/// (receptor antiguo, que nunca confirmará `cemu`), el que tocaría: GamePad
/// para el jugador 1 y Pro para los demás.
fn pad_of(ok: &Value, slot: u8) -> String {
    valid_pad(ok["pad"].as_str())
        .unwrap_or(if slot == 0 { "gamepad" } else { "pro" })
        .to_owned()
}

/// Mensajes que solo actualizan el estado de una sesión ya conectada: `mode`
/// (eco o difusión: se aplica igual), `pad` (eco del tipo efectivo) y
/// `notice`. Cualquier otro se ignora (devuelve `false`).
fn apply_update(st: &mut Status, msg: &Value, now: Instant) -> bool {
    let Status::Connected { mode, pad, notice, mode_seq, pad_seq, .. } = st else {
        return false;
    };
    match msg["m"].as_str() {
        Some("mode") => {
            *mode = msg["mode"].as_str().unwrap_or("pointer").to_owned();
            *mode_seq = mode_seq.wrapping_add(1);
            true
        }
        Some("pad") => {
            if let Some(p) = valid_pad(msg["pad"].as_str()) {
                *pad = p.to_owned();
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
        .map_err(|e| format!("Dirección no válida {host}:{port}: {e}"))?
        .next()
        .ok_or_else(|| format!("Dirección no válida {host}:{port}"))
}

/// Emparejamiento por código (bloqueante, ≤ 5 s): hello con `code`, el `ok`
/// trae el token definitivo. `fallback_name` si el receptor no manda nombre.
pub fn pair(host: &str, port: u16, code: &str, fallback_name: &str) -> Result<Pairing, String> {
    let addr = resolve(host, port)?;
    let mut s = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("No llego a {host}:{port}: {e}"))?;
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
        .map_err(|e| format!("El PC no responde: {e}"))?;
    let v: Value = serde_json::from_str(resp.trim()).map_err(|_| "Respuesta ilegible del PC".to_owned())?;
    let _ = s.write_all(b"{\"m\":\"bye\"}\n");
    let _ = s.shutdown(Shutdown::Both);
    match v["m"].as_str() {
        Some("ok") => {
            let token = v["token"]
                .as_str()
                .ok_or("El PC no devolvió el token (¿receptor antiguo? actualízalo)")?
                .to_owned();
            Ok(Pairing {
                host: host.to_owned(),
                port,
                token,
                pc_name: v["name"].as_str().unwrap_or(fallback_name).to_owned(),
            })
        }
        Some("err") => Err(v["msg"].as_str().unwrap_or("Rechazado").to_owned()),
        _ => Err("Respuesta inesperada del PC".into()),
    }
}

fn control_thread(mut pairing: Pairing, source: Box<dyn Source>, pending_mode: Option<String>, ctx: Ctx) {
    let mut source = Some(source);
    let mut pending_mode = pending_mode;

    // Conectar con reintentos; entre ellos, buscar el PC por nombre por si
    // cambió de IP (DHCP, otra Wi-Fi) y actualizar el emparejamiento solo
    let mut attempt = 0;
    let stream = loop {
        attempt += 1;
        let result = resolve(&pairing.host, pairing.port)
            .and_then(|a| TcpStream::connect_timeout(&a, CONNECT_TIMEOUT).map_err(|e| e.to_string()));
        match result {
            Ok(s) => break s,
            Err(e) => {
                if attempt >= MAX_ATTEMPTS || ctx.stop.load(Ordering::Relaxed) {
                    *ctx.status.lock().unwrap() = Status::Failed {
                        code: "io".into(),
                        msg: format!("No llego a {} ({}:{}): {e}", pairing.pc_name, pairing.host, pairing.port),
                    };
                    return;
                }
                if let Some(r) = discovery::scan(Duration::from_millis(1200))
                    .into_iter()
                    .find(|r| r.name == pairing.pc_name && (r.host != pairing.host || r.port != pairing.port))
                {
                    pairing.host = r.host;
                    pairing.port = r.port;
                    store::save(&pairing);
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    };
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(7)));
    let Ok(read_half) = stream.try_clone() else {
        *ctx.status.lock().unwrap() = Status::Failed {
            code: "io".into(),
            msg: "socket".into(),
        };
        return;
    };
    let mut reader = BufReader::new(read_half);
    *ctx.writer.lock().unwrap() = Some(stream);

    send_json(&ctx.writer, &hello(&pairing.token, ctx.role));

    // Latido TCP 1 Hz
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
                let session_id = msg["session_id"].as_u64().unwrap_or(0) as u32;
                let udp_port = msg["udp_port"].as_u64().map(|p| p as u16).unwrap_or(pairing.port);
                let mode = msg["mode"].as_str().unwrap_or("pointer").to_owned();
                let slot = msg["slot"].as_u64().unwrap_or(0) as u8;
                let player = player_of(&msg, slot);
                let role = Role::parse(msg["role"].as_str()).unwrap_or(ctx.role);
                if let Some(name) = msg["name"].as_str() {
                    if name != pairing.pc_name {
                        pairing.pc_name = name.to_owned();
                        store::save(&pairing);
                    }
                }
                *ctx.status.lock().unwrap() = Status::Connected {
                    pc_name: pairing.pc_name.clone(),
                    mode,
                    slot,
                    player,
                    role,
                    rtt_ms: None,
                    supports_cemu: supports_cemu(&msg),
                    pad: pad_of(&msg, slot),
                    notice: None,
                    mode_seq: 0,
                    pad_seq: 0,
                };
                // la pantalla del GamePad va por otra conexión TCP al mismo
                // puerto PMP que este control (el `udp_port` del ok es ese
                // mismo puerto), con la sesión del ok
                *ctx.endpoint.lock().unwrap() = Some(Endpoint {
                    host: pairing.host.clone(),
                    port: pairing.port,
                    session_id,
                });
                if let Some(src) = source.take() {
                    start_hot_path(&pairing.host, udp_port, session_id, src, &ctx);
                }
                if let Some(m) = pending_mode.take() {
                    send_json(&ctx.writer, &json!({"m":"mode","mode":m}));
                }
            }
            Some("err") => {
                *ctx.status.lock().unwrap() = Status::Failed {
                    code: msg["code"].as_str().unwrap_or("err").to_owned(),
                    msg: msg["msg"].as_str().unwrap_or("Rechazado por el PC").to_owned(),
                };
                break;
            }
            Some("ping") => {
                send_json(&ctx.writer, &json!({"m":"pong","t":msg["t"]}));
            }
            // mode (eco o difundido), pad, notice… y lo desconocido se ignora
            _ => {
                apply_update(&mut ctx.status.lock().unwrap(), &msg, Instant::now());
            }
        }
    }

    ctx.stop.store(true, Ordering::Relaxed);
    ctx.buttons.release_all();
    ctx.endpoint.lock().unwrap().take();
    let mut st = ctx.status.lock().unwrap();
    if !matches!(*st, Status::Failed { .. }) {
        *st = Status::Disconnected;
    }
}

/// UDP: hilo del sensor → hilo de paquetes (fusión + INPUT) + oyente PING/PONG.
fn start_hot_path(host: &str, port: u16, session_id: u32, source: Box<dyn Source>, ctx: &Ctx) {
    let Ok(udp) = UdpSocket::bind("0.0.0.0:0") else {
        return;
    };
    if udp.connect((host, port)).is_err() {
        return;
    }
    let _ = udp.set_read_timeout(Some(Duration::from_millis(500)));
    let udp = Arc::new(udp);

    // Oyente: eco de PING y RTT de nuestros PING
    {
        let udp = udp.clone();
        let stop = ctx.stop.clone();
        let status = ctx.status.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 128];
            while !stop.load(Ordering::Relaxed) {
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

    // Sensor → canal
    let (tx, rx) = mpsc::channel::<Sample>();
    {
        let stop = ctx.stop.clone();
        std::thread::Builder::new()
            .name("pepomote-sensor".into())
            .spawn(move || source.run(tx, stop))
            .expect("hilo sensor");
    }

    // Paquetes
    {
        let stop = ctx.stop.clone();
        let buttons = ctx.buttons.clone();
        let sensor_hz = ctx.sensor_hz.clone();
        let role = ctx.role;
        std::thread::Builder::new()
            .name("pepomote-packets".into())
            .spawn(move || packet_loop(udp, session_id, rx, buttons, stop, sensor_hz, role))
            .expect("hilo paquetes");
    }
}

/// INPUT a partir del estado fusionado y de lo que tiene el dedo. Como
/// GamePad/Pro de Wii U (`buttons.is_gamepad()`, que la UI solo activa con
/// `mode == "cemu"` confirmado): 80 bytes con FLAG_EXT (+FLAG_TOUCH con dedo
/// en la pantalla táctil), stick izquierdo en 6-7, derecho y táctil en la
/// extensión, y los sensores remapeados al marco apaisado según el giro. Si
/// no, 72 bytes como siempre: el Nunchuk lleva su stick con FLAG_STICK_VALID
/// y el mando manda 0,0.
#[allow(clippy::too_many_arguments)]
fn packet_from_state(
    st: &State,
    buttons: &Buttons,
    role: Role,
    now: Instant,
    session_id: u32,
    seq: u32,
    battery_pct: u8,
) -> pmp::InputPacket {
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
        let (touch_x, touch_y, touch_down) = buttons.touch();
        let mut flags = pmp::FLAG_QUAT_VALID | pmp::FLAG_STICK_VALID | pmp::FLAG_EXT;
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
        Role::Nunchuk => (pmp::FLAG_QUAT_VALID | pmp::FLAG_STICK_VALID, buttons.stick()),
        Role::Wiimote => (pmp::FLAG_QUAT_VALID, (0, 0)),
    };
    pmp::InputPacket {
        flags,
        stick_x,
        stick_y,
        ..base
    }
}

#[allow(clippy::too_many_arguments)]
fn packet_loop(
    udp: Arc<UdpSocket>,
    session_id: u32,
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
    let mut last_sent = Instant::now();
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
        match rx.recv_timeout(wait) {
            Ok(sample) => {
                let now = Instant::now();
                last_sample_at = Some(now);
                let dt = last_t_us
                    .map(|t| sample.t_us.saturating_sub(t) as f32 / 1e6)
                    .filter(|dt| (1e-5..0.25).contains(dt))
                    .unwrap_or(0.005);
                last_t_us = Some(sample.t_us);
                let gyro = bias.correct(sample.t_us, sample.gyro, sample.accel);
                fusion.update(gyro, sample.accel, dt);
                pacer.push(
                    State {
                        t_us: sample.t_us,
                        quat: fusion.quat(),
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
                continue;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // 2) Envío a ritmo fijo (tope del protocolo), interpolado con el
        //    reloj de pared: las ráfagas del sensor no llegan al receptor
        let now = Instant::now();
        if now < next_send {
            continue;
        }
        next_send = now + MIN_PACKET_GAP;
        let idle = last_sample_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(1));
        if idle {
            // sin muestras: keepalive 1 Hz con lo último (PROTOCOL.md §4.1)
            sensor_hz.store(0f32.to_bits(), Ordering::Relaxed);
            if last_sent.elapsed() < Duration::from_secs(1) {
                continue;
            }
        }
        let st = pacer.output(wall_us(now)).unwrap_or(State {
            t_us: sensor::now_us(),
            quat: fusion.quat(),
            gyro: [0.0; 3],
            accel: [0.0; 3],
        });
        last_sent = now;
        seq = seq.wrapping_add(1);
        let packet = packet_from_state(&st, &buttons, role, now, session_id, seq, battery.pct());
        let _ = udp.send(&pmp::build_input(&packet));

        if last_ping.elapsed() >= Duration::from_secs(1) {
            last_ping = Instant::now();
            let _ = udp.send(&pmp::build_ping(session_id, sensor::now_us()));
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

    #[test]
    fn hello_declara_el_papel_solo_en_el_nunchuk() {
        let w = hello("tok", Role::Wiimote);
        assert_eq!(w["m"], "hello");
        assert_eq!(w["pv"], 1);
        assert_eq!(w["token"], "tok");
        assert!(w.get("role").is_none(), "un mando no manda role (receptores anteriores)");
        assert!(w.get("pad").is_none(), "el tipo de mando de Wii U se elige en la sesión, no en el hello");
        let n = hello("tok", Role::Nunchuk);
        assert_eq!(n["role"], "nunchuk");
        assert_eq!(n["token"], "tok");
        assert!(n.get("pad").is_none());
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
            slot: 0,
            player: 1,
            role: Role::Wiimote,
            rtt_ms: None,
            supports_cemu: true,
            pad: "gamepad".into(),
            notice: None,
            mode_seq: 0,
            pad_seq: 0,
        }
    }

    #[test]
    fn mode_pad_y_notice_actualizan_la_sesion_y_lo_demas_se_ignora() {
        let mut st = connected();
        let now = Instant::now();
        assert!(apply_update(&mut st, &json!({"m":"mode","mode":"cemu"}), now), "difundido sin pedirlo: se aplica");
        assert!(matches!(&st, Status::Connected { mode, mode_seq: 1, pad_seq: 0, .. } if mode == "cemu"));
        assert!(apply_update(&mut st, &json!({"m":"mode","mode":"cemu"}), now));
        assert!(matches!(&st, Status::Connected { mode_seq: 2, .. }), "cada eco cuenta, aunque repita el modo");
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
        let p = packet_from_state(&wiiu_state(), &b, Role::Wiimote, Instant::now(), 0xAABBCCDD, 11, 66);
        assert_eq!(p.flags, 0x0F, "QUAT|STICK|EXT|TOUCH");
        assert_eq!(p.buttons, 0x1FF8_0001);
        assert_eq!((p.stick_x, p.stick_y), (100, -50));
        assert_eq!((p.stick_rx, p.stick_ry), (-30, 120));
        assert_eq!((p.touch_x, p.touch_y), (0x8000, 0x4000));
        assert_eq!(p.accel, [0.0, 0.0, 9.5], "plano: la gravedad no cambia con el giro");
        let q = p.quat;
        assert!((q[0] - 0.7071068).abs() < 1e-6 && q[1] == 0.0 && q[2] == 0.0 && (q[3] + 0.7071068).abs() < 1e-6, "{q:?}");
        let out = pmp::build_input(&p);
        assert_eq!(out.len(), pmp::INPUT_EXT_LEN);
        // todo menos el quaternion (remapeado) coincide byte a byte con el vector
        assert_eq!(&out[..24], &golden[..24], "cabecera, flags, stick izquierdo, sesión, seq, t");
        assert_eq!(&out[40..], &golden[40..], "gyro, accel, botones, recentrado, batería, scroll y extensión");
        assert_eq!(&out[72..80], &[0xE2, 0x78, 0x00, 0x80, 0x00, 0x40, 0x00, 0x00]);
        // sin giro conocido (valor fuera de 0/1) no se remapea: paridad TOTAL
        b.set_rotation(7);
        let p = packet_from_state(&wiiu_state(), &b, Role::Wiimote, Instant::now(), 0xAABBCCDD, 11, 66);
        assert_eq!(pmp::build_input(&p), golden);
        assert_eq!(pmp::parse(&golden), Some(pmp::Packet::Input(p)));
    }

    #[test]
    fn gamepad_gira_los_sensores_segun_el_ajuste() {
        let b = wiiu_buttons();
        let st = State {
            t_us: 1,
            quat: [1.0, 0.0, 0.0, 0.0],
            gyro: [1.0, 0.0, 0.0],
            accel: [0.0, 1.0, 0.0],
        };
        b.set_rotation(1);
        let p = packet_from_state(&st, &b, Role::Wiimote, Instant::now(), 1, 1, 100);
        assert_eq!(p.gyro, [0.0, -1.0, 0.0], "derecha: (gy, −gx, gz)");
        assert_eq!(p.accel, [1.0, 0.0, 0.0]);
        assert!((p.quat[3] - 0.7071068).abs() < 1e-6);
        b.set_rotation(0);
        let p = packet_from_state(&st, &b, Role::Wiimote, Instant::now(), 1, 1, 100);
        assert_eq!(p.gyro, [0.0, 1.0, 0.0], "izquierda: (−gy, gx, gz)");
        assert_eq!(p.accel, [-1.0, 0.0, 0.0]);
        assert!((p.quat[3] + 0.7071068).abs() < 1e-6);
    }

    #[test]
    fn sin_dedo_en_la_tactil_cae_flag_touch() {
        let b = wiiu_buttons();
        b.set_touch(0x8000, 0x4000, false);
        let p = packet_from_state(&wiiu_state(), &b, Role::Wiimote, Instant::now(), 1, 1, 100);
        assert_eq!(p.flags, pmp::FLAG_QUAT_VALID | pmp::FLAG_STICK_VALID | pmp::FLAG_EXT);
        assert_eq!(pmp::build_input(&p).len(), pmp::INPUT_EXT_LEN, "sigue siendo un paquete de 80 bytes");
    }

    #[test]
    fn fuera_del_gamepad_72_bytes_como_siempre() {
        let b = wiiu_buttons();
        b.set_gamepad(false);
        let st = wiiu_state();
        let p = packet_from_state(&st, &b, Role::Wiimote, Instant::now(), 1, 1, 100);
        assert_eq!(p.flags, pmp::FLAG_QUAT_VALID, "mando: sin stick ni extensión");
        assert_eq!((p.stick_x, p.stick_y), (0, 0), "un mando manda 0,0 aunque el stick tenga valor");
        assert_eq!((p.stick_rx, p.stick_ry, p.touch_x, p.touch_y), (0, 0, 0, 0));
        assert_eq!(p.quat, st.quat, "sin remapeo");
        assert_eq!(pmp::build_input(&p).len(), pmp::INPUT_LEN);
        let n = packet_from_state(&st, &b, Role::Nunchuk, Instant::now(), 1, 1, 100);
        assert_eq!(n.flags, pmp::FLAG_QUAT_VALID | pmp::FLAG_STICK_VALID, "Nunchuk: stick en 6-7");
        assert_eq!((n.stick_x, n.stick_y), (100, -50));
        assert_eq!(pmp::build_input(&n).len(), pmp::INPUT_LEN);
        // un Nunchuk nunca es GamePad aunque el atómico quede puesto
        b.set_gamepad(true);
        let n = packet_from_state(&st, &b, Role::Nunchuk, Instant::now(), 1, 1, 100);
        assert_eq!(pmp::build_input(&n).len(), pmp::INPUT_LEN);
    }
}
