//! Backend Qualcomm Snapdragon Sensor Core (SSC / SLPI): los Snapdragon
//! desde 2018 (SDM845: OnePlus 6/6T, SHIFT6mq, Poco F1…; SM8150+, SM7xxx…)
//! no conectan el IMU al procesador principal sino a un DSP de sensores, y
//! Linux no ve nada en IIO. Se habla con el DSP por **QRTR** (sockets
//! AF_QIPCRTR, sin privilegios) usando **QMI** (servicio 0x190 "SSC") con el
//! contenido en **protobuf**. Mismo protocolo que usa libssc (la lib que
//! da la rotación de pantalla en postmarketOS), reimplementado aquí porque
//! hace falta gyro + accel en streaming a ~200 Hz.
//!
//! Flujo: buscar el servicio en el bus → SUID del sensor "gyro" y "accel"
//! (petición al sensor especial SUID) → atributos (frecuencias, matriz) →
//! activar streaming continuo a la frecuencia elegida → informes 1025 con
//! 3 floats (rad/s · m/s²) y timestamp del QTimer (19,2 MHz).

// Fuera de Linux solo se compila el códec (tests): el cliente QRTR no existe
#![cfg_attr(not(target_os = "linux"), allow(dead_code, unused_imports))]

use super::{now_us, Sample, Source};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- protobuf

/// Codec protobuf mínimo (solo lo que usa el SSC).
pub mod pb {
    pub fn put_varint(out: &mut Vec<u8>, mut v: u64) {
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    fn key(out: &mut Vec<u8>, field: u32, wire: u8) {
        put_varint(out, ((field as u64) << 3) | wire as u64);
    }

    pub fn put_bytes(out: &mut Vec<u8>, field: u32, data: &[u8]) {
        key(out, field, 2);
        put_varint(out, data.len() as u64);
        out.extend_from_slice(data);
    }

    pub fn put_string(out: &mut Vec<u8>, field: u32, s: &str) {
        put_bytes(out, field, s.as_bytes());
    }

    pub fn put_fixed64(out: &mut Vec<u8>, field: u32, v: u64) {
        key(out, field, 1);
        out.extend_from_slice(&v.to_le_bytes());
    }

    pub fn put_fixed32(out: &mut Vec<u8>, field: u32, v: u32) {
        key(out, field, 5);
        out.extend_from_slice(&v.to_le_bytes());
    }

    pub fn put_float(out: &mut Vec<u8>, field: u32, v: f32) {
        put_fixed32(out, field, v.to_bits());
    }

    pub fn put_varint_field(out: &mut Vec<u8>, field: u32, v: u64) {
        key(out, field, 0);
        put_varint(out, v);
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Value<'a> {
        Varint(u64),
        Fixed64(u64),
        Fixed32(u32),
        Bytes(&'a [u8]),
    }

    fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let b = *buf.get(*pos)?;
            *pos += 1;
            v |= ((b & 0x7F) as u64) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }
    }

    /// Campos (número, valor) en orden de aparición.
    pub fn parse(buf: &[u8]) -> Option<Vec<(u32, Value<'_>)>> {
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < buf.len() {
            let k = read_varint(buf, &mut pos)?;
            let field = (k >> 3) as u32;
            match k & 7 {
                0 => out.push((field, Value::Varint(read_varint(buf, &mut pos)?))),
                1 => {
                    let b = buf.get(pos..pos + 8)?;
                    pos += 8;
                    out.push((field, Value::Fixed64(u64::from_le_bytes(b.try_into().ok()?))));
                }
                2 => {
                    let len = read_varint(buf, &mut pos)? as usize;
                    let b = buf.get(pos..pos + len)?;
                    pos += len;
                    out.push((field, Value::Bytes(b)));
                }
                5 => {
                    let b = buf.get(pos..pos + 4)?;
                    pos += 4;
                    out.push((field, Value::Fixed32(u32::from_le_bytes(b.try_into().ok()?))));
                }
                _ => return None,
            }
        }
        Some(out)
    }

