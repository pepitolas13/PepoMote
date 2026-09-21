//! Canal de control TCP: JSON por líneas (PROTOCOL.md §3).
//! Un hilo por conexión: hasta MAX_PLAYERS móviles a la vez, cada uno con su
//! slot (Jugador 1 = slot 0). El modo puntero/dolphin/cemu solo lo cambia el
//! slot 0; cuando cambia se difunde a los demás móviles.

use crate::state::LockTolerant;
use super::{broadcast, free_slot, ghosts_of, send_line, Session, Sessions};
use crate::pairing::PairingInfo;
use crate::screen::ScreenHub;
use crate::retroarch::RetroPadKind;
use crate::input::text_plan;
use crate::state::{pad_state, PadState, player_number, LinkStatus, Mode, PendingText, PlayerInfo, Role, SharedState, SwitchPad, MAX_TEXT_QUEUE};
use rand::Rng;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::tr;

/// PEPOMOTE_DEBUG=1: traza del canal de control (hello/slot/modo/bye).
fn debug() -> bool {
    std::env::var_os("PEPOMOTE_DEBUG").is_some()
}

pub fn run(shared: SharedState, sessions: Sessions, pairing: PairingInfo, hub: Arc<ScreenHub>) {
    let listener = match crate::ports::bind_tcp(&shared, "0.0.0.0", pairing.port, tr!("port.what_phone")) {
        Ok(l) => l,
        Err(e) => {
            shared.lock_tolerant().last_error = Some(e);
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

/// Nombre del tipo de mando (Wii U o Switch, según el modo) de la sesión del
/// `slot` (`ok.pad` / eco `pad`).
fn current_pad(shared: &SharedState, slot:u8) -> PadState {
    let s=shared.lock_tolerant(); pad_state(s.mode,&s.players,slot)
}

/// `layout`: plantilla de consola que dice el móvil (`pad.layout`), devuelta
/// tal cual (o null) para que se pueda comprobar de punta a punta.
fn pad_message(p:PadState, layout: Option<&'static str>) -> Value {
    json!({"m":"pad","pad":p.pad,"half":null,
        "side":null,"player":p.player,"layout":layout})
}

fn layout_of(players: &[Option<crate::state::PlayerInfo>], slot: u8) -> Option<&'static str> {
    players.get(slot as usize).and_then(|p| p.as_ref()).and_then(|p| p.retro_layout)
}

fn push_pad_states(shared: &SharedState, sessions: &Sessions) {
    let (mode,players)={let s=shared.lock_tolerant();(s.mode,s.players.clone())};
    let mut outgoing=Vec::new();
    {
        let mut guard=sessions.lock_tolerant();
        for s in guard.values_mut() {
            let pad=pad_state(mode,&players,s.slot);
            if s.last_pad==Some(pad) {continue;}
            s.last_pad=Some(pad);
            if let Some(w)=&s.writer {outgoing.push((w.clone(),pad_message(pad, layout_of(&players, s.slot))));}
        }
    }
    for (writer,message) in outgoing {send_line(&writer,&message);}
}

/// Disparo de la autoconfiguración de emuladores (cada una se filtra por modo)
/// y aviso a los móviles de su tipo de mando si cambió.
fn auto_configure(shared: &SharedState, sessions: &Sessions) {
    push_pad_states(shared, sessions);
    // Mandos virtuales de la vibración ANTES de escribir los perfiles: así
    // Dolphin y Cemu se escriben con el índice XInput real
    crate::rumble::sync(shared);
    crate::dolphin::maybe_auto_configure(shared);
    crate::cemu::maybe_auto_configure(shared);
    crate::eden::maybe_auto_configure(shared);
    crate::retroarch::maybe_auto_configure(shared);
    // El enlace con RetroArch sabe qué móviles son mando ahora (para soltar
    // todo cuando uno se va y callar en los demás modos)
    sync_retroarch_presence(shared);
}

/// Qué slots son un mando de RetroArch ahora mismo (modo RetroArch y papel
/// de mando): el enlace suelta lo que tuviera pulsado quien deja de serlo.
fn sync_retroarch_presence(shared: &SharedState) {
    let Some(link) = crate::retroarch::link() else { return };
    let (mode, players) = {
        let s = shared.lock_tolerant();
        (s.mode, s.players.clone())
    };
    for (slot, p) in players.iter().enumerate() {
        let present = mode == Mode::RetroArch && p.as_ref().is_some_and(|p| p.role == Role::Wiimote);
        link.set_present(slot as u8, present);
    }
}

/// El PC cambia el modo por su cuenta (modo automático: se abrió o cerró
/// Dolphin o Cemu). Se difunde a TODOS los móviles con `by:"pc"` (el que
/// tuviera una intención Wii U pendiente la descarta sin aviso), se
/// autoconfigura lo que toque y se avisa con `notice`. Sin móviles, el modo
/// queda puesto y el siguiente `ok` lo lleva.
pub fn set_mode_from_pc(shared: &SharedState, mode: Mode, notice: &str) {
    {
        let mut s = shared.lock_tolerant();
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
            .is_some_and(|c| shared.lock_tolerant().pair_code.try_accept(c.trim()));
    if !token_ok && !code_ok {
        let (code, msg) = if hello["code"].is_string() {
            ("bad_code", tr!("err.bad_code"))
        } else {
            // Que el PC también lo diga: quien mira la ventana entiende por
            // qué el móvil no entra (token.txt regenerado, PC reinstalado…)
            let who = hello["name"].as_str().filter(|n| !n.trim().is_empty()).unwrap_or(tr!("err.a_phone"));
            crate::log_line!("Móvil «{who}» ({peer_ip}) trae un QR antiguo: token rechazado");
            shared.lock_tolerant().last_error = Some(tr!("err.old_qr", who, peer_ip));
            ("bad_token", tr!("err.bad_token"))
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
                    "mode":shared.lock_tolerant().mode.as_str()}),
        );
        return;
    }

    let device_name = hello["name"].as_str().unwrap_or("Móvil").to_owned();
    let device_model = hello["model"].as_str().unwrap_or("").to_owned();
    // Papel: Nunchuk en la otra mano (ausente = mando)
    let role = if hello["role"].as_str() == Some("nunchuk") { Role::Nunchuk } else { Role::Wiimote };
    // Modo Wii U: quiere ser Mando Wii (ausente = GamePad / Pro según jugador)
    let pad_wii = role == Role::Wiimote && hello["pad"].as_str() == Some("wiimote");
    // Nunchuk en el mismo móvil (ausente = no: como un móvil anterior a 1.5.5)
    let own_nunchuk = role == Role::Wiimote && hello["nunchuk"].as_str() == Some("own");
    // Modo Wii U: el móvil GamePad solo hace de pantalla táctil (ausente = no)
    let screen_only = role == Role::Wiimote && hello["screen_only"].as_bool() == Some(true);
    // Modo Switch: qué mando de Switch quiere ser (ausente o de Wii U = Pro Controller)
    let switch_pad = hello["pad"].as_str().and_then(SwitchPad::parse).unwrap_or_default();
    // Modo RetroArch: RetroPad, mando de NES o pistola (ausente o de otro modo = RetroPad)
    let retro_pad = hello["pad"].as_str().and_then(RetroPadKind::parse).unwrap_or_default();
    // y la plantilla de consola que enseña (solo para la ventana)
    let retro_layout = hello["layout"].as_str().and_then(crate::retroarch::layout_id);

    let session_id: u32 = rand::thread_rng().gen();
    let (slot, evicted_slots) = {
        let mut guard = sessions.lock_tolerant();
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
        if role == Role::Nunchuk {
            "Nunchuk"
        } else if own_nunchuk {
            "mando + Nunchuk"
        } else if screen_only {
            "GamePad solo pantalla"
        } else {
            "mando"
        },
        if evicted_slots.is_empty() { String::new() } else { format!(" (fantasma desalojada en slot {evicted_slots:?})") }
    );

    let (mode, player) = {
        let mut s = shared.lock_tolerant();
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
            own_nunchuk,
            screen_only,
            switch_pad,
            retro_pad,
            retro_layout,
            tilt: false,
        });
        if !s.injection_error {
            s.last_error = None;
        }
        (s.mode, player_number(&s.players, slot))
    };
    // El mando universal solo se anuncia donde PUEDE existir: en macOS no hay
    // mando virtual sin un driver firmado por Apple, y anunciarlo dejaba al
    // móvil en un modo que ni mueve el cursor ni mueve nada.
    let universal = crate::rumble::status() != crate::rumble::Status::Unsupported;
    let modes: Vec<&str> = Mode::ALL
        .iter()
        .filter(|m| universal || **m != Mode::Gamepad)
        .map(|m| m.as_str())
        .collect();
    let pad = current_pad(shared, slot);
    if let Some(sess) = sessions.lock_tolerant().get_mut(&session_id) {
        sess.last_pad = Some(pad);
    }
    let mut ok = json!({"m":"ok","session_id":session_id,"udp_port":pairing.port,
                        "mode":mode.as_str(),"slot":slot,"name":pairing.name,
                        "role":if role == Role::Nunchuk { "nunchuk" } else { "wiimote" },
                        "player":pad.player,
                        "modes":modes,
                        "pad":pad.pad,
                        "half":null,
                        "side":null,
                        "nunchuk":if own_nunchuk { "own" } else { "none" },
                        "screen_only":screen_only,
                        // Este receptor entiende el apuntado por inclinación
                        // (INPUT flags bit4): el móvil solo lo pide si lo ve aquí
                        "tilt":true, "frame_rotation":true,
                        // Vibración de los juegos: si este PC puede mandarla
                        // (ready / driver / denied / unsupported), para la
                        // línea de Ajustes del móvil
                        "rumble":crate::rumble::status().as_str()});
    if code_ok {
        ok["token"] = json!(pairing.token);
    }
    let _ = send(&writer, &ok);
    // Qué juego tiene RetroArch (o null): el móvil elige con él la plantilla
    // de consola. Siempre la primera línea tras `ok`, en cualquier modo.
    let game = shared.lock_tolerant().retroarch_game.clone();
    let _ = send(&writer, &crate::retroarch::game_message(game.as_ref()));
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
                        let mut s = shared.lock_tolerant();
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
                    let cur = shared.lock_tolerant().mode;
                    if debug() {
                        eprintln!("[control] {device_name} (slot {slot}) pidió modo: solo decide el Jugador 1, sigue {}", cur.as_str());
                    }
                    let _ = send(&writer, &json!({"m":"mode","mode":cur.as_str()}));
                }
            }
            Some("pad") => {
                let changed={
                    let mut s=shared.lock_tolerant(); let mode=s.mode;
                    if let Some(p)=s.players[slot as usize].as_mut().filter(|p|p.role==Role::Wiimote) {
                        match (mode,msg["pad"].as_str()) {
                            (Mode::Switch,Some(value)) => {
                                if let Some(kind)=SwitchPad::parse(value) {
                                    let changed=p.switch_pad!=kind; p.switch_pad=kind; changed
                                } else {false}
                            }
                            (Mode::Cemu,Some(value @ ("wiimote" | "gamepad" | "pro")))=> {
                                let wii=value=="wiimote"; let changed=p.pad_wii!=wii;p.pad_wii=wii;changed
                            }
                            (Mode::RetroArch,Some(value)) => {
                                // y la plantilla de consola que enseña (solo para la ventana)
                                let layout=msg["layout"].as_str().and_then(crate::retroarch::layout_id);
                                if p.retro_layout!=layout {
                                    p.retro_layout=layout;
                                    crate::log_line!("Móvil «{device_name}»: plantilla {}", layout.unwrap_or("ninguna"));
                                }
                                if let Some(kind)=RetroPadKind::parse(value) {
                                    let changed=p.retro_pad!=kind; p.retro_pad=kind; changed
                                } else {false}
                            }
                            _=>false,
                        }
                    } else {false}
                };
                let effective=current_pad(shared,slot);
                let (wants_wii,layout)={
                    let s=shared.lock_tolerant();
                    (s.players[slot as usize].as_ref().is_some_and(|p|p.pad_wii), layout_of(&s.players, slot))
                };
                if let Some(sess)=sessions.lock_tolerant().get_mut(&session_id) {
                    sess.pad_wii=wants_wii;sess.last_pad=Some(effective);
                }
                let _=send(&writer,&pad_message(effective, layout));
                if changed {auto_configure(shared,sessions);}
            }
            Some("hotkey") => {
                // Tecla rápida de RetroArch (menú, guardar/cargar estado,
                // avance rápido…): solo en modo RetroArch; las de mantener
                // llevan `down` true/false, las demás disparan con `down` true
                let name = msg["name"].as_str().unwrap_or("");
                let down = msg["down"].as_bool().unwrap_or(true);
                let in_mode = shared.lock_tolerant().mode == Mode::RetroArch;
                let ok = in_mode
                    && role == Role::Wiimote
                    && crate::retroarch::link().is_some_and(|l| l.hotkey(slot, name, down));
                if debug() {
                    eprintln!("[control] {device_name} (slot {slot}) hotkey {name} {down} → {ok}");
                }
                let _ = send(&writer, &json!({"m":"hotkey","name":name,"down":down,"ok":ok}));
            }
            Some("nunchuk") => {
                // Nunchuk en el mismo móvil (modo Dolphin): el mando manda
                // también stick, C y Z, y su Wiimote emulado lleva Extension =
                // Nunchuk leyendo de su mismo pad. Eco siempre; reconfigurar
                // Dolphin solo si cambia (con Dolphin abierto queda pendiente).
                if role == Role::Wiimote {
                    let own = msg["own"].as_bool().unwrap_or(false);
                    let changed = {
                        let mut s = shared.lock_tolerant();
                        match s.players[slot as usize].as_mut() {
                            Some(p) if p.own_nunchuk != own => {
                                p.own_nunchuk = own;
                                true
                            }
                            _ => false,
                        }
                    };
                    if debug() {
                        eprintln!("[control] {device_name} (slot {slot}) Nunchuk propio → {own}");
                    }
                    let _ = send(&writer, &json!({"m":"nunchuk","own":own}));
                    if changed {
                        crate::log_line!(
                            "Móvil «{device_name}»: Nunchuk en el mismo móvil {}",
                            if own { "activado" } else { "desactivado" }
                        );
                        auto_configure(shared, sessions);
                    }
                } else {
                    let _ = send(&writer, &json!({"m":"nunchuk","own":false}));
                }
            }
            Some("screen_only") => {
                // Modo Wii U: el móvil GamePad solo hace de pantalla táctil; el
                // mando real del usuario sigue siendo el Controller 1 de Cemu y
                // el receptor fusiona el DSU del móvil en su perfil. Eco
                // siempre; reconfigurar Cemu solo si cambia (abierto: pendiente).
                if role == Role::Wiimote {
                    let on = msg["on"].as_bool().unwrap_or(false);
                    let changed = {
                        let mut s = shared.lock_tolerant();
                        match s.players[slot as usize].as_mut() {
                            Some(p) if p.screen_only != on => {
                                p.screen_only = on;
                                true
                            }
                            _ => false,
                        }
                    };
                    let _ = send(&writer, &json!({"m":"screen_only","on":on}));
                    if changed {
                        crate::log_line!(
                            "Móvil «{device_name}»: solo pantalla {}",
                            if on { "activado" } else { "desactivado" }
                        );
                        crate::cemu::maybe_auto_configure(shared);
                    }
                } else {
                    let _ = send(&writer, &json!({"m":"screen_only","on":false}));
                }
            }
            Some("text") => {
                // Teclado del móvil → teclado en pantalla de Cemu (no acepta
                // toques, solo teclas): a la ventana de Cemu en modo Wii U;
                // si no se la encuentra, o en otros modos, al SO (ventana con
                // el foco) desde el hilo de telemetría
                if let Some(t) = msg["text"].as_str().filter(|t| !t.is_empty()) {
                    // Tope de longitud: el tecleo va en el hilo caliente (el
                    // que mueve el puntero y alimenta el DSU), así que un
                    // pegado enorme lo dejaría ocupado durante segundos
                    let (t, cortado) = text_plan::clamp_text(t);
                    if cortado {
                        let _ = send(&writer, &json!({"m":"notice","text":tr!("text.truncated", text_plan::MAX_TEXT)}));
                    }
                    let target = shared.lock_tolerant().mode;
                    // RetroArch: a su ventana (se activa desde telemetría)
                    let wiiu = target == Mode::Cemu;
                    let to_os = if wiiu {
                        // sin camino directo a la ventana (Linux): al SO, pero
                        // solo con Cemu abierto (que tendrá el foco)
                        !crate::screen::type_text(t) && cfg!(target_os = "linux") && crate::cemu::running_exe().0
                    } else {
                        true
                    };
                    if to_os {
                        let lleno = {
                            let mut s = shared.lock_tolerant();
                            let lleno = s.text_queue.len() >= MAX_TEXT_QUEUE;
                            if !lleno {
                                s.text_queue.push(PendingText { mode: target, text: t.to_owned(), slot });
                            }
                            lleno
                        };
                        if lleno {
                            let _ = send(&writer, &json!({"m":"notice","text":tr!("text.queue_full")}));
                        }
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
    let still_mine = sessions.lock_tolerant().remove(&session_id).is_some();
    crate::log_line!(
        "Móvil «{device_name}» (slot {slot}) se va{}",
        if still_mine { "" } else { " — ya desalojada por su reconexión" }
    );
    if !still_mine {
        return;
    }
    let (empty, player, was_screen_only) = {
        let mut s = shared.lock_tolerant();
        // el número de jugador se calcula ANTES de vaciar la plaza (su campanita)
        let player = player_number(&s.players, slot);
        let was_screen_only = s.players[slot as usize].as_ref().is_some_and(|p| p.screen_only);
        s.players[slot as usize] = None;
        let empty = s.player_count() == 0;
        if empty {
            s.status = LinkStatus::Waiting;
            s.pps = 0.0;
            s.sensor_hz = 0.0;
            s.rtt_hist.clear();
        }
        (empty, player, was_screen_only)
    };
    crate::sound::disconnect_chime(player);
    // Un mando de RetroArch que se va con algo pulsado: el enlace lo suelta
    sync_retroarch_presence(shared);
    if !empty {
        auto_configure(shared, sessions);
    } else {
        // sin móviles no queda ningún mando virtual
        crate::rumble::sync(shared);
    }
    if empty && was_screen_only {
        // el último móvil era «solo pantalla»: el perfil del usuario vuelve
        crate::cemu::cleanup_after_screen_only(shared);
    }
}

fn send(w: &Writer, v: &Value) -> std::io::Result<()> {
    let mut s = v.to_string();
    s.push('\n');
    let mut w = w.lock().unwrap_or_else(|e| e.into_inner());
    w.write_all(s.as_bytes())
}
