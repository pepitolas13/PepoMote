//! Hilo caliente: UDP INPUT → puntero (Jugador 1) y DSU (todos los slots).
//! También responde el broadcast de descubrimiento y mide RTT por jugador.

use crate::state::LockTolerant;
use super::codec::{self, Packet};
use super::Sessions;
use crate::dsu::{wii_ir, Dsu, DsuProfile, MotionSample, WiiIr};
use crate::input::{self, KeyCode, MouseButton};
use crate::pairing::PairingInfo;
use crate::pointer::{PointerEngine, PointerOutput};
use crate::state::{Mode, Role, SharedState};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::tr;

/// Mapeo bit de botón → acción en modo puntero (PROTOCOL.md §4.2).
enum Action {
    Mouse(MouseButton),
    Key(KeyCode),
}

const BUTTON_MAP: [(u32, Action); 16] = [
    (1 << 0, Action::Mouse(MouseButton::Left)),   // A
    (1 << 1, Action::Mouse(MouseButton::Right)),  // B
    (1 << 2, Action::Key(KeyCode::ArrowUp)),
    (1 << 3, Action::Key(KeyCode::ArrowDown)),
    (1 << 4, Action::Key(KeyCode::BrowserBack)),    // cruceta ←: atrás en el navegador
    (1 << 5, Action::Key(KeyCode::BrowserForward)), // cruceta →: adelante
    (1 << 6, Action::Key(KeyCode::VolumeUp)),     // Plus
    (1 << 7, Action::Key(KeyCode::VolumeDown)),   // Minus
    (1 << 9, Action::Key(KeyCode::Enter)),        // Uno
    (1 << 10, Action::Key(KeyCode::Escape)),      // Dos
    (1 << 11, Action::Key(KeyCode::VolumeUp)),
    (1 << 12, Action::Key(KeyCode::VolumeDown)),
    (1 << 13, Action::Key(KeyCode::Mute)),
    (1 << 14, Action::Key(KeyCode::PlayPause)),
    (1 << 15, Action::Key(KeyCode::NextTrack)),
    (1 << 16, Action::Key(KeyCode::PrevTrack)),
];

/// El SO no repite las teclas inyectadas: mantener el volumen sería un solo
/// paso. Mientras el móvil sostiene el bit, el receptor re-toca la tecla
/// (soltar + pulsar) cada [`REPEAT_EVERY`] a partir de [`REPEAT_DELAY`].
/// 350 ms: más que un toque deliberado (el latch del móvil da ≥ 70 ms) y
/// menos que el retardo de teclado de Windows (500 ms). 100 ms: Windows sube
/// un 2 % por pulsación → 20 %/s (de 0 a 100 en 5 s).
const REPEAT_DELAY: Duration = Duration::from_millis(350);
const REPEAT_EVERY: Duration = Duration::from_millis(100);

/// Repetición en curso: la tecla y cuándo toca el siguiente re-toque.
struct Repeat {
    key: KeyCode,
    next: Instant,
}

/// Solo repite el volumen: por tecla, no por bit (Plus/Minus y vol± dan las mismas).
fn repeats(key: KeyCode) -> bool {
    matches!(key, KeyCode::VolumeUp | KeyCode::VolumeDown)
}

/// Primera tecla repetible entre las que el SO ve pulsadas (`held`).
fn repeat_after(held: u32) -> Option<KeyCode> {
    BUTTON_MAP.iter().find_map(|(bit, action)| match action {
        Action::Key(k) if held & bit != 0 && repeats(*k) => Some(*k),
        _ => None,
    })
}

/// Un tic del repetidor. Todo sale de `held`: `release_all` lo deja a 0 y
/// con eso muere la repetición (cambio de modo, sesión nueva, Jugador 1
/// ido) sin más avisos. Un re-toque por tic y el siguiente a contar desde
/// ahora: tras un parón no hay ráfaga de recuperación.
fn repeat_tick(rep: &mut Option<Repeat>, held: u32, injector: &mut dyn input::Injector, now: Instant) {
    let Some(key) = repeat_after(held) else {
        *rep = None;
        return;
    };
    match rep {
        Some(r) if r.key == key => {
            if now >= r.next {
                injector.key(key, false);
                injector.key(key, true);
                r.next = now + REPEAT_EVERY;
            }
        }
        _ => *rep = Some(Repeat { key, next: now + REPEAT_DELAY }),
    }
}