    /// `repeated float` puede venir suelto (wire 5 por valor) o empaquetado
    /// (wire 2 con 4 bytes por valor): se aceptan las dos formas.
    pub fn floats(fields: &[(u32, Value<'_>)], field: u32) -> Vec<f32> {
        let mut v = Vec::new();
        for (f, val) in fields {
            if *f != field {
                continue;
            }
            match val {
                Value::Fixed32(x) => v.push(f32::from_bits(*x)),
                Value::Bytes(b) => {
                    for c in b.chunks_exact(4) {
                        v.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
                    }
                }
                _ => {}
            }
        }
        v
    }
}

// ------------------------------------------------------------- mensajes SSC

/// Sensor especial de descubrimiento (SUID lookup).
const SUID_SENSOR: (u64, u64) = (0xABAB_ABAB_ABAB_ABAB, 0xABAB_ABAB_ABAB_ABAB);
const MSG_SUID_REQ: u32 = 512;
const MSG_SUID_RESP: u32 = 768;
const MSG_ATTR_REQ: u32 = 1;
const MSG_ATTR_RESP: u32 = 128;
const MSG_ENABLE_CONTINUOUS: u32 = 513;
const MSG_DISABLE: u32 = 10;
const MSG_MEASUREMENT: u32 = 1025;
const PROCESSOR_APSS: u64 = 1;
const SUSPEND_WAKEUP: u64 = 0;
const ATTR_SAMPLE_RATE: u64 = 6;
const ATTR_MOUNT_MATRIX: u64 = 20;
/// Reloj del DSP (QTimer): 19,2 MHz.
const QTIMER_HZ: f64 = 19_200_000.0;
const MAX_RATE_HZ: f32 = 250.0;

fn encode_uid(uid: (u64, u64)) -> Vec<u8> {
    let mut b = Vec::with_capacity(20);
    pb::put_fixed64(&mut b, 1, uid.0); // low
    pb::put_fixed64(&mut b, 2, uid.1); // high
    b
}

/// SscClientRequest { uid=1, msg_id=2 fixed32, config=3 {processor=1, suspend_mode=2}, request=4 {msg=2 bytes} }
#[cfg_attr(not(test), allow(dead_code))]
pub fn encode_client_request(uid: (u64, u64), msg_id: u32, payload: Option<&[u8]>) -> Vec<u8> {
    encode_client_request_ext(uid, msg_id, payload, None)
}

/// Como `encode_client_request`, con `request.batching = 1 { batch_period = 1 }`
/// (sns_std_request.batch_spec): 0 = sin agrupar, cada muestra según sale.
pub fn encode_client_request_ext(uid: (u64, u64), msg_id: u32, payload: Option<&[u8]>, batch_period_us: Option<u32>) -> Vec<u8> {
    let mut config = Vec::new();
    pb::put_varint_field(&mut config, 1, PROCESSOR_APSS);
    pb::put_varint_field(&mut config, 2, SUSPEND_WAKEUP);
    let mut body = Vec::new();
    if let Some(bp) = batch_period_us {
        let mut batching = Vec::new();
        pb::put_varint_field(&mut batching, 1, bp.into());
        pb::put_bytes(&mut body, 1, &batching);
    }
    if let Some(p) = payload {
        pb::put_bytes(&mut body, 2, p);
    }
    let mut out = Vec::new();
    pb::put_bytes(&mut out, 1, &encode_uid(uid));
    pb::put_fixed32(&mut out, 2, msg_id);
    pb::put_bytes(&mut out, 3, &config);
    pb::put_bytes(&mut out, 4, &body);
    out
}

/// SscSuidRequest { data_type=1, enable_updates=2, only_default_values=3 }
pub fn encode_suid_request(data_type: &str) -> Vec<u8> {
    let mut out = Vec::new();
    pb::put_string(&mut out, 1, data_type);
    pb::put_varint_field(&mut out, 2, 0);
    pb::put_varint_field(&mut out, 3, 1);
    out
}

/// SscAttrRequest { enable_updates=2 }
pub fn encode_attr_request() -> Vec<u8> {
    let mut out = Vec::new();
    pb::put_varint_field(&mut out, 2, 0);
    out
}

/// SscEnableConfigRequest { sample_rate=1 float }
pub fn encode_enable(sample_rate: f32) -> Vec<u8> {
    let mut out = Vec::new();
    pb::put_float(&mut out, 1, sample_rate);
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub uid: (u64, u64),
    pub msg_id: u32,
    pub timestamp: u64,
    pub msg: Vec<u8>,
}

fn decode_uid(buf: &[u8]) -> Option<(u64, u64)> {
    let (mut low, mut high) = (0u64, 0u64);
    for (f, v) in pb::parse(buf)? {
        match (f, v) {
            (1, pb::Value::Fixed64(x)) => low = x,
            (2, pb::Value::Fixed64(x)) => high = x,
            _ => {}
        }
    }
    Some((low, high))
}

/// SscClientResponse { uid=1, response=2 repeated { msg_id=1 fixed32, timestamp=2 fixed64, msg=3 bytes } }
pub fn decode_client_response(buf: &[u8]) -> Option<Vec<Report>> {
    let fields = pb::parse(buf)?;
    let mut uid = (0, 0);
    let mut out = Vec::new();
    for (f, v) in &fields {
        match (f, v) {
            (1, pb::Value::Bytes(b)) => uid = decode_uid(b)?,
            (2, pb::Value::Bytes(b)) => {
                let mut r = Report {
                    uid,
                    msg_id: 0,
                    timestamp: 0,
                    msg: Vec::new(),
                };
                for (ff, vv) in pb::parse(b)? {
                    match (ff, vv) {
                        (1, pb::Value::Fixed32(x)) => r.msg_id = x,
                        (2, pb::Value::Fixed64(x)) => r.timestamp = x,
                        (3, pb::Value::Bytes(m)) => r.msg = m.to_vec(),
                        _ => {}
                    }
                }
                out.push(r);
            }
            _ => {}
        }
    }
    // el uid puede venir después de las respuestas en el orden de campos
    for r in &mut out {
        r.uid = uid;
    }
    Some(out)
}

/// SscSuidResponse { data_type=1, uid=2 repeated }
pub fn decode_suid_response(buf: &[u8]) -> Option<(String, Vec<(u64, u64)>)> {
    let mut data_type = String::new();
    let mut uids = Vec::new();
    for (f, v) in pb::parse(buf)? {
        match (f, v) {
            (1, pb::Value::Bytes(b)) => data_type = String::from_utf8_lossy(b).into_owned(),
            (2, pb::Value::Bytes(b)) => uids.push(decode_uid(b)?),
            _ => {}
        }
    }
    Some((data_type, uids))
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrVal {
    Float(f32),
    Int(u64),
    Bool(bool),
    Str(String),
    Other,
}

/// SscAttrResponse { attr=1 repeated { id=1, value_array=2 { v=1 repeated { a=1, s=2, f=3, i=4, b=5 } } } }
pub fn decode_attr_response(buf: &[u8]) -> Vec<(u64, Vec<AttrVal>)> {
    let mut out = Vec::new();
    let Some(fields) = pb::parse(buf) else { return out };
    for (f, v) in fields {
        let (1, pb::Value::Bytes(attr)) = (f, v) else { continue };
        let Some(af) = pb::parse(attr) else { continue };
        let mut id = u64::MAX;
        let mut vals = Vec::new();
        for (ff, vv) in af {
            match (ff, vv) {
                (1, pb::Value::Varint(x)) => id = x,
                (2, pb::Value::Bytes(arr)) => {
                    let Some(items) = pb::parse(arr) else { continue };
                    for (_, item) in items.into_iter().filter(|(k, _)| *k == 1) {
                        let pb::Value::Bytes(ib) = item else { continue };
                        let mut val = AttrVal::Other;
                        if let Some(inner) = pb::parse(ib) {
                            for (k, x) in inner {
                                val = match (k, x) {
                                    (2, pb::Value::Bytes(s)) => AttrVal::Str(String::from_utf8_lossy(s).into_owned()),
                                    (3, pb::Value::Fixed32(b)) => AttrVal::Float(f32::from_bits(b)),
                                    (4, pb::Value::Fixed64(i)) => AttrVal::Int(i),
                                    (5, pb::Value::Varint(b)) => AttrVal::Bool(b != 0),
                                    _ => continue,
                                };
                            }
                        }
                        vals.push(val);
                    }
                }
                _ => {}
            }
        }
        out.push((id, vals));
    }
    out
}

/// Medida: `repeated float` en el campo 1 (accel: aceleración m/s²; gyro: rad/s).
pub fn decode_vec3(buf: &[u8]) -> Option<[f32; 3]> {
    let v = pb::floats(&pb::parse(buf)?, 1);
    if v.len() < 3 {
        return None;
    }
    Some([v[0], v[1], v[2]])
}

/// Elige la frecuencia: la mayor que no pase del tope del protocolo.
pub fn pick_rate(rates: &[f32]) -> Option<f32> {
    let ok = rates.iter().copied().filter(|r| *r > 0.0 && *r <= MAX_RATE_HZ).fold(f32::NAN, f32::max);
    if ok.is_nan() {
        rates.iter().copied().filter(|r| *r > 0.0).fold(f32::NAN, f32::min).into_option()
    } else {
        Some(ok)
    }
}

trait NanOption {
    fn into_option(self) -> Option<f32>;
}
impl NanOption for f32 {
    fn into_option(self) -> Option<f32> {
        if self.is_nan() {
            None
        } else {
            Some(self)
        }
    }
}

// --------------------------------------------------------------------- QMI

pub const QMI_SERVICE_SSC: u32 = 0x190;
const QMI_TYPE_REQUEST: u8 = 0x00;
const QMI_TYPE_RESPONSE: u8 = 0x02;
const QMI_TYPE_INDICATION: u8 = 0x04;
const QMI_SSC_CONTROL: u16 = 0x0020;
const QMI_SSC_REPORT_SMALL: u16 = 0x0021;
const QMI_SSC_REPORT_LARGE: u16 = 0x0022;

/// QMI sobre QRTR: sin cabecera QMUX. [tipo u8][txn u16][msg u16][len u16][TLVs].
pub fn qmi_encode(msg_type: u8, txn: u16, msg_id: u16, tlvs: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let body_len: usize = tlvs.iter().map(|(_, v)| 3 + v.len()).sum();
    let mut out = Vec::with_capacity(7 + body_len);
    out.push(msg_type);
    out.extend_from_slice(&txn.to_le_bytes());
    out.extend_from_slice(&msg_id.to_le_bytes());
    out.extend_from_slice(&(body_len as u16).to_le_bytes());
    for (t, v) in tlvs {
        out.push(*t);
        out.extend_from_slice(&(v.len() as u16).to_le_bytes());
        out.extend_from_slice(v);
    }
    out
}

pub struct QmiMsg<'a> {
    pub msg_type: u8,
    pub txn: u16,
    pub msg_id: u16,
    pub tlvs: Vec<(u8, &'a [u8])>,
}

pub fn qmi_decode(buf: &[u8]) -> Option<QmiMsg<'_>> {
    if buf.len() < 7 {
        return None;
    }
    let msg_type = buf[0];
    let txn = u16::from_le_bytes([buf[1], buf[2]]);
    let msg_id = u16::from_le_bytes([buf[3], buf[4]]);
    let len = u16::from_le_bytes([buf[5], buf[6]]) as usize;
    let body = buf.get(7..7 + len)?;
    let mut tlvs = Vec::new();
    let mut pos = 0;
    while pos + 3 <= body.len() {
        let t = body[pos];
        let l = u16::from_le_bytes([body[pos + 1], body[pos + 2]]) as usize;
        let v = body.get(pos + 3..pos + 3 + l)?;
        tlvs.push((t, v));
        pos += 3 + l;
    }
    Some(QmiMsg {
        msg_type,
        txn,
        msg_id,
        tlvs,
    })
}

/// Petición Control: TLV 0x01 Data (u16 len + bytes) + TLV 0x10 Report Type = LARGE.
pub fn control_request(txn: u16, protobuf: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(2 + protobuf.len());
    data.extend_from_slice(&(protobuf.len() as u16).to_le_bytes());
    data.extend_from_slice(protobuf);
    qmi_encode(QMI_TYPE_REQUEST, txn, QMI_SSC_CONTROL, &[(0x01, data), (0x10, vec![0x01])])
}

/// Result TLV 0x02: (result u16, error u16); Ok si result == 0.
pub fn qmi_result(msg: &QmiMsg<'_>) -> Result<(), String> {
    let Some((_, v)) = msg.tlvs.iter().find(|(t, _)| *t == 0x02) else {
        return Err("respuesta QMI sin resultado".into());
    };
    if v.len() < 4 {
        return Err("resultado QMI truncado".into());
    }
    let result = u16::from_le_bytes([v[0], v[1]]);
    let error = u16::from_le_bytes([v[2], v[3]]);
    if result == 0 {
        Ok(())
    } else {
        Err(format!("el SSC rechazó la petición (error QMI {error})"))
    }
}

/// Indicación Report (small/large): TLV 0x02 Data (u16 len + protobuf).
pub fn report_payload<'a>(msg: &QmiMsg<'a>) -> Option<&'a [u8]> {
    if msg.msg_type != QMI_TYPE_INDICATION
        || (msg.msg_id != QMI_SSC_REPORT_SMALL && msg.msg_id != QMI_SSC_REPORT_LARGE)
    {
        return None;
    }
    let (_, v) = msg.tlvs.iter().find(|(t, _)| *t == 0x02)?;
    if v.len() < 2 {
        return None;
    }
    let len = u16::from_le_bytes([v[0], v[1]]) as usize;
    v.get(2..2 + len)
}

