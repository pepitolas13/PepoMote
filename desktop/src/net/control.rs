//! Canal de control TCP: JSON por líneas (PROTOCOL.md §3).
//! Un hilo por conexión: hasta MAX_PLAYERS móviles a la vez, cada uno con su
//! slot (Jugador 1 = slot 0). El modo puntero/dolphin solo lo cambia el slot 0.

use super::{lowest_free_slot, Session, Sessions};
use crate::pairing::PairingInfo;
use crate::state::{LinkStatus, Mode, PlayerInfo, SharedState};
use rand::Rng;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

pub fn run(shared: SharedState, sessions: Sessions, pairing: PairingInfo) {
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
        let shared = shared.clone();
        let sessions = sessions.clone();
        let pairing = pairing.clone();
        let _ = std::thread::Builder::new()
            .name("pmp-control-conn".into())
            .spawn(move || handle(stream, &shared, &sessions, &pairing));
    }
}

fn handle(stream: TcpStream, shared: &SharedState, sessions: &Sessions, pairing: &PairingInfo) {
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
    // Token (QR) o, para móviles sin cámara, el código de 4 dígitos que se
    // muestra bajo el QR: en ese caso el `ok` lleva el token definitivo.
    let token_ok = hello["token"].as_str() == Some(pairing.token.as_str());
    let code_ok = !token_ok
        && hello["code"]
            .as_str()
            .is_some_and(|c| shared.lock().unwrap().pair_code.try_accept(c.trim()));
    if !token_ok && !code_ok {
        let (code, msg) = if hello["code"].is_string() {
            ("bad_code", "Código incorrecto o caducado: mira el nuevo bajo el QR del PC")
        } else {
            ("bad_token", "Vuelve a escanear el QR")
        };
        let _ = send(&mut writer, &json!({"m":"err","code":code,"msg":msg}));
        return;
    }

    let session_id: u32 = rand::thread_rng().gen();
    let slot = {
        let mut guard = sessions.lock().unwrap();
        let Some(slot) = lowest_free_slot(&guard) else {
            drop(guard);
            let _ = send(&mut writer, &json!({"m":"err","code":"busy","msg":"Ya hay 4 mandos conectados"}));
            return;
        };
        guard.insert(
            session_id,
            Session {
                id: session_id,
                slot,
                last_seq: None,
                phone_udp: None,
            },
        );
        slot
    };

    let device_name = hello["name"].as_str().unwrap_or("Móvil").to_owned();
    let device_model = hello["model"].as_str().unwrap_or("").to_owned();
    let mode = {
        let mut s = shared.lock().unwrap();
        s.status = LinkStatus::Connected;
        s.players[slot as usize] = Some(PlayerInfo {
            name: device_name,
            model: device_model,
            battery_pct: 0,
            rtt_ms: None,
        });
        s.last_error = None;
        s.mode
    };
    let mut ok = json!({"m":"ok","session_id":session_id,"udp_port":pairing.port,
                        "mode":mode_str(mode),"slot":slot,"name":pairing.name});
    if code_ok {
        ok["token"] = json!(pairing.token);
    }
    let _ = send(&mut writer, &ok);
    crate::sound::connect_chime();
    crate::dolphin::maybe_auto_configure(shared);

    // Bucle de control hasta que este móvil se vaya
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break, // 5 s sin nada (el latido es 1 Hz): muerto
        }
        let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        match msg["m"].as_str() {
            Some("ping") => {
                let _ = send(&mut writer, &json!({"m":"pong","t":msg["t"]}));
            }
            Some("mode") => {
                // Solo el Jugador 1 decide el modo
                if slot == 0 {
                    let new_mode = match msg["mode"].as_str() {
                        Some("dolphin") => Mode::Dolphin,
                        _ => Mode::Pointer,
                    };
                    shared.lock().unwrap().mode = new_mode;
                    let _ = send(&mut writer, &json!({"m":"mode","mode":mode_str(new_mode)}));
                    crate::dolphin::maybe_auto_configure(shared);
                } else {
                    let cur = shared.lock().unwrap().mode;
                    let _ = send(&mut writer, &json!({"m":"mode","mode":mode_str(cur)}));
                }
            }
            Some("config") => {
                let _ = send(&mut writer, &msg);
            }
            Some("bye") | None => break,
            _ => {}
        }
    }

    // Limpieza de ESTA sesión
    sessions.lock().unwrap().remove(&session_id);
    let empty = {
        let mut s = shared.lock().unwrap();
        s.players[slot as usize] = None;
        let empty = s.player_count() == 0;
        if empty {
            s.status = LinkStatus::Waiting;
            s.pps = 0.0;
            s.sensor_hz = 0.0;
        }
        empty
    };
    crate::sound::disconnect_chime();
    if !empty {
        crate::dolphin::maybe_auto_configure(shared);
    }
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
