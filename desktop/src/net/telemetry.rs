//! Hilo caliente: UDP INPUT → puntero. También responde el broadcast de
//! descubrimiento y mide RTT con PING/PONG (PROTOCOL.md §4).

use super::codec::{self, Packet};
use super::SharedSession;
use crate::input::{self, MouseButton};
use crate::pairing::PairingInfo;
use crate::pointer::PointerEngine;
use crate::state::{Mode, SharedState};
use serde_json::json;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

pub fn run(shared: SharedState, session: SharedSession, pairing: PairingInfo) {
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

    let mut engine = PointerEngine::new();
    let start = Instant::now();
    let now_us = |s: Instant| s.elapsed().as_micros() as u64;

    let mut buf = [0u8; 128];
    let mut prev_buttons: u32 = 0;
    // Estadísticas por ventana de 1 s
    let mut win_start = Instant::now();
    let mut win_packets: u32 = 0;
    let mut win_first_t: Option<u64> = None;
    let mut win_last_t: u64 = 0;
    let mut last_ping = Instant::now();

    loop {
        // Ping periódico al móvil para el RTT del HUD
        if last_ping.elapsed() > Duration::from_millis(500) {
            last_ping = Instant::now();
            let target = session
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|s| s.phone_udp.map(|a| (s.id, a)));
            if let Some((id, addr)) = target {
                let _ = socket.send_to(&codec::build_ping(id, now_us(start)), addr);
            }
        }

        // Ventana de estadísticas
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
            Err(_) => continue, // timeout u otro: seguimos
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
            Some(Packet::Pong { t_us, .. }) => {
                let rtt_ms = (now_us(start).saturating_sub(t_us)) as f32 / 1000.0;
                shared.lock().unwrap().rtt_ms = Some(rtt_ms);
            }
            Some(Packet::Input(p)) => {
                // Validar sesión y seq
                {
                    let mut guard = session.lock().unwrap();
                    let Some(sess) = guard.as_mut() else { continue };
                    if sess.id != p.session_id {
                        continue;
                    }
                    if let Some(last) = sess.last_seq {
                        // descarta duplicados/desordenados (ventana de wrap)
                        if p.seq.wrapping_sub(last) == 0 || p.seq.wrapping_sub(last) > u32::MAX / 2
                        {
                            continue;
                        }
                    }
                    sess.last_seq = Some(p.seq);
                    sess.phone_udp = Some(from);
                }

                win_packets += 1;
                if win_first_t.is_none() {
                    win_first_t = Some(p.t_sensor_us);
                }
                win_last_t = p.t_sensor_us;
                shared.lock().unwrap().battery_pct = p.battery_pct;

                let mode = shared.lock().unwrap().mode;
                if mode == Mode::Pointer {
                    // h1: movimiento relativo con gyro crudo
                    let (dx, dy) = engine.apply(p.gyro, p.t_sensor_us);
                    if dx != 0 || dy != 0 {
                        injector.move_rel(dx, dy);
                    }
                    if p.touch_scroll_dy != 0 {
                        injector.wheel(p.touch_scroll_dy as i32 * 4);
                    }
                    // Flancos de botones
                    let changed = p.buttons ^ prev_buttons;
                    if changed & codec::BTN_A != 0 {
                        injector.button(MouseButton::Left, p.buttons & codec::BTN_A != 0);
                    }
                    if changed & codec::BTN_B != 0 {
                        injector.button(MouseButton::Right, p.buttons & codec::BTN_B != 0);
                    }
                }
                prev_buttons = p.buttons;
            }
            None => {}
        }
    }
}
