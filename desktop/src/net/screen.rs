//! Canal de pantalla (PROTOCOL.md §7): una conexión TCP aparte por la que el
//! móvil GamePad recibe la pantalla del GamePad de Cemu en tramas JPEG, con
//! confirmación de un byte por imagen (una imagen en vuelo).

use super::Sessions;
use crate::screen::ScreenHub;
use crate::state::{Role, SharedState};
use serde_json::{json, Value};
use std::io::{BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const MAGIC: &[u8; 4] = b"PMPS";
pub const FRAME_JPEG: u8 = 1;
pub const FRAME_STATUS: u8 = 2;
pub const FRAME_KEEPALIVE: u8 = 3;
/// Sin confirmación del móvil en este tiempo, la conexión está muerta.
const ACK_TIMEOUT: Duration = Duration::from_secs(10);
const KEEPALIVE_EVERY: Duration = Duration::from_secs(2);

/// Cabecera + carga de una trama.
pub fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(kind);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

struct Subscription(Arc<ScreenHub>);

impl Drop for Subscription {
    fn drop(&mut self) {
        self.0.client_left();
    }
}

fn send_json(w: &mut TcpStream, v: &Value) -> std::io::Result<()> {
    let mut s = v.to_string();
    s.push('\n');
    w.write_all(s.as_bytes())
}

/// Atiende un móvil que ha abierto el canal con la línea `req`.
pub fn handle(
    mut reader: BufReader<TcpStream>,
    mut writer: TcpStream,
    req: &Value,
    shared: &SharedState,
    sessions: &Sessions,
    hub: &Arc<ScreenHub>,
) {
    let session_id = req["session_id"].as_u64().map(|v| v as u32);
    let ok = session_id.is_some_and(|id| {
        sessions.lock().unwrap().get(&id).is_some_and(|s| s.role == Role::Wiimote)
    });
    if !ok {
        let _ = send_json(
            &mut writer,
            &json!({"m":"err","code":"bad_session","msg":"Sesión desconocida: vuelve a conectar el mando"}),
        );
        return;
    }
    let w = req["w"].as_u64().unwrap_or(crate::screen::MAX_W as u64) as u32;
    let h = req["h"].as_u64().unwrap_or(crate::screen::MAX_H as u64) as u32;
    let q = req["q"].as_u64().unwrap_or(70) as u8;
    if send_json(&mut writer, &json!({"m":"screen","ok":true})).is_err() {
        return;
    }
    let _ = writer.set_nodelay(true);
    let _ = reader.get_ref().set_read_timeout(Some(ACK_TIMEOUT));
    hub.client_joined(w, h, q);
    let _sub = Subscription(hub.clone());
    let slot = session_id.and_then(|id| sessions.lock().unwrap().get(&id).map(|s| s.slot));
    if std::env::var_os("PEPOMOTE_DEBUG").is_some() {
        eprintln!("[screen] móvil del slot {slot:?} suscrito ({w}×{h}, q{q})");
    }
    let _ = shared; // el estado para la UI lo publica el hub

    let mut last_id = 0u64;
    let mut last_status = String::new();
    let mut last_sent = Instant::now();
    let mut ack = [0u8; 1];
    loop {
        // estado nuevo (sin ventana, minimizada…) antes que nada; "" = hay
        // imágenes y no se manda (el móvil oculta la imagen al recibir estado)
        let status = hub.status();
        if status != last_status {
            if !status.is_empty() && writer.write_all(&frame(FRAME_STATUS, status.as_bytes())).is_err() {
                break;
            }
            last_status = status;
            last_sent = Instant::now();
        }
        match hub.wait_frame(last_id, KEEPALIVE_EVERY) {
            Some(enc) => {
                last_id = enc.id;
                if writer.write_all(&frame(FRAME_JPEG, &enc.jpeg)).is_err() {
                    break;
                }
                last_sent = Instant::now();
                // una imagen en vuelo: la siguiente cuando el móvil la haya pintado
                match reader.read(&mut ack) {
                    Ok(1) if ack[0] == 1 => {}
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break, // 10 s sin confirmar: muerto
                }
            }
            None => {
                if last_sent.elapsed() >= KEEPALIVE_EVERY {
                    if writer.write_all(&frame(FRAME_KEEPALIVE, &[])).is_err() {
                        break;
                    }
                    last_sent = Instant::now();
                }
            }
        }
    }
    if std::env::var_os("PEPOMOTE_DEBUG").is_some() {
        eprintln!("[screen] móvil del slot {slot:?} se va");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trama_bien_formada() {
        let f = frame(FRAME_JPEG, &[0xFF, 0xD8, 0xFF, 0xD9]);
        assert_eq!(&f[..4], b"PMPS");
        assert_eq!(f[4], 1);
        assert_eq!(u32::from_le_bytes(f[5..9].try_into().unwrap()), 4);
        assert_eq!(&f[9..], &[0xFF, 0xD8, 0xFF, 0xD9]);
        let k = frame(FRAME_KEEPALIVE, &[]);
        assert_eq!(k.len(), 9);
        assert_eq!(k[4], 3);
    }
}
