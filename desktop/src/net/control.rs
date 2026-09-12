//! Canal de control TCP: JSON por líneas (PROTOCOL.md §3).
//! Un hilo por conexión: hasta MAX_PLAYERS móviles a la vez, cada uno con su
//! slot (Jugador 1 = slot 0). El modo puntero/dolphin/cemu solo lo cambia el
//! slot 0; cuando cambia se difunde a los demás móviles.

use super::{broadcast, free_slot, ghosts_of, send_line, Session, Sessions};
use crate::pairing::PairingInfo;
use crate::screen::ScreenHub;
use crate::state::{effective_pad, player_number, LinkStatus, Mode, PlayerInfo, Role, SharedState};
use rand::Rng;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// PEPOMOTE_DEBUG=1: traza del canal de control (hello/slot/modo/bye).
fn debug() -> bool {
    std::env::var_os("PEPOMOTE_DEBUG").is_some()
}

pub fn run(shared: SharedState, sessions: Sessions, pairing: PairingInfo, hub: Arc<ScreenHub>) {
    let listener = match crate::ports::bind_tcp(&shared, "0.0.0.0", pairing.port, "Móvil") {
        Ok(l) => l,
        Err(e) => {
            shared.lock().unwrap().last_error = Some(e);
            return;
        }
    };

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let shared = shared.clone();
        let sessions = sessions.clone();
        let pairing = pairing.clone();
        let hub = hub.clone();
        let _ = std::thread::Builder::new()
            .name("pmp-control-conn".into())
            .spawn(move || handle(stream, &shared, &sessions, &pairing, &hub));
    }
}

/// Escritor compartido: el hilo de esta conexión y las difusiones desde
/// otros hilos escriben líneas enteras bajo el mismo candado.
type Writer = Arc<Mutex<TcpStream>>;

/// Nombre del tipo de mando Wii U de la sesión del `slot` (`ok.pad` / eco `pad`).
fn pad_str(shared: &SharedState, slot: u8) -> &'static str {
    effective_pad(&shared.lock().unwrap().players, slot)
}

/// El reparto de Cemu cambia con cada entrada, salida o elección de Mando de
/// Wii: a cada móvil cuyo `pad` efectivo haya cambiado se le manda (J2 pasa
/// a GamePad si J1 se va; el Nunchuk entra en uso, o deja de estarlo).
fn push_pad_states(shared: &SharedState, sessions: &Sessions) {
    let players = shared.lock().unwrap().players.clone();
    let mut guard = sessions.lock().unwrap();
    for s in guard.values_mut() {
        let pad = effective_pad(&players, s.slot);
        if s.last_pad == Some(pad) {
            continue;
        }
        s.last_pad = Some(pad);
        if let Some(w) = &s.writer {
            send_line(w, &json!({"m":"pad","pad":pad}));
        }
    }
}

/// Disparo de la autoconfiguración de emuladores (cada una se filtra por modo)
/// y aviso a los móviles de su tipo de mando si cambió.
fn auto_configure(shared: &SharedState, sessions: &Sessions) {
    push_pad_states(shared, sessions);
    crate::dolphin::maybe_auto_configure(shared);
    crate::cemu::maybe_auto_configure(shared);
}

/// El PC cambia el modo por su cuenta (modo automático: se abrió o cerró
/// Dolphin o Cemu). Se difunde a TODOS los móviles con `by:"pc"` (el que
/// tuviera una intención Wii U pendiente la descarta sin aviso), se
/// autoconfigura lo que toque y se avisa con `notice`. Sin móviles, el modo
/// queda puesto y el siguiente `ok` lo lleva.
pub fn set_mode_from_pc(shared: &SharedState, mode: Mode, notice: &str) {
    {
        let mut s = shared.lock().unwrap();
        if s.mode == mode {
            return;
        }
        s.mode = mode;
    }
    crate::log_line!("Modo automático: {notice}");
    broadcast(&json!({"m":"mode","mode":mode.as_str(),"by":"pc"}), None);
    if let Some(sessions) = super::sessions() {
        auto_configure(shared, &sessions);
    }
    super::notify_all(notice);
}