// -------------------------------------------------------------------- QRTR

#[cfg(target_os = "linux")]
mod qrtr {
    use std::io;
    use std::time::Duration;

    const AF_QIPCRTR: libc::c_int = 42;
    pub const QRTR_PORT_CTRL: u32 = 0xffff_fffe;
    const QRTR_TYPE_NEW_SERVER: u32 = 4;
    const QRTR_TYPE_NEW_LOOKUP: u32 = 10;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SockaddrQrtr {
        family: u16,
        node: u32,
        port: u32,
    }

    pub struct Sock {
        fd: libc::c_int,
        pub node: u32,
        pub port: u32,
    }

    impl Drop for Sock {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.fd);
            }
        }
    }

    impl Sock {
        pub fn open() -> io::Result<Sock> {
            let fd = unsafe { libc::socket(AF_QIPCRTR, libc::SOCK_DGRAM, 0) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut sock = Sock { fd, node: 0, port: 0 };
            // El kernel rechaza (EINVAL) un bind cuyo nodo no sea el local
            // (1 en kernels modernos): se lo preguntamos antes con getsockname.
            let me = sock.name().map_err(|e| io::Error::new(e.kind(), format!("getsockname: {e}")))?;
            sock.node = me.node;
            // puerto efímero automático (sin privilegios)
            let addr = SockaddrQrtr {
                family: AF_QIPCRTR as u16,
                node: me.node,
                port: 0,
            };
            let rc = unsafe {
                libc::bind(
                    fd,
                    &addr as *const SockaddrQrtr as *const libc::sockaddr,
                    std::mem::size_of::<SockaddrQrtr>() as libc::socklen_t,
                )
            };
            if rc < 0 {
                let e = io::Error::last_os_error();
                return Err(io::Error::new(e.kind(), format!("bind(nodo {}, puerto 0): {e}", me.node)));
            }
            let me = sock.name().map_err(|e| io::Error::new(e.kind(), format!("getsockname: {e}")))?;
            sock.node = me.node;
            sock.port = me.port;
            sock.set_timeout(Duration::from_millis(500))?;
            Ok(sock)
        }

        fn name(&self) -> io::Result<SockaddrQrtr> {
            let mut me = SockaddrQrtr {
                family: 0,
                node: 0,
                port: 0,
            };
            let mut len = std::mem::size_of::<SockaddrQrtr>() as libc::socklen_t;
            let rc = unsafe { libc::getsockname(self.fd, &mut me as *mut SockaddrQrtr as *mut libc::sockaddr, &mut len) };
            if rc < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(me)
        }

        pub fn set_timeout(&self, d: Duration) -> io::Result<()> {
            let tv = libc::timeval {
                tv_sec: d.as_secs() as libc::time_t,
                tv_usec: d.subsec_micros() as libc::suseconds_t,
            };
            let rc = unsafe {
                libc::setsockopt(
                    self.fd,
                    libc::SOL_SOCKET,
                    libc::SO_RCVTIMEO,
                    &tv as *const libc::timeval as *const libc::c_void,
                    std::mem::size_of::<libc::timeval>() as libc::socklen_t,
                )
            };
            if rc < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }

        pub fn send_to(&self, node: u32, port: u32, data: &[u8]) -> io::Result<()> {
            let addr = SockaddrQrtr {
                family: AF_QIPCRTR as u16,
                node,
                port,
            };
            let rc = unsafe {
                libc::sendto(
                    self.fd,
                    data.as_ptr() as *const libc::c_void,
                    data.len(),
                    0,
                    &addr as *const SockaddrQrtr as *const libc::sockaddr,
                    std::mem::size_of::<SockaddrQrtr>() as libc::socklen_t,
                )
            };
            if rc < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }

        /// (bytes, nodo, puerto) del remitente.
        pub fn recv(&self, buf: &mut [u8]) -> io::Result<(usize, u32, u32)> {
            let mut from = SockaddrQrtr {
                family: 0,
                node: 0,
                port: 0,
            };
            let mut len = std::mem::size_of::<SockaddrQrtr>() as libc::socklen_t;
            let rc = unsafe {
                libc::recvfrom(
                    self.fd,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                    0,
                    &mut from as *mut SockaddrQrtr as *mut libc::sockaddr,
                    &mut len,
                )
            };
            if rc < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok((rc as usize, from.node, from.port))
            }
        }

        /// Busca en el servidor de nombres los servidores de `service`: (nodo, puerto).
        pub fn lookup(&self, service: u32, timeout: Duration) -> io::Result<Vec<(u32, u32)>> {
            Ok(self
                .lookup_raw(service, timeout)?
                .into_iter()
                .filter(|s| s.service == service)
                .map(|s| (s.node, s.port))
                .collect())
        }

        /// Como `lookup`, con todos los datos; `service == 0` lista el bus entero.
        pub fn lookup_raw(&self, service: u32, timeout: Duration) -> io::Result<Vec<Server>> {
            let mut pkt = Vec::with_capacity(20);
            pkt.extend_from_slice(&QRTR_TYPE_NEW_LOOKUP.to_le_bytes());
            pkt.extend_from_slice(&service.to_le_bytes());
            pkt.extend_from_slice(&0u32.to_le_bytes()); // instancia: cualquiera
            pkt.extend_from_slice(&0u32.to_le_bytes());
            pkt.extend_from_slice(&0u32.to_le_bytes());
            self.send_to(self.node, QRTR_PORT_CTRL, &pkt)?;
            let mut found = Vec::new();
            let deadline = std::time::Instant::now() + timeout;
            let mut buf = [0u8; 64];
            while std::time::Instant::now() < deadline {
                let Ok((n, _node, port)) = self.recv(&mut buf) else { continue };
                if port != QRTR_PORT_CTRL || n < 20 {
                    continue;
                }
                let cmd = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                if cmd != QRTR_TYPE_NEW_SERVER {
                    continue;
                }
                let svc = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                let instance = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
                let node = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
                let sport = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
                if svc == 0 && node == 0 && sport == 0 {
                    break; // fin de la lista
                }
                found.push(Server {
                    service: svc,
                    instance,
                    node,
                    port: sport,
                });
            }
            Ok(found)
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub struct Server {
        pub service: u32,
        pub instance: u32,
        pub node: u32,
        pub port: u32,
    }
}

/// Busca (hasta 4 niveles) un directorio `sensors/registry` bajo `root`:
/// hexagonrpcd sirve ahí al SLPI los ficheros de registro copiados de Android
/// (convención `/usr/share/qcom/<soc>/<Vendor>/<device>/sensors/registry`).
#[cfg(target_os = "linux")]
fn find_sensor_registry(root: &std::path::Path, depth: u32) -> Option<std::path::PathBuf> {
    let cand = root.join("sensors").join("registry");
    if cand.is_dir() {
        return Some(cand);
    }
    if depth == 0 {
        return None;
    }
    let rd = std::fs::read_dir(root).ok()?;
    let mut dirs: Vec<std::path::PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    dirs.into_iter().find_map(|d| find_sensor_registry(&d, depth - 1))
}

/// Estado de la cadena que hace falta para que el SLPI anuncie el SSC:
/// remoteprocs (slpi debe estar `running`), hexagonrpcd (sirve al DSP los
/// ficheros de registro de sensores por FastRPC), nodos /dev/fastrpc*,
/// directorio de registro y lista de servicios del bus QRTR.
#[cfg(target_os = "linux")]
pub fn slpi_status() -> String {
    let mut out = String::new();
    // remoteprocs
    let mut rprocs: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/sys/class/remoteproc") {
        for e in rd.flatten() {
            let p = e.path();
            let name = std::fs::read_to_string(p.join("name")).unwrap_or_default();
            let state = std::fs::read_to_string(p.join("state")).unwrap_or_default();
            let name = name.trim();
            let short = name.rsplit('.').next().unwrap_or(name).replace("remoteproc-", "");
            rprocs.push(format!("{short}={}", state.trim()));
        }
    }
    rprocs.sort();
    out.push_str("  remoteproc: ");
    if rprocs.is_empty() {
        out.push_str("ninguno (¿kernel sin remoteproc/PAS?)");
    } else {
        out.push_str(&rprocs.join(" · "));
    }
    out.push('\n');
    // hexagonrpcd
    let proc_running = |prefix: &str| {
        std::fs::read_dir("/proc")
            .map(|rd| {
                rd.flatten().any(|e| {
                    std::fs::read_to_string(e.path().join("comm"))
                        .map(|c| c.trim().starts_with(prefix))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    };
    let mut fastrpc: Vec<String> = std::fs::read_dir("/dev")
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_owned()))
                .filter(|n| n.starts_with("fastrpc"))
                .collect()
        })
        .unwrap_or_default();
    fastrpc.sort();
    let registry = ["/usr/share/qcom", "/var/lib/qcom"]
        .iter()
        .find_map(|d| find_sensor_registry(std::path::Path::new(d), 4));
    out.push_str(&format!(
        "  hexagonrpcd: {} · /dev/fastrpc*: {} · registro sensores: {}\n",
        if proc_running("hexagonrpcd") { "corriendo" } else { "NO corre" },
        if fastrpc.is_empty() { "ninguno".to_owned() } else { fastrpc.join(",") },
        registry.map(|p| p.display().to_string()).unwrap_or_else(|| "no encontrado".to_owned())
    ));
    // bus QRTR
    match qrtr::Sock::open() {
        Err(e) => out.push_str(&format!("  QRTR: socket: {e}\n")),
        Ok(sock) => match sock.lookup_raw(0, Duration::from_secs(1)) {
            Err(e) => out.push_str(&format!("  QRTR: búsqueda: {e}\n")),
            Ok(list) => {
                let mut svcs: Vec<String> = list
                    .iter()
                    .map(|s| format!("{}:{}@{}:{}", s.service, s.instance, s.node, s.port))
                    .collect();
                svcs.sort();
                svcs.dedup();
                let ssc = list.iter().any(|s| s.service == QMI_SERVICE_SSC);
                out.push_str(&format!(
                    "  QRTR: {} servicios (svc:inst@nodo:puerto; SSC {}={}): {}\n",
                    svcs.len(),
                    QMI_SERVICE_SSC,
                    if ssc { "sí" } else { "NO" },
                    svcs.join(" ")
                ));
            }
        },
    }
    out
}

