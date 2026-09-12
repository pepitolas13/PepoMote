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
/// INPUT con el bloque de extensión Wii U (FLAG_EXT): stick derecho y
/// pantalla táctil en los bytes 72-79.
pub const INPUT_EXT_LEN: usize = 80;
pub const PING_LEN: usize = 20;

/// Puerto por defecto del receptor (TCP control y UDP telemetría).
pub const DEFAULT_PORT: u16 = 26761;

pub const DISCOVER: &[u8] = b"PMPDISCOVER1";
pub const HERE_PREFIX: &[u8] = b"PMPHERE1 ";

/// flags bit0: el quaternion es válido (el móvil tiene rotation vector / fusión)
pub const FLAG_QUAT_VALID: u8 = 1 << 0;
/// flags bit1: los bytes 6-7 llevan el stick (Nunchuk o stick izquierdo del GamePad)
pub const FLAG_STICK_VALID: u8 = 1 << 1;
/// flags bit2: el paquete mide 80 bytes y los bytes 72-79 llevan el bloque
/// Wii U (stick derecho + táctil). Solo se emite en modo `cemu`, que un
/// receptor antiguo nunca confirma: así nunca recibe 80 bytes.
pub const FLAG_EXT: u8 = 1 << 2;
/// flags bit3: hay un dedo en la pantalla táctil del GamePad (touch_x/y válidos).
pub const FLAG_TOUCH: u8 = 1 << 3;

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
    /// Stick (bytes 6-7, antes reservados): +X derecha, +Y arriba,
    /// −127..127. Del Nunchuk, o el izquierdo del GamePad. Solo con
    /// FLAG_STICK_VALID; los emisores Wiimote mandan 0,0.
    pub stick_x: i8,
    pub stick_y: i8,
    /// Bloque Wii U (bytes 72-79, solo con FLAG_EXT): stick derecho del
    /// GamePad, misma convención que el izquierdo.
    pub stick_rx: i8,
    pub stick_ry: i8,
    /// Pantalla táctil del GamePad (solo con FLAG_EXT y FLAG_TOUCH): fracción
    /// de la pantalla en 0..65535, origen arriba-izquierda.
    pub touch_x: u16,
    pub touch_y: u16,
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
/// Nunchuk (PROTOCOL.md 4.2): C y Z
pub const BTN_C: u32 = 1 << 17;
pub const BTN_Z: u32 = 1 << 18;
/// Wii U GamePad / Pro Controller (PROTOCOL.md 4.2, modo `cemu`)
pub const BTN_X: u32 = 1 << 19;
pub const BTN_Y: u32 = 1 << 20;
pub const BTN_L: u32 = 1 << 21;
pub const BTN_R: u32 = 1 << 22;
pub const BTN_ZL: u32 = 1 << 23;
pub const BTN_ZR: u32 = 1 << 24;
/// Click del stick izquierdo / derecho
pub const BTN_STICK_L: u32 = 1 << 25;
pub const BTN_STICK_R: u32 = 1 << 26;
/// Soplar al micrófono del GamePad
pub const BTN_MIC: u32 = 1 << 27;
/// Cambiar la vista TV ↔ pantalla del GamePad (función de Cemu)
pub const BTN_SCREEN: u32 = 1 << 28;
/// Precisión (modo puntero, desde 1.4): mientras se mantiene, el cursor se
/// mueve al 40 %. En Dolphin y Cemu se ignora.
pub const BTN_PRECISION: u32 = 1 << 29;

#[derive(Debug, PartialEq)]
pub enum Packet {
    Input(InputPacket),
    Ping { session_id: u32, t_us: u64 },
    Pong { session_id: u32, t_us: u64 },
    Discover,
}

