//! Grabación y reproducción de la telemetría del puntero, para analizar un
//! gesto real (un flick que no va fino, una deriva) sin tener el móvil en
//! la mano:
//!
//! - `PEPOMOTE_RECORD=<archivo>` al arrancar el receptor: cada INPUT UDP del
//!   Jugador 1 se apunta tal cual llega (con su instante de llegada).
//! - `PepoMote --replay <archivo> [sens_deg]`: pasa la grabación por el motor
//!   del puntero y saca un CSV por stdout (una fila por paquete: tiempo del
//!   sensor, llegada, salida del motor) para mirarlo con calma.
//!
//! Formato del archivo: registros `[u64 llegada_us LE][u16 len LE][paquete]`.

use crate::net::codec::{self, Packet};
use crate::pointer::{PointerEngine, PointerOutput};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::time::Instant;

pub struct Recorder {
    out: BufWriter<File>,
    start: Instant,
}

impl Recorder {
    /// Abre la grabación si `PEPOMOTE_RECORD` está definida.
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("PEPOMOTE_RECORD")?;
        match File::create(&path) {
            Ok(f) => {
                eprintln!("[record] grabando la telemetría del Jugador 1 en {}", path.to_string_lossy());
                Some(Self { out: BufWriter::new(f), start: Instant::now() })
            }
            Err(e) => {
                eprintln!("[record] no puedo crear {}: {e}", path.to_string_lossy());
                None
            }
        }
    }

    pub fn write(&mut self, raw: &[u8]) {
        let t = self.start.elapsed().as_micros() as u64;
        let _ = self.out.write_all(&t.to_le_bytes());
        let _ = self.out.write_all(&(raw.len() as u16).to_le_bytes());
        let _ = self.out.write_all(raw);
        let _ = self.out.flush();
    }
}

/// Registros (llegada_us, paquete) de una grabación.
pub fn read_recording(path: &str) -> std::io::Result<Vec<(u64, Vec<u8>)>> {
    let mut data = Vec::new();
    File::open(path)?.read_to_end(&mut data)?;
    let mut out = Vec::new();
    let mut i = 0;
    while i + 10 <= data.len() {
        let t = u64::from_le_bytes(data[i..i + 8].try_into().unwrap());
        let len = u16::from_le_bytes(data[i + 8..i + 10].try_into().unwrap()) as usize;
        i += 10;
        if i + len > data.len() {
            break;
        }
        out.push((t, data[i..i + len].to_vec()));
        i += len;
    }
    Ok(out)
}

/// `--replay <archivo> [sens_deg]`: CSV por stdout. Devuelve true si el
/// argumento estaba (el receptor no arranca).
pub fn replay_from_args() -> bool {
    let args: Vec<String> = std::env::args().collect();
    let Some(i) = args.iter().position(|a| a == "--replay") else {
        return false;
    };
    let Some(path) = args.get(i + 1) else {
        eprintln!("uso: PepoMote --replay <archivo> [sens_deg]");
        return true;
    };
    let sens: f32 = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(40.0);
    match read_recording(path) {
        Ok(recs) => {
            let mut engine = PointerEngine::new();
            // El cursor real que vería el receptor: la última posición emitida,
            // recortada a la pantalla (el SO no deja salir el cursor)
            let mut last_abs: Option<(f32, f32)> = None;
            // PEPOMOTE_REPLAY_HINT_LAG=k: el SO tarda k paquetes en aplicar el
            // movimiento (para reproducir carreras SendInput/GetCursorPos)
            let hint_lag: usize = std::env::var("PEPOMOTE_REPLAY_HINT_LAG").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            let mut abs_history: std::collections::VecDeque<(f32, f32)> = std::collections::VecDeque::new();
            let stdout = std::io::stdout();
            let mut w = stdout.lock();
            // Columnas de diagnóstico al final: los parsers por índice siguen
            // valiendo. twist = roll del quat del sensor respecto al marco
            // propio del motor (en fase con gz durante un barrido = el sensor
            // torcido por la aceleración del gesto).
            let _ = writeln!(
                w,
                "t_sensor_us,llegada_us,flags,gx,gy,gz,qw,qx,qy,qz,salida,nx_o_dx,ny_o_dy,qyaw,qpitch,fyaw,fpitch,twist,off_y,off_p,shift_y,shift_p,hint_x,hint_y,congelado,quieto,bias_x,bias_y,bias_z"
            );
            for (arrival, raw) in recs {
                let Some(Packet::Input(p)) = codec::parse(&raw) else { continue };
                let seen = if hint_lag == 0 { last_abs } else { abs_history.front().copied().or(last_abs) };
                engine.set_cursor_hint(seen.map(|(x, y)| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))));
                let out = engine.apply(&p, sens, 16.0 / 9.0, true, 2560.0, p.buttons & codec::BTN_PRECISION != 0);
                if let PointerOutput::Abs { nx, ny } = out {
                    last_abs = Some((nx, ny));
                    abs_history.push_back((nx, ny));
                    while abs_history.len() > hint_lag.max(1) {
                        abs_history.pop_front();
                    }
                }
                let (kind, a, b) = match out {
                    PointerOutput::Abs { nx, ny } => ("abs", nx, ny),
                    PointerOutput::Rel { dx, dy } => ("rel", dx as f32, dy as f32),
                    PointerOutput::None => ("-", 0.0, 0.0),
                };
                let d = engine.debug();
                let (hx, hy) = d.hint.unwrap_or((f32::NAN, f32::NAN));
                let _ = writeln!(
                    w,
                    "{},{},{},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5},{},{:.5},{:.5},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{},{},{:.5},{:.5},{:.5}",
                    p.t_sensor_us, arrival, p.flags, p.gyro[0], p.gyro[1], p.gyro[2], p.quat[0], p.quat[1], p.quat[2],
                    p.quat[3], kind, a, b, d.qyaw, d.qpitch, d.fyaw, d.fpitch, d.twist_deg, d.offset.0, d.offset.1,
                    d.shift.0, d.shift.1, hx, hy, d.frozen as u8, d.quiet as u8, d.bias[0], d.bias[1], d.bias[2]
                );
            }
        }
        Err(e) => eprintln!("no puedo leer {path}: {e}"),
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grabacion_ida_y_vuelta() {
        let dir = std::env::temp_dir().join(format!("pepomote-rec-{}", std::process::id()));
        let path = dir.to_string_lossy().to_string();
        {
            let mut r = Recorder { out: BufWriter::new(File::create(&path).unwrap()), start: Instant::now() };
            r.write(&[1, 2, 3]);
            r.write(&[9; 80]);
        }
        let recs = read_recording(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].1, vec![1, 2, 3]);
        assert_eq!(recs[1].1.len(), 80);
        assert!(recs[1].0 >= recs[0].0);
    }
}
