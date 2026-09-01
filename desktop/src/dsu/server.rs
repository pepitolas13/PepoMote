//! Servidor DSU (cemuhook) — verificado contra la spec comunitaria y el
//! código de Dolphin (protocol/DSU.md). Hasta 4 mandos, uno por slot.
//!
//! Este hilo SOLO atiende las peticiones de Dolphin (version/PortInfo/registro
//! PadData, ~1/s). El streaming de PadData sale inline del hilo de telemetría
//! (dsu::Dsu::push) para no añadir latencia.

use super::{mapping, MotionSample, SlotSamples};
use crate::net::MAX_PLAYERS;
use crate::state::SharedState;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DSU_PORT: u16 = 26760;
const PROTOCOL_VERSION: u16 = 1001;
const SERVER_ID: u32 = 0x50455030; // "0EPP"

const MSG_VERSION: u32 = 0x100000;
const MSG_PORT_INFO: u32 = 0x100001;
const MSG_PAD_DATA: u32 = 0x100002;

/// Clientes con registro caducable: Dolphin re-pide cada 1 s.
const CLIENT_TTL: Duration = Duration::from_secs(3);
/// Con más de esto sin muestras del móvil, ese slot se reporta desconectado.
const PAD_TTL: Duration = Duration::from_secs(1);

/// MAC estable por slot: "PMP1" + 0x00 + slot.
fn mac(slot: u8) -> [u8; 6] {
    [0x50, 0x4D, 0x50, 0x31, 0x00, slot]
}

pub fn run(
    shared: SharedState,
    socket: UdpSocket,
    clients: Arc<Mutex<HashMap<SocketAddr, Instant>>>,
    last: SlotSamples,
) {
    let _ = socket.set_read_timeout(Some(Duration::from_millis(250)));
    let mut buf = [0u8; 128];
    let mut last_sweep = Instant::now();

    loop {
        if let Ok((len, from)) = socket.recv_from(&mut buf) {
            if let Some((msg_type, payload)) = parse_request(&buf[..len]) {
                match msg_type {
                    MSG_VERSION => {
                        let mut out = Vec::with_capacity(22);
                        out.extend_from_slice(&MSG_VERSION.to_le_bytes());
                        out.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
                        let _ = socket.send_to(&finish(out), from);
                    }
                    MSG_PORT_INFO => {
                        let samples = last.lock().unwrap();
                        let count = payload
                            .first_chunk::<4>()
                            .map(|c| i32::from_le_bytes(*c))
                            .unwrap_or(0)
                            .clamp(0, 4) as usize;
                        for i in 0..count {
                            let slot = payload.get(4 + i).copied().unwrap_or(i as u8);
                            let info = slot_info(slot, &samples);
                            let mut out = Vec::with_capacity(32);
                            out.extend_from_slice(&MSG_PORT_INFO.to_le_bytes());
                            out.extend_from_slice(&info);
                            out.push(0);
                            let _ = socket.send_to(&finish(out), from);
                        }
                    }
                    MSG_PAD_DATA => {
                        clients.lock().unwrap().insert(from, Instant::now());
                    }
                    _ => {}
                }
            }
        }

        if last_sweep.elapsed() > Duration::from_secs(1) {
            last_sweep = Instant::now();
            let mut c = clients.lock().unwrap();
            c.retain(|_, t| t.elapsed() < CLIENT_TTL);
            shared.lock().unwrap().dsu_clients = c.len();
        }
    }
}

/// Los 11 bytes de info de mando para PortInfo, con el estado real del slot.
fn slot_info(slot: u8, samples: &[Option<(MotionSample, Instant)>; MAX_PLAYERS]) -> [u8; 11] {
    let fresh = (slot as usize) < MAX_PLAYERS
        && matches!(&samples[slot as usize], Some((_, t)) if t.elapsed() < PAD_TTL);
    let battery = if fresh {
        samples[slot as usize]
            .as_ref()
            .map(|(s, _)| mapping::battery_to_dsu(s.battery_pct))
            .unwrap_or(0)
    } else {
        0
    };
    pad_info(slot, fresh, battery)
}