pub fn run(
    shared: SharedState,
    sessions: Sessions,
    pairing: PairingInfo,
    dsu: Option<Arc<Dsu>>,
) {
    let socket = match crate::ports::bind_udp(&shared, "0.0.0.0", pairing.port, tr!("port.what_phone")) {
        Ok(s) => s,
        Err(e) => {
            shared.lock_tolerant().last_error = Some(e);
            return;
        }
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(100)));
    // La vibración de los juegos sale por este mismo socket (PROTOCOL.md §4.5)
    if let Ok(s) = socket.try_clone() {
        crate::rumble::set_socket(s);
    }

    // El inyector puede no nacer a la primera (/dev/uinput sin permiso, o el
    // compositor aún sin arrancar): el resto del receptor sigue vivo (Dolphin
    // no lo necesita) y se reintenta en el bucle — la auto-reparación da
    // permiso sin reiniciar la app.
    let mut injector: Option<Box<dyn input::Injector>> = None;
    let mut injector_retry = Instant::now().checked_sub(Duration::from_secs(60)).unwrap_or_else(Instant::now);
    // Último error de creación ya enseñado (para no repetirlo cada 2 s)
    let mut injector_err_seen: Option<String> = None;

    // En Linux, el hilo de pantallas (screens::watch) publica el mapeo del
    // apuntado en shared.pointing; aquí solo se lee (barato) y se aplica al
    // inyector cuando cambia. Nunca se llama a la detección en este hilo
    // caliente: un roundtrip Wayland lento no puede congelar el cursor.
    #[allow(unused_mut)]
    let mut aspect = screen_aspect();
    #[allow(unused_mut)]
    let mut screen_w = screen_width();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let mut last_norm: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
    let mut engine = PointerEngine::new();
    let start = Instant::now();
    let now_us = |s: Instant| s.elapsed().as_micros() as u64;

    let mut buf = [0u8; 128];
    // Botones que el SO ve pulsados ahora mismo (solo inyecta el Jugador 1).
    // Si su sesión muere o el modo deja de ser puntero con A/B/tecla
    // sostenidos, hay que soltarlos: si no, el clic queda atascado en el SO.
    let mut held = OsHeld::default();
    // Volumen mantenido: la repetición en curso (nace y muere con `held`)
    let mut repeat: Option<Repeat> = None;
    // El motor de puntero pertenece al Jugador 1: se resetea si cambia su
    // sesión; si es el mismo móvil que se reconecta, hereda el sesgo del
    // gyro que ya había aprendido
    let mut engine_session: Option<u32> = None;
    let mut engine_device: Option<String> = None;
    // Modo Wii U con «Mando Wii»: cada uno de esos móviles tiene su propio
    // motor de puntero (su IR va a Cemu por el touchpad DSU, no al SO)
    let mut ir_engines: HashMap<u32, IrPointer> = HashMap::new();
    // PEPOMOTE_RECORD=<archivo>: grabar la telemetría del Jugador 1 (y las
    // órdenes de vibración que se le mandan) para analizar un gesto real
    // después (pointer/record.rs)
    crate::pointer::record::start_from_env();
    // Modo mando universal: qué se le ha escrito ya a cada mando virtual,
    // para no repetir estados (a 250 Hz serían cientos de IOCTL de más)
    let mut feed = crate::pad::Feed::new();
    // El mando virtual sobrevive a los cambios de modo, así que al cambiar
    // deja de valer lo que creemos haberle escrito
    let mut last_mode = Mode::Pointer;

    let mut win_start = Instant::now();
    let mut win_packets: u32 = 0;
    let mut win_first_t: Option<u64> = None;
    let mut win_last_t: u64 = 0;
    let mut last_ping = Instant::now();

    loop {
        if injector.is_none() && injector_retry.elapsed() >= Duration::from_secs(2) {
            injector_retry = Instant::now();
            match input::new_injector() {
                Ok(i) => {
                    // inyector nuevo: que reciba la pantalla de apuntado ya
                    #[cfg(any(target_os = "linux", target_os = "macos"))]
                    {
                        last_norm = [-1.0, -1.0, -1.0, -1.0]; // forzar re-aplicar
                    }
                    crate::log_line!("Inyección: {}", i.name());
                    let mut s = shared.lock_tolerant();
                    s.injector = Some(i.name());
                    s.uinput_denied = false;
                    s.uinput_missing = false;
                    s.ax_denied = false;
                    if s.injection_error {
                        s.last_error = None;
                        s.injection_error = false;
                    }
                    drop(s);
                    injector_err_seen = None;
                    injector = Some(i);
                }
                Err(e) => {
                    if injector_err_seen.as_deref() != Some(e.msg.as_str()) {
                        crate::log_line!("Inyección de entrada: sin inyector: {}", e.msg);
                        injector_err_seen = Some(e.msg.clone());
                    }
                    // macOS: el diálogo del sistema que lleva a Ajustes →
                    // Accesibilidad, una sola vez por proceso
                    #[cfg(target_os = "macos")]
                    if e.ax_denied {
                        crate::macos::ax_prompt_once();
                    }
                    let mut s = shared.lock_tolerant();
                    // solo si uinput se intentó de verdad y /dev/uinput falló:
                    // enciende «Reparar ahora» (nunca con el backend Wayland)
                    s.uinput_denied = e.uinput_denied;
                    s.uinput_missing = e.uinput_missing;
                    s.ax_denied = e.ax_denied;
                    s.injector = None;
                    s.last_error = Some(tr!("inj.error", e.msg));
                    s.injection_error = true;
                }
            }
        }

        // Linux y macOS: aplicar el mapeo de pantalla que publica
        // screens::watch (el dispositivo absoluto cubre TODO el escritorio;
        // esto lo dirige a la pantalla elegida, o a todas). Solo se re-aplica
        // cuando cambia.
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let pointing = shared.lock_tolerant().pointing;
            if let Some((norm, asp, sw)) = pointing {
                aspect = asp;
                screen_w = sw;
                if norm != last_norm {
                    last_norm = norm;
                    if let Some(inj) = injector.as_deref_mut() {
                        inj.set_screen(norm);
                    }
                }
            }
        }

        // Texto pendiente de teclear (teclado de Cemu desde el móvil)
        let pending = std::mem::take(&mut shared.lock_tolerant().text_queue);
        if !pending.is_empty() {
            if let Some(inj) = injector.as_deref_mut() {
                for p in &pending {
                    // Eden y RetroArch: a su ventana, activada antes (donde
                    // el SO lo permite); si no está, se avisa y no se teclea
                    // en otra aplicación
                    let (focused, unavailable) = match p.mode {
                        Mode::Switch => (crate::eden::focus_keyboard(), tr!("eden.keyboard_unavailable")),
                        Mode::RetroArch => (crate::retroarch::focus_keyboard(), tr!("retroarch.keyboard_unavailable")),
                        _ => (true, ""),
                    };
                    if focused {
                        // Lo que el backend no pudo teclear (Unicode donde no
                        // hay forma, o una ventana que el SO protege) vuelve
                        // al móvil que lo escribió: nunca se pierde en silencio
                        let report = inj.type_text(&p.text);
                        if let Some(aviso) = input::text_plan::type_notice(&report) {
                            super::notify_slot(p.slot, &aviso);
                        }
                    } else {
                        super::notify_all(unavailable);
                    }
                }
            }
        }

        // Ping de RTT a TODOS los jugadores
        if last_ping.elapsed() > Duration::from_millis(500) {
            last_ping = Instant::now();
            // Backend Wayland: la conexión puede morir (compositor reiniciado,
            // error de protocolo). Se tira el inyector y el reintento de
            // arriba lo recrea en ≤2 s; lo que el SO viera pulsado ya no existe.
            if injector.as_deref_mut().is_some_and(|i| !i.alive()) {
                // (en macOS, `alive` = el permiso de Accesibilidad sigue dado)
                crate::log_line!("Inyección de entrada: el inyector ha muerto; se vuelve a crear");
                injector = None;
                held = OsHeld::default();
                repeat = None;
                injector_err_seen = None;
                let mut s = shared.lock_tolerant();
                s.injector = None;
                #[cfg(target_os = "macos")]
                {
                    s.ax_denied = true;
                    s.last_error = Some(tr!("inj.mac_ax_denied").to_owned());
                }
                #[cfg(not(target_os = "macos"))]
                {
                    s.last_error = Some(tr!("inj.wayland_lost").to_owned());
                }
                s.injection_error = true;
            }
            let targets: Vec<(u32, std::net::SocketAddr)> = sessions
                .lock_tolerant()
                .values()
                .filter_map(|s| s.phone_udp.map(|a| (s.id, a)))
                .collect();
            for (id, addr) in targets {
                let _ = socket.send_to(&codec::build_ping(id, now_us(start)), addr);
            }
            // Límites reales del cursor (pueden cambiar: monitores que van y
            // vienen); barato, cada 500 ms
            if let Some(inj) = injector.as_deref_mut() {
                engine.set_cursor_bounds(inj.cursor_bounds());
            }
            // El Jugador 1 se ha ido (bye o timeout): nada puede quedar pulsado
            let j1_gone = match engine_session {
                Some(id) => !sessions.lock_tolerant().contains_key(&id),
                None => false,
            };
            if j1_gone {
                engine_session = None;
                if let Some(inj) = injector.as_deref_mut() {
                    release_all(inj, &mut held);
                }
            }
            if !ir_engines.is_empty() {
                let alive = sessions.lock_tolerant();
                ir_engines.retain(|id, _| alive.contains_key(id));
            }
            // Mando universal: un móvil que se calla (app al fondo, red que
            // se va) no puede dejarle un botón clavado al juego, y un slot
            // vacío tiene que olvidarse para reescribir el mando que venga
            for (slot, st) in feed.idle(Instant::now()) {
                crate::rumble::push_pad(slot as u8, &st);
            }
            let vivos = {
                let alive = sessions.lock_tolerant();
                let mut v = [false; super::MAX_PLAYERS];
                for s in alive.values() {
                    if let Some(f) = v.get_mut(s.slot as usize) {
                        *f = true;
                    }
                }
                v
            };
            feed.forget_absent(&vivos);
        }

        if win_start.elapsed() >= Duration::from_secs(1) {
            let secs = win_start.elapsed().as_secs_f32();
            let hz = match (win_first_t, win_packets) {
                (Some(t0), n) if n > 1 && win_last_t > t0 => {
                    (n - 1) as f32 / ((win_last_t - t0) as f32 / 1e6)
                }
                _ => 0.0,
            };
            {
                let mut s = shared.lock_tolerant();
                s.pps = win_packets as f32 / secs;
                s.sensor_hz = hz;
            }
            win_start = Instant::now();
            win_packets = 0;
            win_first_t = None;
        }

        // Volumen mantenido: re-toque periódico (tras cada paquete y, sin
        // paquetes, cada 100 ms por el timeout de lectura)
        if let Some(inj) = injector.as_deref_mut() {
            repeat_tick(&mut repeat, held.bits, inj, Instant::now());
        }

        let (len, from) = match socket.recv_from(&mut buf) {
            Ok(r) => r,
            Err(_) => continue,
        };

        match codec::parse(&buf[..len]) {
            // Rumble travels from the receiver to the controller, never back.
            Some(Packet::Rumble(_)) => {}
            Some(Packet::Discover) => {
                let reply = json!({"pv": 1, "name": pairing.name, "tcp": pairing.port});
                let mut out = codec::HERE_PREFIX.to_vec();
                out.extend_from_slice(reply.to_string().as_bytes());
                let _ = socket.send_to(&out, from);
            }
            Some(Packet::Ping { session_id, t_us }) => {
                let _ = socket.send_to(&codec::build_pong(session_id, t_us), from);
            }
            Some(Packet::Pong { session_id, t_us }) => {
                let rtt_ms = (now_us(start).saturating_sub(t_us)) as f32 / 1000.0;
                let slot = sessions
                    .lock_tolerant()
                    .get(&session_id)
                    .map(|s| s.slot as usize);
                if let Some(slot) = slot {
                    let mut s = shared.lock_tolerant();
                    if let Some(p) = s.players[slot].as_mut() {
                        p.rtt_ms = Some(rtt_ms);
                    }
                    if slot == 0 {
                        // el latido del Jugador 1 se ve en la ventana (sparkline)
                        if s.rtt_hist.len() >= crate::state::RTT_HIST {
                            s.rtt_hist.pop_front();
                        }
                        s.rtt_hist.push_back(rtt_ms);
                    }
                }
            }
            Some(Packet::Input(p)) => {
                // Validar sesión, seq y aprender la dirección UDP del jugador
                let (slot, role) = {
                    let mut guard = sessions.lock_tolerant();
                    let Some(sess) = guard.get_mut(&p.session_id) else {
                        continue;
                    };
                    if let Some(last) = sess.last_seq {
                        if p.seq.wrapping_sub(last) == 0 || p.seq.wrapping_sub(last) > u32::MAX / 2
                        {
                            continue;
                        }
                    }
                    sess.last_seq = Some(p.seq);
                    sess.phone_udp = Some(from);
                    (sess.slot, sess.role)
                };

                win_packets += 1;
                // Hz del sensor solo con el reloj del Jugador 1: los t_sensor
                // de móviles distintos no son comparables entre sí
                if slot == 0 {
                    if win_first_t.is_none() {
                        win_first_t = Some(p.t_sensor_us);
                    }
                    win_last_t = p.t_sensor_us;
                    crate::pointer::record::write(&buf[..len]);
                }

                let (mode, sens_deg, abs_mode, pad_wii, own_nunchuk, retro_pad) = {
                    let mut s = shared.lock_tolerant();
                    let mut pad_wii = false;
                    let mut own_nunchuk = false;
                    let mut retro_pad = crate::retroarch::RetroPadKind::RetroPad;
                    if let Some(pl) = s.players[slot as usize].as_mut() {
                        pl.battery_pct = p.battery_pct;
                        pl.tilt = p.flags & codec::FLAG_TILT != 0;
                        pad_wii = pl.pad_wii;
                        own_nunchuk = pl.own_nunchuk;
                        retro_pad = pl.retro_pad;
                    }
                    (s.mode, s.config.sens_deg, s.config.abs_mode, pad_wii, own_nunchuk, retro_pad)
                };

                if mode != last_mode {
                    last_mode = mode;
                    feed.forget_all();
                }

                // RetroArch: botones y sticks al mando en red (el enlace los
                // dosifica a un datagrama por fotograma). Wii y pistola del
                // Jugador 1 además apunta con el ratón del SO, más abajo.
                let retro_pointer = mode == Mode::RetroArch && retro_pad.points() && role == Role::Wiimote;
                if mode == Mode::RetroArch {
                    if let Some(link) = crate::retroarch::link() {
                        if role == Role::Wiimote {
                            link.push_input(slot, retro_pad, &p);
                        }
                    }
                }

                // Mando universal: al mando de Xbox 360 virtual que ya
                // existe para la vibración. No se configura nada en el juego
                // (lo crea el sistema) ni se inyecta nada en el SO.
                if mode == Mode::Gamepad {
                    if let Some(inj) = injector.as_deref_mut() {
                        release_all(inj, &mut held);
                    }
                    if role == Role::Wiimote {
                        if let Some(st) = feed.push(slot as usize, crate::pad::state_of(&p), Instant::now()) {
                            // Si no había mando que escribir (driver a medio
                            // instalar, mando recién tirado), se olvida: el
                            // `Feed` solo manda lo que cambia y el mando nuevo
                            // nace en reposo.
                            if !crate::rumble::push_pad(slot, &st) {
                                feed.forget(slot as usize);
                            }
                        }
                    }
                    continue;
                }

                if mode.feeds_dsu() || (mode == Mode::RetroArch && !(retro_pointer && slot == 0)) {
                    // Cambio a Dolphin/Cemu/RetroArch con algo sostenido: soltarlo en el SO
                    // Otro jugador/Nunchuk nunca suelta el ratón del Jugador 1.
                    if mode.feeds_dsu() || (slot == 0 && role == Role::Wiimote) {
                        if let Some(inj) = injector.as_deref_mut() {
                            release_all(inj, &mut held);
                        }
                        engine_session = None;
                    }
                    if mode == Mode::RetroArch {
                        continue;
                    }
                    // Todos los jugadores al DSU, cada uno en su slot, INLINE
                    if let Some(dsu) = &dsu {
                        let (profile, touch, wii_ir) = match mode {
                            Mode::Cemu => {
                                let touch = if role == Role::Wiimote && pad_wii {
                                    // Mando Wii en Cemu: su puntero IR es el
                                    // touchpad DSU (Cemu lo lee como posición)
                                    ir_engines
                                        .entry(p.session_id)
                                        .or_default()
                                        .shaking(crate::rumble::is_shaking(slot))
                                        .apply(&p, sens_deg, aspect, screen_w)
                                } else {
                                    gamepad_touch(&p)
                                };
                                (DsuProfile::WiiU, touch, None)
                            }
                            // Switch: el mismo paquete de 80 bytes que el
                            // GamePad, sin táctil (Eden no lo usa)
                            Mode::Switch => (DsuProfile::Switch, None, None),
                            // Dolphin: los puntos IR del Mando Wii los genera
                            // el receptor (perfil IRPassthrough, dsu/wii_ir.rs);
                            // un Nunchuk no apunta
                            _ => {
                                let wii_ir = (role == Role::Wiimote).then(|| {
                                    ir_engines
                                        .entry(p.session_id)
                                        .or_default()
                                        .shaking(crate::rumble::is_shaking(slot))
                                        .apply_wii(&p, screen_w, own_nunchuk)
                                });
                                (DsuProfile::Wii, None, wii_ir.flatten())
                            }
                        };
                        dsu.push(
                            slot,
                            &MotionSample {
                                t_us: p.t_sensor_us,
                                accel_ms2: p.accel,
                                gyro_rads: p.gyro,
                                buttons: p.buttons,
                                battery_pct: p.battery_pct,
                                recenter_count: p.recenter_count,
                                stick_x: p.stick_x,
                                stick_y: p.stick_y,
                                stick_rx: p.stick_rx,
                                stick_ry: p.stick_ry,
                                touch,
                                wii_ir,
                                profile,
                            },
                        );
                    }
                } else if slot == 0 && role == Role::Wiimote {
                    // Modo puntero (o Wii/pistola de RetroArch): el SO tiene UN
                    // cursor y es del Jugador 1 (un Nunchuk nunca mueve el cursor)
                    let Some(inj) = injector.as_deref_mut() else {
                        continue; // sin inyector aún: se está reintentando
                    };
                    if engine_session != Some(p.session_id) {
                        engine_session = Some(p.session_id);
                        let device = sessions.lock_tolerant().get(&p.session_id).map(|s| s.device.clone());
                        let same_phone = device.is_some() && device == engine_device && engine.same_frame(&p);
                        engine = if same_phone { PointerEngine::with_bias(engine.bias()) } else { PointerEngine::new() };
                        engine_device = device;
                        engine.set_cursor_bounds(inj.cursor_bounds());
                        release_all(inj, &mut held);
                        inj.move_abs(0.5, 0.5);
                    }

                    // Con el motor de vibración encendido el gyro viene sacudido:
                    // el motor no aprende el sesgo mientras tanto (engine.rs)
                    engine.set_shaking(crate::rumble::is_shaking(slot));
                    engine.set_cursor_hint(inj.cursor_pos());
                    let precision = p.buttons & codec::BTN_PRECISION != 0;
                    match engine.apply(&p, sens_deg, aspect, abs_mode, screen_w, precision) {
                        PointerOutput::Abs { nx, ny } => inj.move_abs(nx, ny),
                        PointerOutput::Rel { dx, dy } => inj.move_rel(dx, dy),
                        PointerOutput::None => {}
                    }
                    if p.touch_scroll_dy != 0 {
                        inj.wheel(p.touch_scroll_dy as i32 * 4);
                    }
                    // Pistola de luz: B es el gatillo (clic izquierdo) y A la
                    // recarga fuera de pantalla (clic derecho); el resto va al
                    // RetroPad por el enlace, no al SO
                    if retro_pointer {
                        let map = if retro_pad == crate::retroarch::RetroPadKind::Gun { &GUN_MAP[..] } else { &[] };
                        // Wii conserva X/Y por red; no duplica clics ni atajos.
                        // El mapa vacío también suelta un gatillo anterior.
                        let bits = if map.is_empty() { 0 } else { p.buttons & crate::retroarch::GUN_MOUSE_BITS };
                        apply_buttons_map(inj, &mut held, bits, map);
                    } else {
                        apply_buttons(inj, &mut held, p.buttons);
                    }
                }
                // slot > 0 en modo puntero: se ignora (apunta el Jugador 1)
            }
            None => {}
        }
    }
}