// ----------------------------------------------------------------- cliente

#[cfg(target_os = "linux")]
pub struct Ssc {
    sock: qrtr::Sock,
    server: (u32, u32),
    txn: u16,
    pending: Vec<Report>,
    accel: (u64, u64),
    gyro: (u64, u64),
    pub rate_hz: f32,
    pub info: String,
}

/// Qué hace falta para que el SLPI anuncie el SSC (postmarketOS).
#[cfg(target_os = "linux")]
const SLPI_HINT: &str = "Hace falta: remoteproc slpi en `running` (firmware slpi.mbn) y hexagonrpcd corriendo \
(postmarketOS: sudo apk add hexagonrpcd && sudo rc-update add hexagonrpcd-sdsp && sudo rc-service hexagonrpcd-sdsp start)";

#[cfg(target_os = "linux")]
impl Ssc {
    /// Conecta con el SSC y localiza gyro + accel. Bloqueante (≤ ~6 s).
    pub fn open() -> Result<Self, String> {
        let sock = qrtr::Sock::open().map_err(|e| format!("socket QRTR: {e}{}", if e.raw_os_error() == Some(libc::EAFNOSUPPORT) { " (kernel sin CONFIG_QRTR)" } else { "" }))?;
        let servers = sock
            .lookup(QMI_SERVICE_SSC, Duration::from_secs(2))
            .map_err(|e| format!("búsqueda QRTR: {e}"))?;
        let Some(&server) = servers.first() else {
            return Err(format!(
                "el bus QRTR no anuncia el servicio SSC ({QMI_SERVICE_SSC}): el SLPI no está operativo.\n{}  {}",
                slpi_status(),
                SLPI_HINT
            ));
        };
        let mut ssc = Ssc {
            sock,
            server,
            txn: 1,
            pending: Vec::new(),
            accel: (0, 0),
            gyro: (0, 0),
            rate_hz: 0.0,
            info: String::new(),
        };
        // El SSC tarda en estar listo tras el arranque: el sensor "registry"
        // aparece cuando el servicio está operativo
        let mut registry_ok = false;
        for _ in 0..5 {
            if let Ok(Some(_)) = ssc.lookup_suid("registry", Duration::from_millis(1500)) {
                registry_ok = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        if !registry_ok {
            ssc.info.push_str("aviso: el sensor 'registry' no responde (SSC arrancando o sin firmware)\n");
        }
        ssc.gyro = ssc
            .lookup_suid("gyro", Duration::from_secs(2))?
            .ok_or("el SSC no tiene ningún sensor 'gyro'")?;
        ssc.accel = ssc
            .lookup_suid("accel", Duration::from_secs(2))?
            .ok_or("el SSC no tiene ningún sensor 'accel'")?;

        let (g_rates, g_matrix) = ssc.attributes(ssc.gyro);
        let (a_rates, _) = ssc.attributes(ssc.accel);
        let g_rate = pick_rate(&g_rates).unwrap_or(200.0);
        let a_rate = pick_rate(&a_rates).unwrap_or(g_rate).min(g_rate.max(100.0));
        ssc.rate_hz = g_rate;
        ssc.info.push_str(&format!(
            "gyro {:016x}{:016x} rates {:?} · accel rates {:?}{}\n",
            ssc.gyro.1,
            ssc.gyro.0,
            g_rates,
            a_rates,
            g_matrix.map(|m| format!(" · matriz fw {m:?}")).unwrap_or_default()
        ));
        ssc.enable(ssc.gyro, g_rate)?;
        ssc.enable(ssc.accel, a_rate)?;
        Ok(ssc)
    }

    fn next_txn(&mut self) -> u16 {
        self.txn = self.txn.wrapping_add(1);
        if self.txn == 0 {
            self.txn = 1;
        }
        self.txn
    }

    /// Envía una petición al sensor y espera su acuse QMI (las indicaciones
    /// que lleguen entre medias se guardan).
    fn request(&mut self, uid: (u64, u64), msg_id: u32, payload: Option<&[u8]>) -> Result<(), String> {
        self.request_ext(uid, msg_id, payload, None)
    }

    fn request_ext(&mut self, uid: (u64, u64), msg_id: u32, payload: Option<&[u8]>, batch_period_us: Option<u32>) -> Result<(), String> {
        let txn = self.next_txn();
        let proto = encode_client_request_ext(uid, msg_id, payload, batch_period_us);
        let frame = control_request(txn, &proto);
        self.sock
            .send_to(self.server.0, self.server.1, &frame)
            .map_err(|e| format!("envío QMI: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut buf = vec![0u8; 8192];
        while Instant::now() < deadline {
            let Ok((n, _, _)) = self.sock.recv(&mut buf) else { continue };
            let Some(msg) = qmi_decode(&buf[..n]) else { continue };
            if msg.msg_type == QMI_TYPE_RESPONSE && msg.msg_id == QMI_SSC_CONTROL && msg.txn == txn {
                return qmi_result(&msg);
            }
            if let Some(p) = report_payload(&msg) {
                if let Some(reports) = decode_client_response(p) {
                    self.pending.extend(reports);
                }
            }
        }
        Err(format!("el SSC no contestó a la petición {msg_id}"))
    }

    /// Espera un informe que cumpla `pred` (los demás quedan pendientes).
    fn wait_report(&mut self, timeout: Duration, pred: impl Fn(&Report) -> bool) -> Option<Report> {
        if let Some(i) = self.pending.iter().position(&pred) {
            return Some(self.pending.remove(i));
        }
        let deadline = Instant::now() + timeout;
        let mut buf = vec![0u8; 8192];
        while Instant::now() < deadline {
            let Ok((n, _, _)) = self.sock.recv(&mut buf) else { continue };
            let Some(msg) = qmi_decode(&buf[..n]) else { continue };
            let Some(p) = report_payload(&msg) else { continue };
            let Some(reports) = decode_client_response(p) else { continue };
            for r in reports {
                if pred(&r) {
                    return Some(r);
                }
                self.pending.push(r);
            }
        }
        None
    }

    fn lookup_suid(&mut self, data_type: &str, timeout: Duration) -> Result<Option<(u64, u64)>, String> {
        self.request(SUID_SENSOR, MSG_SUID_REQ, Some(&encode_suid_request(data_type)))?;
        let wanted = data_type.to_owned();
        let r = self.wait_report(timeout, |r| {
            r.uid == SUID_SENSOR
                && r.msg_id == MSG_SUID_RESP
                && decode_suid_response(&r.msg).map(|(t, _)| t == wanted).unwrap_or(false)
        });
        Ok(r.and_then(|r| decode_suid_response(&r.msg)).and_then(|(_, uids)| uids.first().copied()))
    }

    /// (frecuencias disponibles, matriz de montaje del firmware).
    fn attributes(&mut self, uid: (u64, u64)) -> (Vec<f32>, Option<[[f32; 3]; 3]>) {
        if self.request(uid, MSG_ATTR_REQ, Some(&encode_attr_request())).is_err() {
            return (Vec::new(), None);
        }
        let Some(r) = self.wait_report(Duration::from_secs(2), |r| r.uid == uid && r.msg_id == MSG_ATTR_RESP) else {
            return (Vec::new(), None);
        };
        let mut rates = Vec::new();
        let mut matrix = None;
        for (id, vals) in decode_attr_response(&r.msg) {
            match id {
                ATTR_SAMPLE_RATE => {
                    rates = vals.iter().filter_map(|v| if let AttrVal::Float(f) = v { Some(*f) } else { None }).collect();
                }
                ATTR_MOUNT_MATRIX => {
                    let f: Vec<f32> = vals.iter().filter_map(|v| if let AttrVal::Float(f) = v { Some(*f) } else { None }).collect();
                    if f.len() >= 9 && f[..9].iter().any(|x| *x != 0.0) {
                        matrix = Some([[f[0], f[1], f[2]], [f[3], f[4], f[5]], [f[6], f[7], f[8]]]);
                    }
                }
                _ => {}
            }
        }
        (rates, matrix)
    }

    fn enable(&mut self, uid: (u64, u64), rate: f32) -> Result<(), String> {
        // Primero pidiendo explícitamente sin agrupar (batch_period 0): el
        // puntero quiere cada muestra al salir, no ráfagas. Si el DSP no lo
        // acepta, la petición de siempre.
        if self
            .request_ext(uid, MSG_ENABLE_CONTINUOUS, Some(&encode_enable(rate)), Some(0))
            .is_ok()
        {
            return Ok(());
        }
        self.request(uid, MSG_ENABLE_CONTINUOUS, Some(&encode_enable(rate)))
            .map_err(|e| format!("activar streaming a {rate} Hz: {e}"))
    }

    fn disable(&mut self, uid: (u64, u64)) {
        let _ = self.request(uid, MSG_DISABLE, None);
    }
}

#[cfg(target_os = "linux")]
impl Source for Ssc {
    fn run(mut self: Box<Self>, tx: Sender<Sample>, stop: Arc<AtomicBool>) {
        let mut last_accel = [0f32; 3];
        let mut buf = vec![0u8; 16384];
        let _ = self.sock.set_timeout(Duration::from_millis(200));
        let pending = std::mem::take(&mut self.pending);
        let mut queue: Vec<Report> = pending;
        while !stop.load(Ordering::Relaxed) {
            if queue.is_empty() {
                let Ok((n, _, _)) = self.sock.recv(&mut buf) else { continue };
                let Some(msg) = qmi_decode(&buf[..n]) else { continue };
                let Some(p) = report_payload(&msg) else { continue };
                if let Some(reports) = decode_client_response(p) {
                    queue.extend(reports);
                }
                continue;
            }
            for r in queue.drain(..) {
                if r.msg_id != MSG_MEASUREMENT {
                    continue;
                }
                let Some(v) = decode_vec3(&r.msg) else { continue };
                if r.uid == self.accel {
                    last_accel = v;
                } else if r.uid == self.gyro {
                    let t_us = if r.timestamp > 0 {
                        (r.timestamp as f64 / QTIMER_HZ * 1e6) as u64
                    } else {
                        now_us()
                    };
                    if tx
                        .send(Sample {
                            t_us,
                            gyro: v,
                            accel: last_accel,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }
        let (g, a) = (self.gyro, self.accel);
        self.disable(g);
        self.disable(a);
    }

    fn describe(&self) -> String {
        format!("SSC/SLPI Qualcomm · gyro+accel · {:.0} Hz", self.rate_hz)
    }
}

/// Diagnóstico para `--sensors` / inventario.
pub fn probe() -> String {
    #[cfg(target_os = "linux")]
    {
        match Ssc::open() {
            Ok(mut s) => {
                let (g, a) = (s.gyro, s.accel);
                s.disable(g);
                s.disable(a);
                format!("SSC (Qualcomm SLPI por QRTR): OK · {}{}", s.describe(), if s.info.is_empty() { String::new() } else { format!("\n  {}", s.info.trim_end().replace('\n', "\n  ")) })
            }
            Err(e) => {
                if e.contains("remoteproc:") {
                    format!("SSC (Qualcomm SLPI por QRTR): {e}")
                } else {
                    format!("SSC (Qualcomm SLPI por QRTR): {e}\n{}", slpi_status().trim_end())
                }
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        "SSC: solo en Linux".to_owned()
    }
}

#[cfg(not(target_os = "linux"))]
pub struct Ssc;

#[cfg(not(target_os = "linux"))]
impl Ssc {
    pub fn open() -> Result<Self, String> {
        Err("SSC: solo en Linux".into())
    }
}

#[cfg(not(target_os = "linux"))]
impl Source for Ssc {
    fn run(self: Box<Self>, _tx: Sender<Sample>, _stop: Arc<AtomicBool>) {}
    fn describe(&self) -> String {
        "SSC".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peticion_con_batching_explicito() {
        let base = encode_client_request((1, 2), 513, Some(&[0x0d, 0, 0, 0x48, 0x43]));
        let ext = encode_client_request_ext((1, 2), 513, Some(&[0x0d, 0, 0, 0x48, 0x43]), Some(0));
        assert_eq!(base, encode_client_request_ext((1, 2), 513, Some(&[0x0d, 0, 0, 0x48, 0x43]), None));
        // request=4 { batching=1 { batch_period=1: 0 }, msg=2 {...} }
        let body_tag = ext.iter().rposition(|&b| b == 0x22).unwrap();
        assert_eq!(&ext[body_tag..body_tag + 6], &[0x22, ext[body_tag + 1], 0x0a, 0x02, 0x08, 0x00]);
        assert_eq!(ext.len(), base.len() + 4);
    }

    #[test]
    fn protobuf_ida_y_vuelta() {
        let mut b = Vec::new();
        pb::put_varint_field(&mut b, 1, 300);
        pb::put_fixed64(&mut b, 2, 0xABAB_ABAB_ABAB_ABAB);
        pb::put_float(&mut b, 3, 1.5);
        pb::put_string(&mut b, 4, "gyro");
        let f = pb::parse(&b).unwrap();
        assert_eq!(f[0], (1, pb::Value::Varint(300)));
        assert_eq!(f[1], (2, pb::Value::Fixed64(0xABAB_ABAB_ABAB_ABAB)));
        assert_eq!(f[2], (3, pb::Value::Fixed32(1.5f32.to_bits())));
        assert_eq!(f[3], (4, pb::Value::Bytes(b"gyro")));
    }

    #[test]
    fn peticion_cliente_tiene_los_campos_de_libssc() {
        let req = encode_client_request(SUID_SENSOR, MSG_SUID_REQ, Some(&encode_suid_request("gyro")));
        let f = pb::parse(&req).unwrap();
        // uid=1 (msg), msg_id=2 (fixed32), config=3 (msg), request=4 (msg)
        assert!(matches!(f[0], (1, pb::Value::Bytes(_))));
        assert_eq!(f[1], (2, pb::Value::Fixed32(512)));
        let pb::Value::Bytes(cfg) = f[2].1 else { panic!() };
        assert_eq!(pb::parse(cfg).unwrap(), vec![(1, pb::Value::Varint(1)), (2, pb::Value::Varint(0))]);
        let pb::Value::Bytes(body) = f[3].1 else { panic!() };
        let bf = pb::parse(body).unwrap();
        let pb::Value::Bytes(suid_req) = bf[0].1 else { panic!() };
        let sf = pb::parse(suid_req).unwrap();
        assert_eq!(sf[0], (1, pb::Value::Bytes(b"gyro")));
        assert_eq!(sf[2], (3, pb::Value::Varint(1)), "only_default_values");
        // uid: low=1, high=2 fixed64
        let pb::Value::Bytes(uid) = f[0].1 else { panic!() };
        assert_eq!(decode_uid(uid), Some(SUID_SENSOR));
    }

    #[test]
    fn respuesta_cliente_y_suid() {
        // SscClientResponse { uid, response[ { msg_id 768, ts 19200000, msg = SscSuidResponse } ] }
        let mut suid_resp = Vec::new();
        pb::put_string(&mut suid_resp, 1, "gyro");
        pb::put_bytes(&mut suid_resp, 2, &encode_uid((0x1122, 0x3344)));
        let mut body = Vec::new();
        pb::put_fixed32(&mut body, 1, MSG_SUID_RESP);
        pb::put_fixed64(&mut body, 2, 19_200_000);
        pb::put_bytes(&mut body, 3, &suid_resp);
        let mut resp = Vec::new();
        pb::put_bytes(&mut resp, 1, &encode_uid(SUID_SENSOR));
        pb::put_bytes(&mut resp, 2, &body);
        let reports = decode_client_response(&resp).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].uid, SUID_SENSOR);
        assert_eq!(reports[0].msg_id, 768);
        assert_eq!(reports[0].timestamp, 19_200_000);
        let (dt, uids) = decode_suid_response(&reports[0].msg).unwrap();
        assert_eq!(dt, "gyro");
        assert_eq!(uids, vec![(0x1122, 0x3344)]);
    }

    #[test]
    fn medida_suelta_y_empaquetada() {
        let mut loose = Vec::new();
        for v in [0.1f32, -0.2, 9.8] {
            pb::put_float(&mut loose, 1, v);
        }
        pb::put_varint_field(&mut loose, 2, 3);
        assert_eq!(decode_vec3(&loose), Some([0.1, -0.2, 9.8]));
        let mut packed = Vec::new();
        let mut raw = Vec::new();
        for v in [1.0f32, 2.0, 3.0] {
            raw.extend_from_slice(&v.to_le_bytes());
        }
        pb::put_bytes(&mut packed, 1, &raw);
        assert_eq!(decode_vec3(&packed), Some([1.0, 2.0, 3.0]));
    }

    #[test]
    fn atributos_frecuencias_y_matriz() {
        // attr { id=6, value_array { v{f=100} v{f=200} v{f=400} } }, attr { id=20, value_array { 9 floats } }
        fn value_f(f: f32) -> Vec<u8> {
            let mut v = Vec::new();
            pb::put_float(&mut v, 3, f);
            v
        }
        let mut arr = Vec::new();
        for f in [100.0, 200.0, 400.0] {
            pb::put_bytes(&mut arr, 1, &value_f(f));
        }
        let mut a1 = Vec::new();
        pb::put_varint_field(&mut a1, 1, 6);
        pb::put_bytes(&mut a1, 2, &arr);
        let mut arr2 = Vec::new();
        for f in [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0] {
            pb::put_bytes(&mut arr2, 1, &value_f(f));
        }
        let mut a2 = Vec::new();
        pb::put_varint_field(&mut a2, 1, 20);
        pb::put_bytes(&mut a2, 2, &arr2);
        let mut resp = Vec::new();
        pb::put_bytes(&mut resp, 1, &a1);
        pb::put_bytes(&mut resp, 1, &a2);
        let attrs = decode_attr_response(&resp);
        assert_eq!(attrs[0].0, 6);
        assert_eq!(attrs[0].1, vec![AttrVal::Float(100.0), AttrVal::Float(200.0), AttrVal::Float(400.0)]);
        assert_eq!(attrs[1].0, 20);
        assert_eq!(attrs[1].1.len(), 9);
        assert_eq!(pick_rate(&[100.0, 200.0, 400.0]), Some(200.0));
        assert_eq!(pick_rate(&[400.0, 800.0]), Some(400.0));
        assert_eq!(pick_rate(&[]), None);
    }

    #[test]
    fn qmi_control_e_indicacion() {
        let frame = control_request(7, b"hola");
        let m = qmi_decode(&frame).unwrap();
        assert_eq!((m.msg_type, m.txn, m.msg_id), (0, 7, 0x0020));
        assert_eq!(m.tlvs[0].0, 0x01);
        assert_eq!(&m.tlvs[0].1[..2], &4u16.to_le_bytes());
        assert_eq!(&m.tlvs[0].1[2..], b"hola");
        assert_eq!(m.tlvs[1], (0x10, &[1u8][..]));

        let ok = qmi_encode(QMI_TYPE_RESPONSE, 7, QMI_SSC_CONTROL, &[(0x02, vec![0, 0, 0, 0])]);
        assert!(qmi_result(&qmi_decode(&ok).unwrap()).is_ok());
        let bad = qmi_encode(QMI_TYPE_RESPONSE, 7, QMI_SSC_CONTROL, &[(0x02, vec![1, 0, 3, 0])]);
        assert!(qmi_result(&qmi_decode(&bad).unwrap()).is_err());

        let mut data = Vec::new();
        data.extend_from_slice(&3u16.to_le_bytes());
        data.extend_from_slice(b"abc");
        let ind = qmi_encode(QMI_TYPE_INDICATION, 0, QMI_SSC_REPORT_LARGE, &[(0x01, vec![0; 8]), (0x02, data)]);
        let m = qmi_decode(&ind).unwrap();
        assert_eq!(report_payload(&m), Some(&b"abc"[..]));
    }
}
