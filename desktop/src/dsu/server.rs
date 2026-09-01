//! Servidor DSU (cemuhook) en 127.0.0.1:26760 — verificado contra la spec
//! comunitaria y el cliente de Dolphin (protocol/DSU.md). Un mando, slot 0.

use super::mapping;
use super::MotionSample;
use crate::state::SharedState;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

const DSU_PORT: u16 = 26760;
const PROTOCOL_VERSION: u16 = 1001;
const SERVER_ID: u32 = 0x50455030; // "0EPP"
const MAC: [u8; 6] = [0x50, 0x4D, 0x50, 0x31, 0x00, 0x01]; // "PMP1" + slot

const MSG_VERSION: u32 = 0x100000;
const MSG_PORT_INFO: u32 = 0x100001;
const MSG_PAD_DATA: u32 = 0x100002;

/// Clientes con registro caducable: Dolphin re-pide cada 1 s.
const CLIENT_TTL: Duration = Duration::from_secs(3);
/// Con más de esto sin muestras del móvil, el pad se reporta desconectado.
const PAD_TTL: Duration = Duration::from_secs(1);
/// Duración del pulso del botón Touch al recentrar (para IMUPointer/Recenter).
const RECENTER_PULSE: Duration = Duration::from_millis(150);

pub fn run(shared: SharedState, rx: Receiver<MotionSample>) {
    let socket = match UdpSocket::bind(("127.0.0.1", DSU_PORT)) {
        Ok(s) => s,
        Err(e) => {
            shared.lock().unwrap().last_error =
                Some(format!("DSU: no puedo escuchar en {DSU_PORT}: {e}"));
            return;
        }
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(4)));

    let mut clients: HashMap<SocketAddr, Instant> = HashMap::new();
    let mut packet_counter: u32 = 0;
    let mut buf = [0u8; 128];
    let mut last_sample: Option<(MotionSample, Instant)> = None;
    let mut last_recenter: Option<u8> = None;
    let mut pulse_until = Instant::now();
    let mut last_sweep = Instant::now();

    loop {
        // Peticiones entrantes
        while let Ok((len, from)) = socket.recv_from(&mut buf) {
            if let Some((msg_type, payload)) = parse_request(&buf[..len]) {
                match msg_type {
                    MSG_VERSION => {
                        let mut out = Vec::with_capacity(22);
                        out.extend_from_slice(&MSG_VERSION.to_le_bytes());
                        out.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
                        let _ = socket.send_to(&finish(out), from);
                    }
                    MSG_PORT_INFO => {
                        let connected = pad_connected(&last_sample);
                        let count = payload
                            .first_chunk::<4>()
                            .map(|c| i32::from_le_bytes(*c))
                            .unwrap_or(0)
                            .clamp(0, 4) as usize;
                        for i in 0..count {
                            let slot = payload.get(4 + i).copied().unwrap_or(i as u8);
                            let mut out = Vec::with_capacity(32);
                            out.extend_from_slice(&MSG_PORT_INFO.to_le_bytes());
                            out.extend_from_slice(&pad_info(slot, connected, &last_sample));
                            out.push(0);
                            let _ = socket.send_to(&finish(out), from);
                        }
                    }
                    MSG_PAD_DATA => {
                        clients.insert(from, Instant::now());
                    }
                    _ => {}
                }
            }
        }

        // Caducar clientes
        if last_sweep.elapsed() > Duration::from_secs(1) {
            last_sweep = Instant::now();
            clients.retain(|_, t| t.elapsed() < CLIENT_TTL);
            shared.lock().unwrap().dsu_clients = clients.len();
        }

        // Muestras del móvil → PadData a los clientes registrados
        while let Ok(sample) = rx.try_recv() {
            if last_recenter != Some(sample.recenter_count) {
                if last_recenter.is_some() {
                    pulse_until = Instant::now() + RECENTER_PULSE;
                }
                last_recenter = Some(sample.recenter_count);
            }
            let touch_pressed = Instant::now() < pulse_until;
            last_sample = Some((sample, Instant::now()));

            if !clients.is_empty() {
                packet_counter = packet_counter.wrapping_add(1);
                let packet = pad_data_packet(&sample, touch_pressed, packet_counter, &last_sample);
                for addr in clients.keys() {
                    let _ = socket.send_to(&packet, addr);
                }
            }
        }
    }
}