/// Resolución del touchpad DSU (la de un DS4): Cemu divide por esto para
/// obtener la posición 0..1 (`DSUController::get_position`).
const DSU_TOUCH_W: f32 = 1920.0;
const DSU_TOUCH_H: f32 = 942.0;

/// Pantalla táctil del GamePad: fracción 0..65535 del paquete → touchpad DSU.
fn gamepad_touch(p: &codec::InputPacket) -> Option<(u16, u16)> {
    if p.flags & codec::FLAG_EXT == 0 || p.flags & codec::FLAG_TOUCH == 0 {
        return None;
    }
    let scale = |v: u16, max: f32| (v as f32 / 65535.0 * max).round() as u16;
    Some((scale(p.touch_x, DSU_TOUCH_W), scale(p.touch_y, DSU_TOUCH_H)))
}

/// Puntero IR de un Mando Wii dentro de un emulador: el mismo motor absoluto
/// que mueve el cursor del PC, pero su salida va por el DSU. En Cemu es el
/// touchpad (Cemu lo lee como posición; fuera de la pantalla, con margen, el
/// toque se apaga y el juego esconde el cursor). En Dolphin son los dos
/// puntos de la barra sensora (dsu/wii_ir.rs), con la rampa de «acercar» y
/// el roll del móvil.
struct IrPointer {
    engine: PointerEngine,
    /// Último apuntado absoluto: (yaw, pitch) en grados respecto al
    /// recentrado, positivo = derecha / arriba. Lo comparten Cemu y Dolphin:
    /// el motor calla mientras el móvil está quieto («congelado»), así que al
    /// cambiar de modo sin moverlo el puntero sigue donde estaba.
    last_angles: Option<(f32, f32)>,
    /// Dolphin: rampa de niveles del bit «acercar».
    near: wii_ir::NearRamp,
}

