//! Codec del protocolo PMP v1. Fuente de verdad: protocol/PROTOCOL.md.
//! Lo comparten el receptor (parse) y el emisor Linux móvil (build); Android
//! tiene su gemelo en PmpCodec.kt. Los tests de paridad usan los vectores
//! dorados de protocol/vectors/: los tres codecs decodifican/generan los
//! mismos bytes.
#![forbid(unsafe_code)]

pub const MAGIC: u32 = 0x3150_4D50; // "PMP1" en LE
pub const TYPE_INPUT: u8 = 0x01;
pub const TYPE_PING: u8 = 0x02;
pub const TYPE_PONG: u8 = 0x03;
pub const INPUT_LEN: usize = 72;
pub const PING_LEN: usize = 20;

/// Puerto por defecto del receptor (TCP control y UDP telemetría).
pub const DEFAULT_PORT: u16 = 26761;

pub const DISCOVER: &[u8] = b"PMPDISCOVER1";
pub const HERE_PREFIX: &[u8] = b"PMPHERE1 ";

/// flags bit0: el quaternion es válido (el móvil tiene rotation vector / fusión)
pub const FLAG_QUAT_VALID: u8 = 1 << 0;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InputPacket {
    pub flags: u8,
    pub session_id: u32,
    pub seq: u32,
    pub t_sensor_us: u64,
    pub quat: [f32; 4], // w, x, y, z
    pub gyro: [f32; 3], // rad/s
    pub accel: [f32; 3], // m/s²
    pub buttons: u32,
    pub recenter_count: u8,
    pub battery_pct: u8,
    pub touch_scroll_dy: i16,
}

// Bits de botones (PROTOCOL.md 4.2)
pub const BTN_A: u32 = 1 << 0;
pub const BTN_B: u32 = 1 << 1;
pub const BTN_DPAD_UP: u32 = 1 << 2;
pub const BTN_DPAD_DOWN: u32 = 1 << 3;
pub const BTN_DPAD_LEFT: u32 = 1 << 4;
pub const BTN_DPAD_RIGHT: u32 = 1 << 5;
pub const BTN_PLUS: u32 = 1 << 6;
pub const BTN_MINUS: u32 = 1 << 7;
pub const BTN_HOME: u32 = 1 << 8;
pub const BTN_ONE: u32 = 1 << 9;
pub const BTN_TWO: u32 = 1 << 10;
pub const BTN_MEDIA_VOL_UP: u32 = 1 << 11;
pub const BTN_MEDIA_VOL_DOWN: u32 = 1 << 12;
pub const BTN_MEDIA_MUTE: u32 = 1 << 13;
pub const BTN_MEDIA_PLAY_PAUSE: u32 = 1 << 14;
pub const BTN_MEDIA_NEXT: u32 = 1 << 15;
pub const BTN_MEDIA_PREV: u32 = 1 << 16;

#[derive(Debug, PartialEq)]
pub enum Packet {
    Input(InputPacket),
    Ping { session_id: u32, t_us: u64 },
    Pong { session_id: u32, t_us: u64 },
    Discover,
}

pub fn parse(buf: &[u8]) -> Option<Packet> {
    if buf == DISCOVER {
        return Some(Packet::Discover);
    }
    if buf.len() < 12 || u32::from_le_bytes(buf[0..4].try_into().ok()?) != MAGIC {
        return None;
    }
    let ty = buf[4];
    match ty {
        TYPE_INPUT if buf.len() == INPUT_LEN => {
            let f32_at = |off: usize| f32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
            Some(Packet::Input(InputPacket {
                flags: buf[5],
                session_id: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
                seq: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
                t_sensor_us: u64::from_le_bytes(buf[16..24].try_into().unwrap()),
                quat: [f32_at(24), f32_at(28), f32_at(32), f32_at(36)],
                gyro: [f32_at(40), f32_at(44), f32_at(48)],
                accel: [f32_at(52), f32_at(56), f32_at(60)],
                buttons: u32::from_le_bytes(buf[64..68].try_into().unwrap()),
                recenter_count: buf[68],
                battery_pct: buf[69],
                touch_scroll_dy: i16::from_le_bytes(buf[70..72].try_into().unwrap()),
            }))
        }
        TYPE_PING | TYPE_PONG if buf.len() == PING_LEN => {
            let session_id = u32::from_le_bytes(buf[8..12].try_into().unwrap());
            let t_us = u64::from_le_bytes(buf[12..20].try_into().unwrap());
            Some(if ty == TYPE_PING {
                Packet::Ping { session_id, t_us }
            } else {
                Packet::Pong { session_id, t_us }
            })
        }
        _ => None,
    }
}