/// Un INPUT mide 72 bytes, u 80 si trae el bloque de extensión (FLAG_EXT).
fn input_len_ok(buf: &[u8]) -> bool {
    buf.len() == INPUT_LEN || (buf.len() == INPUT_EXT_LEN && buf[5] & FLAG_EXT != 0)
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
        TYPE_INPUT if input_len_ok(buf) => {
            let f32_at = |off: usize| f32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
            let ext = buf.len() == INPUT_EXT_LEN;
            Some(Packet::Input(InputPacket {
                // Sin bloque de extensión sus flags no significan nada:
                // se normalizan (y así parse→build vuelve a dar 72 bytes)
                flags: if ext { buf[5] } else { buf[5] & !(FLAG_EXT | FLAG_TOUCH) },
                stick_x: buf[6] as i8,
                stick_y: buf[7] as i8,
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
                stick_rx: if ext { buf[72] as i8 } else { 0 },
                stick_ry: if ext { buf[73] as i8 } else { 0 },
                touch_x: if ext { u16::from_le_bytes([buf[74], buf[75]]) } else { 0 },
                touch_y: if ext { u16::from_le_bytes([buf[76], buf[77]]) } else { 0 },
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

/// INPUT de 72 bytes (PROTOCOL.md §4.1), u 80 con FLAG_EXT (bloque Wii U).
pub fn build_input(p: &InputPacket) -> Vec<u8> {
    let ext = p.flags & FLAG_EXT != 0;
    let mut out = vec![0u8; if ext { INPUT_EXT_LEN } else { INPUT_LEN }];
    out[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    out[4] = TYPE_INPUT;
    out[5] = p.flags;
    out[6] = p.stick_x as u8; // stick del Nunchuk / izquierdo (0 en un Wiimote)
    out[7] = p.stick_y as u8;
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
    if ext {
        out[72] = p.stick_rx as u8;
        out[73] = p.stick_ry as u8;
        out[74..76].copy_from_slice(&p.touch_x.to_le_bytes());
        out[76..78].copy_from_slice(&p.touch_y.to_le_bytes());
        // 78-79 reservados a 0
    }
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
            "input_nunchuk" => from_hex(include_str!("../../protocol/vectors/input_nunchuk.hex")),
            "input_wiiu" => from_hex(include_str!("../../protocol/vectors/input_wiiu.hex")),
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
        assert_eq!((p.stick_x, p.stick_y), (0, 0), "sin stick: bytes reservados a 0");
        assert_eq!((p.stick_rx, p.stick_ry, p.touch_x, p.touch_y), (0, 0, 0, 0), "sin extensión");
    }

    #[test]
    fn vector_input_nunchuk() {
        let Packet::Input(p) = parse(&vector("input_nunchuk")).unwrap() else {
            panic!("no es INPUT")
        };
        assert_eq!(p.flags, FLAG_QUAT_VALID | FLAG_STICK_VALID);
        assert_eq!(p.seq, 10);
        assert_eq!(p.t_sensor_us, 4_000_000);
        assert_eq!((p.stick_x, p.stick_y), (100, -50));
        assert_eq!(p.buttons, BTN_C | BTN_Z);
        assert_eq!(p.accel, [0.0, 0.0, 9.5]);
        assert_eq!(p.battery_pct, 77);
    }

    #[test]
    fn vector_input_wiiu() {
        let buf = vector("input_wiiu");
        assert_eq!(buf.len(), INPUT_EXT_LEN);
        let Packet::Input(p) = parse(&buf).unwrap() else {
            panic!("no es INPUT")
        };
        assert_eq!(p.flags, FLAG_QUAT_VALID | FLAG_STICK_VALID | FLAG_EXT | FLAG_TOUCH);
        assert_eq!(p.session_id, 0xAABBCCDD);
        assert_eq!(p.seq, 11);
        assert_eq!(p.t_sensor_us, 5_000_000);
        assert_eq!((p.stick_x, p.stick_y), (100, -50), "stick izquierdo");
        assert_eq!((p.stick_rx, p.stick_ry), (-30, 120), "stick derecho");
        assert_eq!((p.touch_x, p.touch_y), (0x8000, 0x4000), "táctil");
        assert_eq!(
            p.buttons,
            BTN_A | BTN_X | BTN_Y | BTN_L | BTN_R | BTN_ZL | BTN_ZR | BTN_STICK_L | BTN_STICK_R | BTN_MIC | BTN_SCREEN
        );
        assert_eq!(p.buttons, 0x1FF8_0001);
        assert_eq!(p.accel, [0.0, 0.0, 9.5]);
        assert_eq!(p.recenter_count, 2);
        assert_eq!(p.battery_pct, 66);
        assert_eq!(p.touch_scroll_dy, 0);
    }

    #[test]
    fn extension_solo_con_su_flag() {
        // 80 bytes sin FLAG_EXT no es un INPUT válido
        let mut buf = vector("input_wiiu");
        buf[5] &= !FLAG_EXT;
        assert_eq!(parse(&buf), None);
        // 72 bytes con FLAG_EXT/FLAG_TOUCH puestos: se ignoran (y se limpian)
        let mut short = vector("input_nunchuk");
        short[5] |= FLAG_EXT | FLAG_TOUCH;
        let Packet::Input(p) = parse(&short).unwrap() else {
            panic!("no es INPUT")
        };
        assert_eq!(p.flags, FLAG_QUAT_VALID | FLAG_STICK_VALID);
        assert_eq!(build_input(&p).len(), INPUT_LEN);
        // build con FLAG_EXT da 80 bytes con el bloque; sin él, 72
        let p = InputPacket { flags: FLAG_EXT, stick_rx: -1, stick_ry: 2, touch_x: 3, touch_y: 4, ..Default::default() };
        let out = build_input(&p);
        assert_eq!(out.len(), INPUT_EXT_LEN);
        assert_eq!(&out[72..80], &[0xFF, 0x02, 0x03, 0x00, 0x04, 0x00, 0x00, 0x00]);
        assert_eq!(parse(&out), Some(Packet::Input(p)));
        assert_eq!(build_input(&InputPacket::default()).len(), INPUT_LEN);
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
        for name in ["input_neutral", "input_motion", "input_buttons_all", "input_nunchuk", "input_wiiu"] {
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

    #[test]
    fn el_bit_de_precision_es_el_29_y_viaja_en_el_input() {
        assert_eq!(BTN_PRECISION, 0x2000_0000);
        let wiiu = BTN_A | BTN_X | BTN_Y | BTN_L | BTN_R | BTN_ZL | BTN_ZR | BTN_STICK_L | BTN_STICK_R | BTN_MIC | BTN_SCREEN;
        assert_eq!(wiiu & BTN_PRECISION, 0, "no pisa ningún botón de Wii U");
        let buf = vector("input_wiiu");
        let Packet::Input(mut p) = parse(&buf).unwrap() else { panic!("no es INPUT") };
        p.buttons |= BTN_PRECISION;
        let Packet::Input(back) = parse(&build_input(&p)).unwrap() else { panic!("no es INPUT") };
        assert_eq!(back.buttons, p.buttons, "el bit 29 sobrevive al viaje");
    }
}
