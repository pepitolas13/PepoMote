//! Hilo caliente: UDP INPUT → puntero (Jugador 1) y DSU (todos los slots).
//! También responde el broadcast de descubrimiento y mide RTT por jugador.

use super::codec::{self, Packet};
use super::Sessions;
use crate::dsu::{Dsu, MotionSample};
use crate::input::{self, KeyCode, MouseButton};
use crate::pairing::PairingInfo;
use crate::pointer::{PointerEngine, PointerOutput};
use crate::state::{Mode, SharedState};
use serde_json::json;
use std::collections::HashMap;
use std::net::UdpSocket;
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
    let socket = match UdpSocket::bind(("0.0.0.0", pairing.port)) {
        Ok(s) => s,
        Err(e) => {
            shared.lock().unwrap().last_error =
                Some(format!("No puedo escuchar en UDP {}: {e}", pairing.port));
            return;
        }
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(100)));

    let mut injector = match input::new_injector() {
        Ok(i) => i,
        Err(e) => {
            shared.lock().unwrap().last_error = Some(format!("Inyección de entrada: {e}"));
            return;
        }
    };

    let aspect = screen_aspect();
    let screen_w = screen_width();
    let mut engine = PointerEngine::new();
    let start = Instant::now();
    let now_us = |s: Instant| s.elapsed().as_micros() as u64;

    let mut buf = [0u8; 128];
    // Flancos de botones por sesión (cada jugador pulsa lo suyo)
    let mut prev_buttons: HashMap<u32, u32> = HashMap::new();
    // El motor de puntero pertenece al Jugador 1: se resetea si cambia su sesión
    let mut engine_session: Option<u32> = None;

    let mut win_start = Instant::now();
    let mut win_packets: u32 = 0;
    let mut win_first_t: Option<u64> = None;
    let mut win_last_t: u64 = 0;
    let mut last_ping = Instant::now();

    loop {
        // Ping de RTT a TODOS los jugadores
        if last_ping.elapsed() > Duration::from_millis(500) {
            last_ping = Instant::now();
            let targets: Vec<(u32, std::net::SocketAddr)> = sessions
                .lock()
                .unwrap()
                .values()
                .filter_map(|s| s.phone_udp.map(|a| (s.id, a)))
                .collect();
            for (id, addr) in targets {
                let _ = socket.send_to(&codec::build_ping(id, now_us(start)), addr);
            }
            // higiene: flancos de sesiones muertas fuera
            let alive = sessions.lock().unwrap();
            prev_buttons.retain(|id, _| alive.contains_key(id));
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
                let slot = {
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
                    sess.slot
                };

                win_packets += 1;
                if win_first_t.is_none() {
                    win_first_t = Some(p.t_sensor_us);
                }
                win_last_t = p.t_sensor_us;

                let (mode, config) = {
                    let mut s = shared.lock().unwrap();
                    if let Some(pl) = s.players[slot as usize].as_mut() {
                        pl.battery_pct = p.battery_pct;
                    }
                    (s.mode, s.config)
                };

                if mode == Mode::Dolphin {
                    // Todos los jugadores al DSU, cada uno en su slot, INLINE
                    if let Some(dsu) = &dsu {
                        dsu.push(
                            slot,
                            &MotionSample {
                                t_us: p.t_sensor_us,
                                accel_ms2: p.accel,
                                gyro_rads: p.gyro,
                                buttons: p.buttons,
                                battery_pct: p.battery_pct,
                                recenter_count: p.recenter_count,
                            },
                        );
                    }
                } else if slot == 0 {
                    // Modo puntero: el SO tiene UN cursor y es del Jugador 1
                    if engine_session != Some(p.session_id) {
                        engine_session = Some(p.session_id);
                        engine = PointerEngine::new();
                        prev_buttons.remove(&p.session_id);
                        injector.move_abs(0.5, 0.5);
                    }

                    engine.set_cursor_hint(injector.cursor_pos());
                    match engine.apply(&p, config.sens_deg, aspect, config.abs_mode, screen_w) {
                        PointerOutput::Abs { nx, ny } => injector.move_abs(nx, ny),
                        PointerOutput::Rel { dx, dy } => injector.move_rel(dx, dy),
                        PointerOutput::None => {}
                    }
                    if p.touch_scroll_dy != 0 {
                        injector.wheel(p.touch_scroll_dy as i32 * 4);
                    }
                    let prev = prev_buttons.entry(p.session_id).or_insert(0);
                    let changed = p.buttons ^ *prev;
                    if changed != 0 {
                        for (bit, action) in BUTTON_MAP.iter() {
                            if changed & bit != 0 {
                                let down = p.buttons & bit != 0;
                                match action {
                                    Action::Mouse(b) => injector.button(*b, down),
                                    Action::Key(k) => injector.key(*k, down),
                                }
                            }
                        }
                        if changed & (1 << 16) != 0 {
                            injector.key(KeyCode::PrevTrack, p.buttons & (1 << 16) != 0);
                        }
                    }
                    *prev = p.buttons;
                }
                // slot > 0 en modo puntero: se ignora (apunta el Jugador 1)
            }
            None => {}
        }
    }
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