impl Default for IrPointer {
    fn default() -> Self {
        Self { engine: PointerEngine::new(), last_angles: None, near: wii_ir::NearRamp::new() }
    }
}

impl IrPointer {
    /// El motor de vibración de este móvil está encendido (o acaba de
    /// apagarse): ver `PointerEngine::set_shaking`.
    fn shaking(&mut self, on: bool) -> &mut Self {
        self.engine.set_shaking(on);
        self
    }

    /// Pasa el paquete por el motor y devuelve el apuntado en grados. La
    /// salida absoluta del motor es lineal en `sens_deg` (grados que cubre el
    /// ancho, y `sens_deg / aspect` el alto), así que se deshace aquí. Sin
    /// quaternion (salida relativa) no hay apuntado absoluto que dar;
    /// congelado en reposo se mantiene el último.
    fn track(&mut self, p: &codec::InputPacket, sens_deg: f32, aspect: f32, screen_w: f32) -> Option<(f32, f32)> {
        match self.engine.apply(p, sens_deg, aspect, true, screen_w, false) {
            PointerOutput::Abs { nx, ny } => {
                self.last_angles = Some(((nx - 0.5) * sens_deg, (0.5 - ny) * sens_deg / aspect));
            }
            PointerOutput::Rel { .. } => self.last_angles = None,
            PointerOutput::None => {}
        }
        self.last_angles
    }

