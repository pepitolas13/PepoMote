//! Hilo caliente: UDP INPUT → puntero (Jugador 1) y DSU (todos los slots).
//! También responde el broadcast de descubrimiento y mide RTT por jugador.

use super::codec::{self, Packet};
use super::Sessions;
use crate::dsu::{Dsu, DsuProfile, MotionSample};
use crate::input::{self, KeyCode, MouseButton};
use crate::pairing::PairingInfo;
use crate::pointer::{PointerEngine, PointerOutput};
use crate::state::{Mode, Role, SharedState};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Mapeo bit de botón → acción en modo puntero (PROTOCOL.md §4.2).
enum Action {
    Mouse(MouseButton),
    Key(KeyCode),
}

const BUTTON_MAP: [(u32, Action); 15] = [
    (1 << 0, Action::Mouse(MouseButton::Left)),   // A
    (1 << 1, Action::Mouse(MouseButton::Right)),  // B
    (1 << 2, Action::Key(KeyCode::ArrowUp)),
    (1 << 3, Action::Key(KeyCode::ArrowDown)),
    (1 << 4, Action::Key(KeyCode::ArrowLeft)),
    (1 << 5, Action::Key(KeyCode::ArrowRight)),
    (1 << 6, Action::Key(KeyCode::VolumeUp)),     // Plus
    (1 << 7, Action::Key(KeyCode::VolumeDown)),   // Minus
    (1 << 9, Action::Key(KeyCode::Enter)),        // Uno
    (1 << 10, Action::Key(KeyCode::Escape)),      // Dos
    (1 << 11, Action::Key(KeyCode::VolumeUp)),
    (1 << 12, Action::Key(KeyCode::VolumeDown)),
    (1 << 13, Action::Key(KeyCode::Mute)),
    (1 << 14, Action::Key(KeyCode::PlayPause)),
    (1 << 15, Action::Key(KeyCode::NextTrack)),
];
// bit 16 (prev) se trata aparte.