/// Los 11 bytes de info de mando (compartidos por PortInfo y PadData).
fn pad_info(slot: u8, connected: bool, battery: u8) -> [u8; 11] {
    let mut out = [0u8; 11];
    out[0] = slot;
    if connected {
        out[1] = 2; // conectado
        out[2] = 2; // gyro completo
        out[3] = 2; // "bluetooth"
        out[4..10].copy_from_slice(&mac(slot));
        out[10] = battery;
    }
    out
}

/// Header DSUS + CRC sobre el paquete completo.
fn finish(payload: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + payload.len());
    out.extend_from_slice(b"DSUS");
    out.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // CRC a cero
    out.extend_from_slice(&SERVER_ID.to_le_bytes());
    out.extend_from_slice(&payload);
    let crc = crc32fast::hash(&out);
    out[8..12].copy_from_slice(&crc.to_le_bytes());
    out
}

/// Valida header DSUC y devuelve (tipo, payload tras el tipo).
fn parse_request(buf: &[u8]) -> Option<(u32, &[u8])> {
    if buf.len() < 20 || &buf[0..4] != b"DSUC" {
        return None;
    }
    let version = u16::from_le_bytes(buf[4..6].try_into().ok()?);
    if version != PROTOCOL_VERSION {
        return None;
    }
    let msg_type = u32::from_le_bytes(buf[16..20].try_into().ok()?);
    Some((msg_type, &buf[20..]))
}