    /// Dolphin: los puntos IR de este paquete (`wii_ir::encode`, la cámara
    /// que Dolphin sintetiza). `own_nunchuk`: Left X es el stick del Nunchuk
    /// propio, el par va sin roll. Sin quaternion utilizable el roll es 0.
    fn apply_wii(&mut self, p: &codec::InputPacket, screen_w: f32, own_nunchuk: bool) -> Option<WiiIr> {
        let angles = self.track(p, wii_ir::SENS_DEG, wii_ir::ASPECT, screen_w);
        let level = self.near.update(p.buttons & codec::BTN_NEAR != 0, Instant::now());
        let roll = if own_nunchuk {
            None
        } else if p.flags & codec::FLAG_QUAT_VALID != 0 && p.flags & codec::FLAG_TILT == 0 {
            Some(wii_ir::roll_rad(p.quat))
        } else {
            Some(0.0)
        };
        let (yaw, pitch) = angles?;
        wii_ir::encode(yaw, pitch, level, roll)
    }

    /// Cemu: el puntero como toque del touchpad DSU; fuera de la pantalla
    /// (con margen) el toque se apaga y el juego esconde el cursor.
    fn apply(&mut self, p: &codec::InputPacket, sens_deg: f32, aspect: f32, screen_w: f32) -> Option<(u16, u16)> {
        let (yaw, pitch) = self.track(p, sens_deg, aspect, screen_w)?;
        let nx = 0.5 + yaw / sens_deg;
        let ny = 0.5 - pitch * aspect / sens_deg;
        let on_screen = (-0.05..=1.05).contains(&nx) && (-0.05..=1.05).contains(&ny);
        on_screen.then(|| {
            (
                (nx.clamp(0.0, 1.0) * DSU_TOUCH_W).round() as u16,
                (ny.clamp(0.0, 1.0) * DSU_TOUCH_H).round() as u16,
            )
        })
    }
}