fn pad_connected(last: &Option<(MotionSample, Instant)>) -> bool {
    matches!(last, Some((_, t)) if t.elapsed() < PAD_TTL)
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

/// Los 11 bytes de info de mando (compartidos por PortInfo y PadData).
fn pad_info(slot: u8, connected: bool, last: &Option<(MotionSample, Instant)>) -> [u8; 11] {
    let mut out = [0u8; 11];
    out[0] = slot;
    if slot == 0 && connected {
        out[1] = 2; // conectado
        out[2] = 2; // gyro completo
        out[3] = 2; // "bluetooth"
        out[4..10].copy_from_slice(&MAC);
        out[10] = last
            .as_ref()
            .map(|(s, _)| mapping::battery_to_dsu(s.battery_pct))
            .unwrap_or(0);
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

/// PadData de 100 bytes (spec en protocol/DSU.md).
fn pad_data_packet(
    sample: &MotionSample,
    touch_pressed: bool,
    counter: u32,
    last: &Option<(MotionSample, Instant)>,
) -> Vec<u8> {
    let (accel, gyro) = mapping::to_dsu(sample.accel_ms2, sample.gyro_rads);
    let (b1, b2, ps, dpad, face) = mapping::buttons_to_dsu(sample.buttons);

    let mut p = Vec::with_capacity(84);
    p.extend_from_slice(&MSG_PAD_DATA.to_le_bytes());
    p.extend_from_slice(&pad_info(0, true, last));
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
        let last = Some((s, Instant::now()));
        let out = pad_data_packet(&s, false, 7, &last);
        assert_eq!(out.len(), 100);
        assert_eq!(&out[0..4], b"DSUS");
        assert_eq!(u16::from_le_bytes(out[4..6].try_into().unwrap()), 1001);
        assert_eq!(u16::from_le_bytes(out[6..8].try_into().unwrap()), 84);
        // CRC: recomputar con el campo a cero
        let mut copy = out.clone();
        let crc_in = u32::from_le_bytes(copy[8..12].try_into().unwrap());
        copy[8..12].copy_from_slice(&[0; 4]);
        assert_eq!(crc32fast::hash(&copy), crc_in);
    }

    #[test]
    fn pad_data_campos_clave() {
        let s = sample();
        let last = Some((s, Instant::now()));
        let out = pad_data_packet(&s, true, 42, &last);
        // tipo
        assert_eq!(u32::from_le_bytes(out[16..20].try_into().unwrap()), MSG_PAD_DATA);
        // slot 0 conectado, modelo 2
        assert_eq!(out[20], 0);
        assert_eq!(out[21], 2);
        assert_eq!(out[22], 2);
        // connected + contador
        assert_eq!(out[31], 1);
        assert_eq!(u32::from_le_bytes(out[32..36].try_into().unwrap()), 42);
        // A → Cross en buttons2 (bit6)
        assert_eq!(out[37] & (1 << 6), 1 << 6);
        // pulso de recentrado en el botón Touch
        assert_eq!(out[39], 0xFF);
        // sticks neutros
        assert_eq!(&out[40..44], &[128, 128, 128, 128]);
        // A pulsado → byte analógico de Cross (offset 49: dpad 44-47, square 48)
        assert_eq!(out[49], 0xFF);
        assert_eq!(out[48], 0); // square no
        // offsets: touch1 56..62, touch2 62..68, timestamp 68..76,
        // accel 76..88, gyro 88..100
        assert_eq!(
            u64::from_le_bytes(out[68..76].try_into().unwrap()),
            123_456_789
        );
        // accel plano: DSU Y = -1 g
        let ay = f32::from_le_bytes(out[80..84].try_into().unwrap());
        assert!((ay + 1.0).abs() < 1e-5, "ay={ay}");
        // gyro pitch = +gx en °/s (convención del código de Dolphin)
        let pitch = f32::from_le_bytes(out[88..92].try_into().unwrap());
        assert!((pitch - 57.29578).abs() < 1e-3, "pitch={pitch}");
    }

    #[test]
    fn version_y_portinfo_bien_formados() {
        let mut v = Vec::new();
        v.extend_from_slice(&MSG_VERSION.to_le_bytes());
        v.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        let out = finish(v);
        assert_eq!(out.len(), 22);

        let s = sample();
        let last = Some((s, Instant::now()));
        let mut p = Vec::new();
        p.extend_from_slice(&MSG_PORT_INFO.to_le_bytes());
        p.extend_from_slice(&pad_info(0, true, &last));
        p.push(0);
        let out = finish(p);
        assert_eq!(out.len(), 32);
    }
}