pub fn run(
    shared: SharedState,
    sessions: Sessions,
    pairing: PairingInfo,
    dsu: Option<Arc<Dsu>>,
) {
    let socket = match crate::ports::bind_udp(&shared, "0.0.0.0", pairing.port, "Móvil") {
        Ok(s) => s,
        Err(e) => {
            shared.lock().unwrap().last_error = Some(e);
            return;
        }
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(100)));

    // El inyector puede no nacer a la primera (/dev/uinput sin permiso, o el
    // compositor aún sin arrancar): el resto del receptor sigue vivo (Dolphin
    // no lo necesita) y se reintenta en el bucle — la auto-reparación da
    // permiso sin reiniciar la app.
    let mut injector: Option<Box<dyn input::Injector>> = None;
    let mut injector_retry = Instant::now() - Duration::from_secs(60);
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
    #[cfg(target_os = "linux")]
    let mut last_norm: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
    let mut engine = PointerEngine::new();
    let start = Instant::now();
    let now_us = |s: Instant| s.elapsed().as_micros() as u64;

    let mut buf = [0u8; 128];
    // Botones que el SO ve pulsados ahora mismo (solo inyecta el Jugador 1).
    // Si su sesión muere o el modo deja de ser puntero con A/B/tecla
    // sostenidos, hay que soltarlos: si no, el clic queda atascado en el SO.
    let mut held: u32 = 0;
    // El motor de puntero pertenece al Jugador 1: se resetea si cambia su
    // sesión; si es el mismo móvil que se reconecta, hereda el sesgo del
    // gyro que ya había aprendido
    let mut engine_session: Option<u32> = None;
    let mut engine_device: Option<String> = None;
    // Modo Wii U con «Mando Wii»: cada uno de esos móviles tiene su propio
    // motor de puntero (su IR va a Cemu por el touchpad DSU, no al SO)
    let mut ir_engines: HashMap<u32, IrPointer> = HashMap::new();
    // PEPOMOTE_RECORD=<archivo>: grabar la telemetría del Jugador 1 para
    // analizar un gesto real después (pointer/record.rs)
    let mut recorder = crate::pointer::record::Recorder::from_env();

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
                    #[cfg(target_os = "linux")]
                    {
                        last_norm = [-1.0, -1.0, -1.0, -1.0]; // forzar re-aplicar
                    }
                    crate::log_line!("Inyección: {}", i.name());
                    let mut s = shared.lock().unwrap();
                    s.injector = Some(i.name());
                    s.uinput_denied = false;
                    s.uinput_missing = false;
                    if s.last_error.as_deref().is_some_and(|e| e.starts_with("Inyección")) {
                        s.last_error = None;
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
                    let mut s = shared.lock().unwrap();
                    // solo si uinput se intentó de verdad y /dev/uinput falló:
                    // enciende «Reparar ahora» (nunca con el backend Wayland)
                    s.uinput_denied = e.uinput_denied;
                    s.uinput_missing = e.uinput_missing;
                    s.injector = None;
                    s.last_error = Some(format!("Inyección de entrada: {}", e.msg));
                }
            }
        }

        // Linux: aplicar el mapeo de pantalla que publica screens::watch (el
        // dispositivo absoluto cubre TODO el escritorio; esto lo dirige a la
        // pantalla elegida, o a todas). Solo se re-aplica cuando cambia.
        #[cfg(target_os = "linux")]
        {
            let pointing = shared.lock().unwrap().pointing;
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
        let pending: Vec<String> = std::mem::take(&mut shared.lock().unwrap().text_queue);
        if !pending.is_empty() {
            if let Some(inj) = injector.as_deref_mut() {
                for t in &pending {
                    inj.type_text(t);
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
                crate::log_line!("Inyección de entrada: conexión Wayland perdida; se vuelve a crear el inyector");
                injector = None;
                held = 0;
                injector_err_seen = None;
                let mut s = shared.lock().unwrap();
                s.injector = None;
                s.last_error = Some("Inyección de entrada: conexión Wayland perdida, reconectando…".into());
            }
            let targets: Vec<(u32, std::net::SocketAddr)> = sessions
                .lock()
                .unwrap()
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
                Some(id) => !sessions.lock().unwrap().contains_key(&id),
                None => false,
            };
            if j1_gone {
                engine_session = None;
                if let Some(inj) = injector.as_deref_mut() {
                    release_all(inj, &mut held);
                }
            }
            if !ir_engines.is_empty() {
                let alive = sessions.lock().unwrap();
                ir_engines.retain(|id, _| alive.contains_key(id));
            }
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
                let mut s = shared.lock().unwrap();
                s.pps = win_packets as f32 / secs;
                s.sensor_hz = hz;
            }
            win_start = Instant::now();
            win_packets = 0;
            win_first_t = None;
        }

        let (len, from) = match socket.recv_from(&mut buf) {
            Ok(r) => r,
            Err(_) => continue,
        };

        match codec::parse(&buf[..len]) {
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
                    .lock()
                    .unwrap()
                    .get(&session_id)
                    .map(|s| s.slot as usize);
                if let Some(slot) = slot {
                    if let Some(p) = shared.lock().unwrap().players[slot].as_mut() {
                        p.rtt_ms = Some(rtt_ms);
                    }
                }
            }
            Some(Packet::Input(p)) => {
                // Validar sesión, seq y aprender la dirección UDP del jugador
                let (slot, role) = {
                    let mut guard = sessions.lock().unwrap();
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
                    if let Some(r) = recorder.as_mut() {
                        r.write(&buf[..len]);
                    }
                }

                let (mode, sens_deg, abs_mode, pad_wii) = {
                    let mut s = shared.lock().unwrap();
                    let mut pad_wii = false;
                    if let Some(pl) = s.players[slot as usize].as_mut() {
                        pl.battery_pct = p.battery_pct;
                        pad_wii = pl.pad_wii;
                    }
                    (s.mode, s.config.sens_deg, s.config.abs_mode, pad_wii)
                };

                if mode.feeds_dsu() {
                    // Cambio a Dolphin/Cemu con algo sostenido: soltarlo en el SO
                    if let Some(inj) = injector.as_deref_mut() {
                        release_all(inj, &mut held);
                    }
                    // Todos los jugadores al DSU, cada uno en su slot, INLINE
                    if let Some(dsu) = &dsu {
                        let (profile, touch) = if mode == Mode::Cemu {
                            let touch = if role == Role::Wiimote && pad_wii {
                                // Mando Wii en Cemu: su puntero IR es el
                                // touchpad DSU (Cemu lo lee como posición)
                                ir_engines
                                    .entry(p.session_id)
                                    .or_default()
                                    .apply(&p, sens_deg, aspect, screen_w)
                            } else {
                                gamepad_touch(&p)
                            };
                            (DsuProfile::WiiU, touch)
                        } else {
                            (DsuProfile::Wii, None)
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
                                profile,
                            },
                        );
                    }
                } else if slot == 0 && role == Role::Wiimote {
                    // Modo puntero: el SO tiene UN cursor y es del Jugador 1
                    // (un Nunchuk nunca mueve el cursor)
                    let Some(inj) = injector.as_deref_mut() else {
                        continue; // sin inyector aún: se está reintentando
                    };
                    if engine_session != Some(p.session_id) {
                        engine_session = Some(p.session_id);
                        let device = sessions.lock().unwrap().get(&p.session_id).map(|s| s.device.clone());
                        let same_phone = device.is_some() && device == engine_device;
                        engine = if same_phone { PointerEngine::with_bias(engine.bias()) } else { PointerEngine::new() };
                        engine_device = device;
                        engine.set_cursor_bounds(inj.cursor_bounds());
                        release_all(inj, &mut held);
                        inj.move_abs(0.5, 0.5);
                    }

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
                    apply_buttons(inj, &mut held, p.buttons);
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

/// Puntero IR de un Mando Wii dentro de Cemu: el mismo motor absoluto que
/// mueve el cursor del PC, pero su salida va al touchpad DSU. Fuera de la
/// pantalla (con margen) el toque se apaga y el juego esconde el cursor.
struct IrPointer {
    engine: PointerEngine,
    last: Option<(u16, u16)>,
}

impl Default for IrPointer {
    fn default() -> Self {
        Self { engine: PointerEngine::new(), last: None }
    }
}

impl IrPointer {
    fn apply(&mut self, p: &codec::InputPacket, sens_deg: f32, aspect: f32, screen_w: f32) -> Option<(u16, u16)> {
        match self.engine.apply(p, sens_deg, aspect, true, screen_w, false) {
            PointerOutput::Abs { nx, ny } => {
                let on_screen = (-0.05..=1.05).contains(&nx) && (-0.05..=1.05).contains(&ny);
                self.last = on_screen.then(|| {
                    (
                        (nx.clamp(0.0, 1.0) * DSU_TOUCH_W).round() as u16,
                        (ny.clamp(0.0, 1.0) * DSU_TOUCH_H).round() as u16,
                    )
                });
            }
            // sin quaternion no hay apuntado absoluto que dar
            PointerOutput::Rel { .. } => self.last = None,
            // congelado en reposo: se mantiene la última posición
            PointerOutput::None => {}
        }
        self.last
    }
}

/// Inyecta los flancos entre lo que el SO ve pulsado (`held`) y `target`.
fn apply_buttons(injector: &mut dyn input::Injector, held: &mut u32, target: u32) {
    let changed = target ^ *held;
    if changed == 0 {
        return;
    }
    for (bit, action) in BUTTON_MAP.iter() {
        if changed & bit != 0 {
            let down = target & bit != 0;
            match action {
                Action::Mouse(b) => injector.button(*b, down),
                Action::Key(k) => injector.key(*k, down),
            }
        }
    }
    if changed & (1 << 16) != 0 {
        injector.key(KeyCode::PrevTrack, target & (1 << 16) != 0);
    }
    *held = target;
}

/// Suelta todo lo que siga pulsado en el SO.
fn release_all(injector: &mut dyn input::Injector, held: &mut u32) {
    apply_buttons(injector, held, 0);
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