/// Pistola de luz de RetroArch (Mando Wii apuntando): B = gatillo, A =
/// recarga fuera de pantalla, con los enlaces por defecto de RetroArch
/// (`gun_trigger_mbtn = 1`, `gun_offscreen_shot_mbtn = 2`).
const GUN_MAP: [(u32, Action); 2] = [
    (codec::BTN_B, Action::Mouse(MouseButton::Left)),
    (codec::BTN_A, Action::Mouse(MouseButton::Right)),
];

/// Lo que el SO ve pulsado ahora mismo y con qué tabla se pulsó (puntero o
/// pistola): al cambiar de tabla se suelta primero con la antigua, que es la
/// que sabe qué botón del SO corresponde a cada bit.
struct OsHeld {
    bits: u32,
    map: &'static [(u32, Action)],
}

impl Default for OsHeld {
    fn default() -> Self {
        Self { bits: 0, map: &BUTTON_MAP }
    }
}

/// Inyecta los flancos entre lo que el SO ve pulsado (`held`) y `target`
/// con la tabla del puntero.
fn apply_buttons(injector: &mut dyn input::Injector, held: &mut OsHeld, target: u32) {
    apply_buttons_map(injector, held, target, &BUTTON_MAP);
}

/// Como [`apply_buttons`] con la tabla `map`.
fn apply_buttons_map(injector: &mut dyn input::Injector, held: &mut OsHeld, target: u32, map: &'static [(u32, Action)]) {
    if !std::ptr::eq(held.map, map) {
        if held.bits != 0 {
            let old = held.map;
            apply_buttons_map(injector, held, 0, old);
        }
        held.map = map;
    }
    let changed = target ^ held.bits;
    if changed == 0 {
        return;
    }
    for (bit, action) in map.iter() {
        if changed & bit != 0 {
            let down = target & bit != 0;
            match action {
                Action::Mouse(b) => injector.button(*b, down),
                Action::Key(k) => injector.key(*k, down),
            }
        }
    }
    held.bits = target;
}