/// PadData de 100 bytes (spec en protocol/DSU.md), para el slot dado.
pub fn pad_data_packet(
    slot: u8,
    sample: &MotionSample,
    touch_pressed: bool,
    counter: u32,
) -> Vec<u8> {
    let (accel, gyro) = mapping::to_dsu(sample.accel_ms2, sample.gyro_rads);
    let (b1, b2, ps, dpad, face) = mapping::buttons_to_dsu(sample.buttons);

    let mut p = Vec::with_capacity(84);
    p.extend_from_slice(&MSG_PAD_DATA.to_le_bytes());
    p.extend_from_slice(&pad_info(
        slot,
        true,
        mapping::battery_to_dsu(sample.battery_pct),
    ));
    p.push(1); // connected
    p.extend_from_slice(&counter.to_le_bytes());
    p.push(b1);
    p.push(b2);
    p.push(ps);
    p.push(if touch_pressed { 0xFF } else { 0 }); // botón Touch = recentrado
    p.extend_from_slice(&[128, 128, 128, 128]); // sticks LX LY RX RY neutros
    p.extend_from_slice(&dpad); // analógico L D R U ("Pad W/S/E/N")
    p.extend_from_slice(&face); // analógico square cross circle triangle
    p.extend_from_slice(&[0, 0, 0, 0]); // analógico R1 L1 R2 L2
    p.extend_from_slice(&[0u8; 6]); // touch 1 inactivo
    p.extend_from_slice(&[0u8; 6]); // touch 2 inactivo
    p.extend_from_slice(&sample.t_us.to_le_bytes()); // timestamp del SENSOR
    for v in accel {
        p.extend_from_slice(&v.to_le_bytes());
    }
    for v in gyro {
        p.extend_from_slice(&v.to_le_bytes());
    }

    let out = finish(p);
    debug_assert_eq!(out.len(), 100);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MotionSample {
        MotionSample {
            t_us: 123_456_789,
            accel_ms2: [0.0, 0.0, 9.80665],
            gyro_rads: [1.0, 0.0, 0.0],
            buttons: 1, // A
            battery_pct: 100,
            recenter_count: 0,
        }
    }

    #[test]
    fn pad_data_mide_100_bytes_y_crc_valido() {
        let s = sample();
        let out = pad_data_packet(0, &s, false, 7);
        assert_eq!(out.len(), 100);
        assert_eq!(&out[0..4], b"DSUS");
        assert_eq!(u16::from_le_bytes(out[4..6].try_into().unwrap()), 1001);
        assert_eq!(u16::from_le_bytes(out[6..8].try_into().unwrap()), 84);
        let mut copy = out.clone();
        let crc_in = u32::from_le_bytes(copy[8..12].try_into().unwrap());
        copy[8..12].copy_from_slice(&[0; 4]);
        assert_eq!(crc32fast::hash(&copy), crc_in);
    }

    #[test]
    fn pad_data_campos_clave() {
        let s = sample();
        let out = pad_data_packet(0, &s, true, 42);
        assert_eq!(u32::from_le_bytes(out[16..20].try_into().unwrap()), MSG_PAD_DATA);
        assert_eq!(out[20], 0); // slot
        assert_eq!(out[21], 2); // conectado
        assert_eq!(out[22], 2); // gyro completo
        assert_eq!(out[31], 1);
        assert_eq!(u32::from_le_bytes(out[32..36].try_into().unwrap()), 42);
        assert_eq!(out[37] & (1 << 6), 1 << 6); // A → Cross (bitmask)
        assert_eq!(out[39], 0xFF); // pulso de recentrado en Touch
        assert_eq!(&out[40..44], &[128, 128, 128, 128]);
        assert_eq!(out[49], 0xFF); // Cross analógico
        assert_eq!(out[48], 0);
        assert_eq!(
            u64::from_le_bytes(out[68..76].try_into().unwrap()),
            123_456_789
        );
        let ay = f32::from_le_bytes(out[80..84].try_into().unwrap());
        assert!((ay + 1.0).abs() < 1e-5, "ay={ay}");
        let pitch = f32::from_le_bytes(out[88..92].try_into().unwrap());
        assert!((pitch - 57.29578).abs() < 1e-3, "pitch={pitch}");
    }

    #[test]
    fn slots_distintos_llevan_mac_y_slot_distintos() {
        let s = sample();
        let p0 = pad_data_packet(0, &s, false, 1);
        let p1 = pad_data_packet(1, &s, false, 2);
        assert_eq!(p0[20], 0);
        assert_eq!(p1[20], 1);
        // MAC en los bytes 24..30 del paquete (pad_info offset 4..10 + header 20)
        assert_eq!(&p0[24..30], &[0x50, 0x4D, 0x50, 0x31, 0x00, 0x00]);
        assert_eq!(&p1[24..30], &[0x50, 0x4D, 0x50, 0x31, 0x00, 0x01]);
    }

    #[test]
    fn portinfo_por_slot_vivo_y_muerto() {
        let s = sample();
        let mut samples: [Option<(MotionSample, Instant)>; MAX_PLAYERS] = [None; MAX_PLAYERS];
        samples[1] = Some((s, Instant::now()));
        let dead = slot_info(0, &samples);
        let live = slot_info(1, &samples);
        assert_eq!(dead[1], 0, "slot 0 sin muestras = desconectado");
        assert_eq!(live[1], 2, "slot 1 con muestra fresca = conectado");
        assert_eq!(live[0], 1);
        assert_eq!(&live[4..10], &[0x50, 0x4D, 0x50, 0x31, 0x00, 0x01]);
    }

    #[test]
    fn version_y_portinfo_bien_formados() {
        let mut v = Vec::new();
        v.extend_from_slice(&MSG_VERSION.to_le_bytes());
        v.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        let out = finish(v);
        assert_eq!(out.len(), 22);

        let mut p = Vec::new();
        p.extend_from_slice(&MSG_PORT_INFO.to_le_bytes());
        p.extend_from_slice(&pad_info(0, true, 5));
        p.push(0);
        let out = finish(p);
        assert_eq!(out.len(), 32);
    }
}