fn handle(stream: TcpStream, shared: &SharedState, sessions: &Sessions, pairing: &PairingInfo, hub: &Arc<ScreenHub>) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let peer_ip: IpAddr = stream
        .peer_addr()
        .map(|a| a.ip())
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));
    let raw_writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    // Primer mensaje: hello (canal de control) o screen (canal de pantalla)
    line.clear();
    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }
    let hello: Value = match serde_json::from_str(line.trim()) {
        Ok(v) => v,
        Err(_) => return,
    };
    if hello["m"] == "screen" {
        super::screen::handle(reader, raw_writer, &hello, shared, sessions, hub);
        return;
    }
    if hello["m"] != "hello" {
        return;
    }
    let writer: Writer = Arc::new(Mutex::new(raw_writer));
    if hello["pv"].as_i64() != Some(1) {
        let _ = send(&writer, &json!({"m":"err","code":"bad_version","msg":"Actualiza PepoMote"}));
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
            // Que el PC también lo diga: quien mira la ventana entiende por
            // qué el móvil no entra (token.txt regenerado, PC reinstalado…)
            let who = hello["name"].as_str().filter(|n| !n.trim().is_empty()).unwrap_or("Un móvil");
            crate::log_line!("Móvil «{who}» ({peer_ip}) trae un QR antiguo: token rechazado");
            shared.lock().unwrap().last_error = Some(format!(
                "{who} ({peer_ip}) trae un QR antiguo: en la app, Conectar y luego «Escanear QR del PC»"
            ));
            ("bad_token", "Vuelve a escanear el QR")
        };
        let _ = send(&writer, &json!({"m":"err","code":code,"msg":msg}));
        return;
    }

    // Sonda de emparejamiento (el móvil Linux al teclear el código): solo
    // quiere el token. Sin plaza, sin campanitas, sin tocar Dolphin.
    if hello["probe"].as_bool() == Some(true) {
        let _ = send(
            &writer,
            &json!({"m":"ok","probe":true,"name":pairing.name,"token":pairing.token,
                    "mode":shared.lock().unwrap().mode.as_str()}),
        );
        return;
    }

    let device_name = hello["name"].as_str().unwrap_or("Móvil").to_owned();
    let device_model = hello["model"].as_str().unwrap_or("").to_owned();
    // Papel: Nunchuk en la otra mano (ausente = mando)
    let role = if hello["role"].as_str() == Some("nunchuk") { Role::Nunchuk } else { Role::Wiimote };
    // Modo Wii U: quiere ser Mando Wii (ausente = GamePad / Pro según jugador)
    let pad_wii = role == Role::Wiimote && hello["pad"].as_str() == Some("wiimote");

    let session_id: u32 = rand::thread_rng().gen();
    let (slot, evicted_slots) = {
        let mut guard = sessions.lock().unwrap();
        // Reconexión del mismo móvil: fuera su sesión fantasma, y así
        // recupera su plaza (Jugador 1 sigue siendo Jugador 1).
        let evicted: Vec<u8> = ghosts_of(&guard, peer_ip, &device_name)
            .into_iter()
            .filter_map(|id| guard.remove(&id).map(|s| s.slot))
            .collect();
        let Some(slot) = free_slot(&guard, role) else {
            drop(guard);
            let _ = send(&writer, &json!({"m":"err","code":"busy","msg":"Ya hay 4 mandos conectados"}));
            return;
        };
        guard.insert(
            session_id,
            Session {
                id: session_id,
                slot,
                last_seq: None,
                phone_udp: None,
                peer: peer_ip,
                device: device_name.clone(),
                role,
                pad_wii,
                writer: Some(writer.clone()),
                last_pad: None,
            },
        );
        (slot, evicted)
    };
    crate::log_line!(
        "Móvil «{device_name}» ({peer_ip}) conectado: slot {slot}, {}{}",
        if role == Role::Nunchuk { "Nunchuk" } else { "mando" },
        if evicted_slots.is_empty() { String::new() } else { format!(" (fantasma desalojada en slot {evicted_slots:?})") }
    );

    let (mode, player) = {
        let mut s = shared.lock().unwrap();
        for e in &evicted_slots {
            s.players[*e as usize] = None;
        }
        s.status = LinkStatus::Connected;
        s.players[slot as usize] = Some(PlayerInfo {
            name: device_name.clone(),
            model: device_model,
            battery_pct: 0,
            rtt_ms: None,
            role,
            pad_wii,
        });
        if s.last_error.as_deref().is_some_and(|e| !e.starts_with("Inyección")) {
            s.last_error = None;
        }
        (s.mode, player_number(&s.players, slot))
    };
    let modes: Vec<&str> = Mode::ALL.iter().map(|m| m.as_str()).collect();
    let pad = pad_str(shared, slot);
    if let Some(sess) = sessions.lock().unwrap().get_mut(&session_id) {
        sess.last_pad = Some(pad);
    }
    let mut ok = json!({"m":"ok","session_id":session_id,"udp_port":pairing.port,
                        "mode":mode.as_str(),"slot":slot,"name":pairing.name,
                        "role":if role == Role::Nunchuk { "nunchuk" } else { "wiimote" },
                        "player":player,
                        "modes":modes,
                        "pad":pad});
    if code_ok {
        ok["token"] = json!(pairing.token);
    }
    let _ = send(&writer, &ok);
    crate::sound::connect_chime(player);
    auto_configure(shared, sessions);

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
                let _ = send(&writer, &json!({"m":"pong","t":msg["t"]}));
            }
            Some("mode") => {
                // Solo el Jugador 1 decide el modo
                if slot == 0 {
                    let new_mode = Mode::parse(msg["mode"].as_str());
                    let changed = {
                        let mut s = shared.lock().unwrap();
                        let changed = s.mode != new_mode;
                        s.mode = new_mode;
                        changed
                    };
                    if debug() {
                        eprintln!("[control] {device_name}: modo → {}", new_mode.as_str());
                    }
                    let reply = json!({"m":"mode","mode":new_mode.as_str()});
                    let _ = send(&writer, &reply);
                    if changed {
                        // Los demás móviles cambian de pantalla con el modo
                        broadcast(&reply, Some(session_id));
                    }
                    auto_configure(shared, sessions);
                } else {
                    let cur = shared.lock().unwrap().mode;
                    if debug() {
                        eprintln!("[control] {device_name} (slot {slot}) pidió modo: solo decide el Jugador 1, sigue {}", cur.as_str());
                    }
                    let _ = send(&writer, &json!({"m":"mode","mode":cur.as_str()}));
                }
            }
            Some("pad") => {
                // Modo Wii U: cada móvil elige ser Mando Wii o GamePad/Pro
                if role == Role::Wiimote {
                    let wants_wii = msg["pad"].as_str() == Some("wiimote");
                    let changed = {
                        let mut s = shared.lock().unwrap();
                        match s.players[slot as usize].as_mut() {
                            Some(p) if p.pad_wii != wants_wii => {
                                p.pad_wii = wants_wii;
                                true
                            }
                            _ => false,
                        }
                    };
                    let effective = pad_str(shared, slot);
                    if let Some(sess) = sessions.lock().unwrap().get_mut(&session_id) {
                        sess.pad_wii = wants_wii;
                        sess.last_pad = Some(effective);
                    }
                    if debug() {
                        eprintln!("[control] {device_name} (slot {slot}) pad → {effective}");
                    }
                    let _ = send(&writer, &json!({"m":"pad","pad":effective}));
                    if changed {
                        // el Nunchuk del jugador (y nadie más) cambia de estado
                        push_pad_states(shared, sessions);
                        crate::cemu::maybe_auto_configure(shared);
                    }
                } else {
                    let _ = send(&writer, &json!({"m":"pad","pad":pad_str(shared, slot)}));
                }
            }
            Some("text") => {
                // Teclado del móvil → teclado en pantalla de Cemu (no acepta
                // toques, solo teclas): a la ventana de Cemu en modo Wii U;
                // si no se la encuentra, o en otros modos, al SO (ventana con
                // el foco) desde el hilo de telemetría
                if let Some(t) = msg["text"].as_str().filter(|t| !t.is_empty()) {
                    let wiiu = shared.lock().unwrap().mode == Mode::Cemu;
                    let to_os = if wiiu {
                        // sin camino directo a la ventana (Linux): al SO, pero
                        // solo con Cemu abierto (que tendrá el foco)
                        !crate::screen::type_text(t) && cfg!(target_os = "linux") && crate::cemu::running_exe().0
                    } else {
                        true
                    };
                    if to_os {
                        shared.lock().unwrap().text_queue.push(t.to_owned());
                    }
                }
            }
            Some("config") => {
                let _ = send(&writer, &msg);
            }
            Some("bye") | None => break,
            _ => {}
        }
    }

    // Limpieza de ESTA sesión. Si otra conexión del mismo móvil ya la
    // desalojó, la plaza es suya: no tocar nada.
    let still_mine = sessions.lock().unwrap().remove(&session_id).is_some();
    crate::log_line!(
        "Móvil «{device_name}» (slot {slot}) se va{}",
        if still_mine { "" } else { " — ya desalojada por su reconexión" }
    );
    if !still_mine {
        return;
    }
    let (empty, player) = {
        let mut s = shared.lock().unwrap();
        // el número de jugador se calcula ANTES de vaciar la plaza (su campanita)
        let player = player_number(&s.players, slot);
        s.players[slot as usize] = None;
        let empty = s.player_count() == 0;
        if empty {
            s.status = LinkStatus::Waiting;
            s.pps = 0.0;
            s.sensor_hz = 0.0;
            s.rtt_hist.clear();
        }
        (empty, player)
    };
    crate::sound::disconnect_chime(player);
    if !empty {
        auto_configure(shared, sessions);
    }
}

fn send(w: &Writer, v: &Value) -> std::io::Result<()> {
    let mut s = v.to_string();
    s.push('\n');
    let mut w = w.lock().unwrap_or_else(|e| e.into_inner());
    w.write_all(s.as_bytes())
}