/// Suelta todo lo que siga pulsado en el SO.
fn release_all(injector: &mut dyn input::Injector, held: &mut OsHeld) {
    let map = held.map;
    apply_buttons_map(injector, held, 0, map);
}

#[cfg(windows)]
fn screen_width() -> f32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    if w > 0 {
        w as f32
    } else {
        1920.0
    }
}

#[cfg(windows)]
fn screen_aspect() -> f32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    if w > 0 && h > 0 {
        w as f32 / h as f32
    } else {
        16.0 / 9.0
    }
}

#[cfg(not(windows))]
fn screen_width() -> f32 {
    1920.0
}

#[cfg(not(windows))]
fn screen_aspect() -> f32 {
    16.0 / 9.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inyector de mentira: apunta cada tecla y botón con su flanco, en orden.
    #[derive(Default)]
    struct Fake {
        keys: Vec<(KeyCode, bool)>,
        mouse: Vec<(MouseButton, bool)>,
    }

    impl input::Injector for Fake {
        fn move_rel(&mut self, _dx: i32, _dy: i32) {}
        fn move_abs(&mut self, _nx: f32, _ny: f32) {}
        fn button(&mut self, btn: MouseButton, down: bool) {
            self.mouse.push((btn, down));
        }
        fn key(&mut self, key: KeyCode, down: bool) {
            self.keys.push((key, down));
        }
        fn wheel(&mut self, _delta: i32) {}
        fn name(&self) -> &'static str {
            "fake"
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Pulsa y suelta `bit`; devuelve las teclas que salieron.
    fn tap(bit: u32) -> Vec<(KeyCode, bool)> {
        let mut inj = Fake::default();
        let mut held = OsHeld::default();
        apply_buttons(&mut inj, &mut held, bit);
        apply_buttons(&mut inj, &mut held, 0);
        assert_eq!(held.bits, 0);
        inj.keys
    }

    #[test]
    fn la_cruceta_izquierda_y_derecha_son_atras_y_adelante_del_navegador() {
        assert_eq!(tap(codec::BTN_DPAD_LEFT), vec![(KeyCode::BrowserBack, true), (KeyCode::BrowserBack, false)]);
        assert_eq!(
            tap(codec::BTN_DPAD_RIGHT),
            vec![(KeyCode::BrowserForward, true), (KeyCode::BrowserForward, false)]
        );
    }

    #[test]
    fn la_cruceta_arriba_y_abajo_siguen_siendo_flechas() {
        assert_eq!(tap(codec::BTN_DPAD_UP), vec![(KeyCode::ArrowUp, true), (KeyCode::ArrowUp, false)]);
        assert_eq!(tap(codec::BTN_DPAD_DOWN), vec![(KeyCode::ArrowDown, true), (KeyCode::ArrowDown, false)]);
    }

    #[test]
    fn a_y_b_son_los_clics_y_prev_va_en_la_tabla() {
        let mut inj = Fake::default();
        let mut held = OsHeld::default();
        apply_buttons(&mut inj, &mut held, codec::BTN_A | codec::BTN_B);
        apply_buttons(&mut inj, &mut held, 0);
        assert_eq!(
            inj.mouse,
            vec![
                (MouseButton::Left, true),
                (MouseButton::Right, true),
                (MouseButton::Left, false),
                (MouseButton::Right, false)
            ]
        );
        assert_eq!(tap(codec::BTN_MEDIA_PREV), vec![(KeyCode::PrevTrack, true), (KeyCode::PrevTrack, false)]);
    }

    #[test]
    fn mantener_volumen_repite_tras_el_retardo_y_para_al_soltar() {
        let mut inj = Fake::default();
        let mut held = OsHeld::default();
        let mut rep = None;
        let t0 = Instant::now();
        apply_buttons(&mut inj, &mut held, codec::BTN_MEDIA_VOL_UP);
        for t in [0, 100, 200, 349] {
            repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(t));
        }
        assert_eq!(inj.keys, vec![(KeyCode::VolumeUp, true)], "antes del retardo, solo el flanco");
        for t in [350, 450, 550] {
            repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(t));
        }
        assert_eq!(inj.keys.len(), 7, "tres re-toques (soltar + pulsar)");
        assert_eq!(&inj.keys[1..3], &[(KeyCode::VolumeUp, false), (KeyCode::VolumeUp, true)]);
        assert_eq!(inj.keys.last(), Some(&(KeyCode::VolumeUp, true)), "tras un re-toque la tecla queda pulsada");
        // un tic antes de tiempo no hace nada
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(600));
        assert_eq!(inj.keys.len(), 7);
        apply_buttons(&mut inj, &mut held, 0);
        assert_eq!(inj.keys.last(), Some(&(KeyCode::VolumeUp, false)));
        let n = inj.keys.len();
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(2000));
        assert_eq!(inj.keys.len(), n, "soltado: no repite más");
        assert!(rep.is_none());
    }

    #[test]
    fn un_toque_corto_no_repite() {
        let mut inj = Fake::default();
        let mut held = OsHeld::default();
        let mut rep = None;
        let t0 = Instant::now();
        apply_buttons(&mut inj, &mut held, codec::BTN_MEDIA_VOL_DOWN);
        repeat_tick(&mut rep, held.bits, &mut inj, t0);
        apply_buttons(&mut inj, &mut held, 0);
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(80));
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(400));
        assert_eq!(inj.keys, vec![(KeyCode::VolumeDown, true), (KeyCode::VolumeDown, false)]);
    }

    #[test]
    fn mas_y_menos_repiten_como_las_teclas_de_volumen() {
        for (bit, key) in [(codec::BTN_PLUS, KeyCode::VolumeUp), (codec::BTN_MINUS, KeyCode::VolumeDown)] {
            let mut inj = Fake::default();
            let mut held = OsHeld::default();
            let mut rep = None;
            let t0 = Instant::now();
            apply_buttons(&mut inj, &mut held, bit);
            repeat_tick(&mut rep, held.bits, &mut inj, t0);
            repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(350));
            assert_eq!(inj.keys, vec![(key, true), (key, false), (key, true)]);
        }
        // lo que no es volumen no repite
        let mut inj = Fake::default();
        let mut held = OsHeld::default();
        let mut rep = None;
        let t0 = Instant::now();
        apply_buttons(&mut inj, &mut held, codec::BTN_DPAD_LEFT | codec::BTN_MEDIA_NEXT);
        repeat_tick(&mut rep, held.bits, &mut inj, t0);
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(1000));
        assert_eq!(inj.keys.len(), 2);
        assert!(rep.is_none());
    }

    #[test]
    fn soltar_todo_corta_la_repeticion() {
        let mut inj = Fake::default();
        let mut held = OsHeld::default();
        let mut rep = None;
        let t0 = Instant::now();
        apply_buttons(&mut inj, &mut held, codec::BTN_MEDIA_VOL_DOWN);
        repeat_tick(&mut rep, held.bits, &mut inj, t0);
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(350));
        release_all(&mut inj, &mut held);
        assert_eq!(held.bits, 0);
        assert_eq!(inj.keys.last(), Some(&(KeyCode::VolumeDown, false)));
        let n = inj.keys.len();
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(450));
        assert_eq!(inj.keys.len(), n);
        assert!(rep.is_none());
    }

    #[test]
    fn cambiar_de_tecla_reinicia_el_retardo() {
        let mut inj = Fake::default();
        let mut held = OsHeld::default();
        let mut rep = None;
        let t0 = Instant::now();
        apply_buttons(&mut inj, &mut held, codec::BTN_MEDIA_VOL_UP);
        repeat_tick(&mut rep, held.bits, &mut inj, t0);
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(350)); // primer re-toque de vol+
        apply_buttons(&mut inj, &mut held, codec::BTN_MEDIA_VOL_DOWN); // suelta vol+, pulsa vol−
        let n = inj.keys.len();
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(420));
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(700));
        assert_eq!(inj.keys.len(), n, "vol− aún en su retardo");
        repeat_tick(&mut rep, held.bits, &mut inj, t0 + ms(770));
        assert_eq!(&inj.keys[n..], &[(KeyCode::VolumeDown, false), (KeyCode::VolumeDown, true)]);
    }
}