/// INPUT de 72 bytes (PROTOCOL.md §4.1).
pub fn build_input(p: &InputPacket) -> [u8; INPUT_LEN] {
    let mut out = [0u8; INPUT_LEN];
    out[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    out[4] = TYPE_INPUT;
    out[5] = p.flags;
    // 6..8 reservado = 0
    out[8..12].copy_from_slice(&p.session_id.to_le_bytes());
    out[12..16].copy_from_slice(&p.seq.to_le_bytes());
    out[16..24].copy_from_slice(&p.t_sensor_us.to_le_bytes());
    for (i, v) in p.quat.iter().enumerate() {
        out[24 + i * 4..28 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    for (i, v) in p.gyro.iter().enumerate() {
        out[40 + i * 4..44 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    for (i, v) in p.accel.iter().enumerate() {
        out[52 + i * 4..56 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    out[64..68].copy_from_slice(&p.buttons.to_le_bytes());
    out[68] = p.recenter_count;
    out[69] = p.battery_pct;
    out[70..72].copy_from_slice(&p.touch_scroll_dy.to_le_bytes());
    out
}

fn header(ty: u8, session_id: u32, out: &mut Vec<u8>) {
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.push(ty);
    out.push(0);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&session_id.to_le_bytes());
}

pub fn build_ping(session_id: u32, t_us: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(PING_LEN);
    header(TYPE_PING, session_id, &mut out);
    out.extend_from_slice(&t_us.to_le_bytes());
    out
}

pub fn build_pong(session_id: u32, t_us: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(PING_LEN);
    header(TYPE_PONG, session_id, &mut out);
    out.extend_from_slice(&t_us.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_hex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn vector(name: &str) -> Vec<u8> {
        match name {
            "input_neutral" => from_hex(include_str!("../../protocol/vectors/input_neutral.hex")),
            "input_motion" => from_hex(include_str!("../../protocol/vectors/input_motion.hex")),
            "input_buttons_all" => {
                from_hex(include_str!("../../protocol/vectors/input_buttons_all.hex"))
            }
            "ping" => from_hex(include_str!("../../protocol/vectors/ping.hex")),
            "pong" => from_hex(include_str!("../../protocol/vectors/pong.hex")),
            _ => unreachable!(),
        }
    }

    #[test]
    fn vector_input_neutral() {
        let buf = vector("input_neutral");
        assert_eq!(buf.len(), INPUT_LEN);
        let Packet::Input(p) = parse(&buf).unwrap() else {
            panic!("no es INPUT")
        };
        assert_eq!(p.flags, 0);
        assert_eq!(p.session_id, 0xAABBCCDD);
        assert_eq!(p.seq, 7);
        assert_eq!(p.t_sensor_us, 1_000_000);
        assert_eq!(p.quat, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(p.gyro, [0.0; 3]);
        assert_eq!(p.accel, [0.0; 3]);
        assert_eq!(p.buttons, 0);
        assert_eq!(p.recenter_count, 0);
        assert_eq!(p.battery_pct, 100);
        assert_eq!(p.touch_scroll_dy, 0);
    }

    #[test]
    fn vector_input_motion() {
        let Packet::Input(p) = parse(&vector("input_motion")).unwrap() else {
            panic!("no es INPUT")
        };
        assert_eq!(p.seq, 8);
        assert_eq!(p.t_sensor_us, 2_000_000);
        assert_eq!(p.quat, [0.5, -0.5, 0.5, -0.5]);
        assert_eq!(p.gyro, [1.0, -1.0, 0.5]);
        assert_eq!(p.accel, [-1.0, 2.0, -0.5]);
        assert_eq!(p.buttons, 0x41); // A + Plus
        assert_eq!(p.recenter_count, 1);
        assert_eq!(p.battery_pct, 50);
        assert_eq!(p.touch_scroll_dy, -12);
    }

    #[test]
    fn vector_input_buttons_all() {
        let Packet::Input(p) = parse(&vector("input_buttons_all")).unwrap() else {
            panic!("no es INPUT")
        };
        assert_eq!(p.seq, 9);
        assert_eq!(p.t_sensor_us, 3_000_000);
        assert_eq!(p.buttons, 0x0001_FFFF);
        assert_eq!(p.recenter_count, 3);
        assert_eq!(p.battery_pct, 87);
        assert_eq!(p.touch_scroll_dy, -120);
    }

    #[test]
    fn build_reproduce_los_vectores_byte_a_byte() {
        // parse → build debe devolver EXACTAMENTE el vector: el emisor Linux
        // genera lo mismo que el Kotlin de Android
        for name in ["input_neutral", "input_motion", "input_buttons_all"] {
            let buf = vector(name);
            let Packet::Input(p) = parse(&buf).unwrap() else {
                panic!("no es INPUT")
            };
            assert_eq!(build_input(&p).as_slice(), buf.as_slice(), "{name}");
        }
    }

    #[test]
    fn vector_ping_pong() {
        let ping = vector("ping");
        assert_eq!(
            parse(&ping).unwrap(),
            Packet::Ping {
                session_id: 0xAABBCCDD,
                t_us: 0x0102030405060708
            }
        );
        assert_eq!(build_ping(0xAABBCCDD, 0x0102030405060708), ping);

        let pong = vector("pong");
        assert_eq!(
            parse(&pong).unwrap(),
            Packet::Pong {
                session_id: 0xAABBCCDD,
                t_us: 0x0102030405060708
            }
        );
        assert_eq!(build_pong(0xAABBCCDD, 0x0102030405060708), pong);
    }

    #[test]
    fn discover() {
        assert_eq!(parse(b"PMPDISCOVER1").unwrap(), Packet::Discover);
    }

    #[test]
    fn basura_no_parsea() {
        assert_eq!(parse(b""), None);
        assert_eq!(parse(&[0u8; 72]), None);
        let mut short = build_input(&InputPacket::default()).to_vec();
        short.pop();
        assert_eq!(parse(&short), None);
    }
}
