//! Canal de control TCP: JSON por líneas (PROTOCOL.md §3).

use super::{Session, SharedSession};
use crate::pairing::PairingInfo;
use crate::state::{LinkStatus, Mode, SharedState};
use rand::Rng;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

pub fn run(shared: SharedState, session: SharedSession, pairing: PairingInfo) {
    let listener = match TcpListener::bind(("0.0.0.0", pairing.port)) {
        Ok(l) => l,
        Err(e) => {
            shared.lock().unwrap().last_error =
                Some(format!("No puedo escuchar en TCP {}: {e}", pairing.port));
            return;
        }
    };

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // Un móvil a la vez: si ya hay sesión, rechazamos dentro de handle().
        handle(stream, &shared, &session, &pairing);
    }
}

fn handle(stream: TcpStream, shared: &SharedState, session: &SharedSession, pairing: &PairingInfo) {
    let peer = stream.peer_addr().ok();
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    // Primer mensaje: hello
    line.clear();
    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }
    let hello: Value = match serde_json::from_str(line.trim()) {
        Ok(v) => v,
        Err(_) => return,
    };
    if hello["m"] != "hello" {
        return;
    }
    if hello["pv"].as_i64() != Some(1) {
        let _ = send(&mut writer, &json!({"m":"err","code":"bad_version","msg":"Actualiza PepoMote"}));
        return;
    }
    if hello["token"].as_str() != Some(pairing.token.as_str()) {
        let _ = send(&mut writer, &json!({"m":"err","code":"bad_token","msg":"Vuelve a escanear el QR"}));
        return;
    }
    if session.lock().unwrap().is_some() {
        let _ = send(&mut writer, &json!({"m":"err","code":"busy","msg":"Ya hay un mando conectado"}));
        return;
    }

    let session_id: u32 = rand::thread_rng().gen();
    let device_name = hello["name"].as_str().unwrap_or("Móvil").to_owned();
    let device_model = hello["model"].as_str().unwrap_or("").to_owned();

    *session.lock().unwrap() = Some(Session {
        id: session_id,
        last_seq: None,
        phone_udp: None,
    });
    {
        let mut s = shared.lock().unwrap();
        s.status = LinkStatus::Connected;
        s.device_name = device_name.clone();
        s.device_model = device_model;
        s.last_error = None;
    }
    let mode = shared.lock().unwrap().mode;
    let _ = send(
        &mut writer,
        &json!({"m":"ok","session_id":session_id,"udp_port":pairing.port,"mode":mode_str(mode)}),
    );
    crate::sound::connect_chime();

    // Bucle de control hasta que el móvil se vaya
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // cerrado
            Ok(_) => {}
            Err(_) => {
                // timeout de lectura: ¿sesión aún viva por UDP? El latido TCP del
                // móvil es 1 Hz; 5 s sin nada = muerto.
                break;
            }
        }
        let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        match msg["m"].as_str() {
            Some("ping") => {
                let _ = send(&mut writer, &json!({"m":"pong","t":msg["t"]}));
            }
            Some("mode") => {
                let new_mode = match msg["mode"].as_str() {
                    Some("dolphin") => Mode::Dolphin,
                    _ => Mode::Pointer,
                };
                shared.lock().unwrap().mode = new_mode;
                let _ = send(&mut writer, &json!({"m":"mode","mode":mode_str(new_mode)}));
            }
            Some("config") => {
                // h2: sensibilidad, etc. De momento eco.
                let _ = send(&mut writer, &msg);
            }
            Some("bye") | None => break,
            _ => {}
        }
    }

    let _ = peer; // (log futuro)
    crate::sound::disconnect_chime();
    *session.lock().unwrap() = None;
    let mut s = shared.lock().unwrap();
    s.status = LinkStatus::Waiting;
    s.device_name.clear();
    s.device_model.clear();
    s.pps = 0.0;
    s.sensor_hz = 0.0;
    s.rtt_ms = None;
}

fn mode_str(m: Mode) -> &'static str {
    match m {
        Mode::Pointer => "pointer",
        Mode::Dolphin => "dolphin",
    }
}

fn send(w: &mut TcpStream, v: &Value) -> std::io::Result<()> {
    let mut s = v.to_string();
    s.push('\n');
    w.write_all(s.as_bytes())
}
