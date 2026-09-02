//! Enlace con el receptor: canal de control TCP (hello/ok/err/ping/mode/bye),
//! socket UDP caliente (INPUT a la cadencia del sensor, PING/PONG para el
//! RTT) y el hilo de paquetes que fusiona las muestras del sensor. Mismo
//! comportamiento que LinkForegroundService + MotionEngine de Android.

use crate::buttons::Buttons;
use crate::discovery;
use crate::bias::GyroBias;
use crate::fusion::Madgwick;
use crate::pacing::{Pacer, State};
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

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Disconnected,
    Connecting,
    Connected {
        pc_name: String,
        mode: String,
        slot: u8,
        rtt_ms: Option<f32>,
    },
    Failed {
        code: String,
        msg: String,
    },
}

pub struct Link {
    status: Arc<Mutex<Status>>,
    writer: Arc<Mutex<Option<TcpStream>>>,
    stop: Arc<AtomicBool>,
    sensor_hz: Arc<AtomicU32>,
}

impl Link {
    /// Conecta en segundo plano. `pending_mode` se manda nada más recibir `ok`.
    pub fn connect(
        pairing: Pairing,
        buttons: Arc<Buttons>,
        source: Box<dyn Source>,
        pending_mode: Option<String>,
    ) -> Link {
        let status = Arc::new(Mutex::new(Status::Connecting));
        let writer: Arc<Mutex<Option<TcpStream>>> = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let sensor_hz = Arc::new(AtomicU32::new(0));
        {
            let ctx = Ctx {
                status: status.clone(),
                writer: writer.clone(),
                stop: stop.clone(),
                sensor_hz: sensor_hz.clone(),
                buttons,
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
        }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn sensor_hz(&self) -> f32 {
        f32::from_bits(self.sensor_hz.load(Ordering::Relaxed))
    }

    pub fn send_mode(&self, mode: &str) {
        send_json(&self.writer, &json!({"m":"mode","mode":mode}));
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
    buttons: Arc<Buttons>,
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

fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
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

    send_json(
        &ctx.writer,
        &json!({"m":"hello","pv":1,"token":pairing.token,"name":device_name(),"model":"Linux móvil"}),
    );

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
                    rtt_ms: None,
                };
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
            Some("mode") => {
                if let Status::Connected { mode, .. } = &mut *ctx.status.lock().unwrap() {
                    *mode = msg["mode"].as_str().unwrap_or("pointer").to_owned();
                }
            }
            _ => {}
        }
    }

    ctx.stop.store(true, Ordering::Relaxed);
    ctx.buttons.release_all();
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
        std::thread::Builder::new()
            .name("pepomote-packets".into())
            .spawn(move || packet_loop(udp, session_id, rx, buttons, stop, sensor_hz))
            .expect("hilo paquetes");
    }
}

fn packet_loop(
    udp: Arc<UdpSocket>,
    session_id: u32,
    rx: mpsc::Receiver<Sample>,
    buttons: Arc<Buttons>,
    stop: Arc<AtomicBool>,
    sensor_hz: Arc<AtomicU32>,
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
        let packet = pmp::InputPacket {
            flags: pmp::FLAG_QUAT_VALID,
            session_id,
            seq,
            t_sensor_us: st.t_us,
            quat: st.quat,
            gyro: st.gyro,
            accel: st.accel,
            buttons: buttons.wire_at(now),
            recenter_count: buttons.recenter_count(),
            battery_pct: battery.pct(),
            touch_scroll_dy: buttons.drain_scroll(),
        };
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
