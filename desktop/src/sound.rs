//! Chimes de conexión/desconexión — sintetizados con rodio, cero assets.
//!
//! Aislados del resto: el audio del sistema falla de mil maneras (sin
//! dispositivo, sin plugin ALSA de PipeWire, o cpal entrando en pánico en su
//! propio hilo con ciertos drivers) y nada de eso puede tocar al receptor.
//! Cada chime va en su hilo dentro de `catch_unwind`, con espera por tiempo
//! (nunca `sleep_until_end`: si el hilo de audio murió esperaríamos para
//! siempre) y un pánico desactiva el sonido para el resto de la sesión.

use rodio::source::{SineWave, Source};
use rodio::{OutputStream, Sink};
use std::any::Any;
use std::mem::ManuallyDrop;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Un pánico del audio (o un hilo colgado): sin más chimes en esta sesión.
static DISABLED: AtomicBool = AtomicBool::new(false);
/// Un error normal (sin dispositivo…) se cuenta una vez y se sigue probando.
static ERROR_LOGGED: AtomicBool = AtomicBool::new(false);
/// Hay un chime sonando: no se apilan hilos.
static BUSY: AtomicBool = AtomicBool::new(false);
/// Desde cuándo (ms de reloj propio) está ocupado.
static BUSY_SINCE_MS: AtomicU64 = AtomicU64::new(0);
static ORIGIN: OnceLock<Instant> = OnceLock::new();

/// Margen tras la última nota antes de cerrar el dispositivo.
const NOTE_MARGIN: Duration = Duration::from_millis(150);
/// Un chime dura ~0,3 s: ocupado más de esto = hilo de audio colgado.
const STUCK_AFTER: Duration = Duration::from_secs(5);

const CONNECT: &[(f32, u64)] = &[(659.0, 90), (784.0, 90), (1046.5, 160)];
const DISCONNECT: &[(f32, u64)] = &[(1046.5, 90), (784.0, 90), (659.0, 160)];
/// Cada jugador tiene su campanita: la misma melodía transportada (J1 tal
/// cual, J2 una tercera mayor, J3 una quinta, J4 una octava), así se sabe de
/// oído quién ha entrado o salido.
const SEMITONES_BY_PLAYER: [i32; 4] = [0, 4, 7, 12];

/// Semitonos del jugador `player` (1..4; fuera de rango, el más cercano).
fn semitones_for(player: u8) -> i32 {
    SEMITONES_BY_PLAYER[(player.max(1) as usize - 1).min(SEMITONES_BY_PLAYER.len() - 1)]
}

/// La melodía `semitones` más aguda (f·2^(n/12)), mismas duraciones.
fn transpose(notes: &[(f32, u64)], semitones: i32) -> Vec<(f32, u64)> {
    let k = 2f32.powf(semitones as f32 / 12.0);
    notes.iter().map(|&(f, ms)| (f * k, ms)).collect()
}

/// Campanita de entrada del jugador `player` (1..4).
pub fn connect_chime(player: u8) {
    play_notes("conexión", transpose(CONNECT, semitones_for(player)));
}

/// Campanita de salida del jugador `player` (1..4).
pub fn disconnect_chime(player: u8) {
    play_notes("desconexión", transpose(DISCONNECT, semitones_for(player)));
}

/// El sonido se desactivó en esta sesión (para `--diag`).
pub fn disabled() -> bool {
    DISABLED.load(Ordering::Relaxed)
}

/// Abre y cierra la salida por defecto sin sonar (para `--diag`; llamar
/// dentro de `catch_unwind`).
pub fn probe() -> Result<(), String> {
    play_blocking(&[])
}

fn elapsed_ms() -> u64 {
    ORIGIN.get_or_init(Instant::now).elapsed().as_millis() as u64
}

fn total_ms(notes: &[(f32, u64)]) -> u64 {
    notes.iter().map(|n| n.1).sum()
}

fn play_notes(what: &'static str, notes: Vec<(f32, u64)>) {
    if disabled() {
        return;
    }
    if BUSY.swap(true, Ordering::AcqRel) {
        // Ya hay uno sonando. Si lleva demasiado, el hilo de audio se colgó
        let since = BUSY_SINCE_MS.load(Ordering::Relaxed);
        if elapsed_ms().saturating_sub(since) > STUCK_AFTER.as_millis() as u64 {
            disable(&format!("{what}: el hilo de audio no responde"));
        }
        return; // se pierde este chime; jamás se apilan hilos
    }
    BUSY_SINCE_MS.store(elapsed_ms(), Ordering::Relaxed);
    let spawned = std::thread::Builder::new()
        .name("pmp-sound".into())
        .spawn(move || {
            let r = catch_unwind(AssertUnwindSafe(|| play_blocking(&notes)));
            note_outcome(what, r);
            BUSY.store(false, Ordering::Release);
        });
    if spawned.is_err() {
        BUSY.store(false, Ordering::Release);
    }
}

/// Reproduce y espera por tiempo. Stream y sink van en `ManuallyDrop`: si
/// algo revienta en medio no se destruyen durante el desenrollado (un Drop
/// que entra en pánico mientras se desenrolla aborta el proceso entero); la
/// destrucción va aparte, en su propio `catch_unwind`.
fn play_blocking(notes: &[(f32, u64)]) -> Result<(), String> {
    let (stream, handle) = OutputStream::try_default()
        .map_err(|e| format!("no se pudo abrir la salida de audio: {e}"))?;
    let stream = ManuallyDrop::new(stream);
    let sink = match Sink::try_new(&handle) {
        Ok(s) => ManuallyDrop::new(s),
        Err(e) => {
            drop_quietly(stream, None)?;
            return Err(format!("no se pudo preparar el audio: {e}"));
        }
    };
    for &(freq, ms) in notes {
        sink.append(
            SineWave::new(freq)
                .take_duration(Duration::from_millis(ms))
                .amplify(0.18)
                .fade_in(Duration::from_millis(4)),
        );
    }
    std::thread::sleep(Duration::from_millis(total_ms(notes)) + NOTE_MARGIN);
    drop_quietly(stream, Some(sink))
}

/// Destruye sink y stream capturando el pánico que cpal puede lanzar al
/// cerrar (hace `join().unwrap()` de su hilo de audio: si este murió,
/// revienta aquí y no en el receptor).
fn drop_quietly(stream: ManuallyDrop<OutputStream>, sink: Option<ManuallyDrop<Sink>>) -> Result<(), String> {
    let r = catch_unwind(AssertUnwindSafe(move || {
        let mut stream = stream;
        if let Some(sink) = sink {
            let mut sink = sink;
            sink.stop();
            // SAFETY: cada ManuallyDrop se suelta exactamente una vez, aquí
            unsafe { ManuallyDrop::drop(&mut sink) };
        }
        unsafe { ManuallyDrop::drop(&mut stream) };
    }));
    r.map_err(|p| format!("pánico al cerrar el audio: {}", panic_text(&p)))
}

fn panic_text(p: &Box<dyn Any + Send>) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(sin mensaje)".to_owned())
}

/// Qué hacer con el resultado de un chime: un error normal se apunta una
/// vez (y se seguirá probando: quizá vuelva el dispositivo); un pánico
/// desactiva el sonido. Devuelve si esta llamada desactivó.
fn note_outcome(what: &str, r: Result<Result<(), String>, Box<dyn Any + Send>>) -> bool {
    match r {
        Ok(Ok(())) => false,
        Ok(Err(e)) => {
            if !ERROR_LOGGED.swap(true, Ordering::AcqRel) {
                crate::log_line!("Sonido: {what}: {e} (se seguirá intentando)");
            }
            false
        }
        Err(p) => disable(&format!("{what}: pánico en el audio: {}", panic_text(&p))),
    }
}

/// Desactiva el sonido para el resto de la sesión (avisa la primera vez).
fn disable(reason: &str) -> bool {
    let first = !DISABLED.swap(true, Ordering::AcqRel);
    if first {
        crate::log_line!("Sonido desactivado: {reason}");
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn reset() {
        DISABLED.store(false, Ordering::Relaxed);
        ERROR_LOGGED.store(false, Ordering::Relaxed);
        BUSY.store(false, Ordering::Relaxed);
    }

    #[test]
    fn duracion_total_de_las_notas() {
        assert_eq!(total_ms(CONNECT), 340);
        assert_eq!(total_ms(DISCONNECT), 340);
        assert_eq!(total_ms(&[]), 0);
    }

    #[test]
    fn un_panico_desactiva_el_sonido_y_solo_avisa_una_vez() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(!disabled());
        assert!(note_outcome("conexión", Err(Box::new("get_htstamp was earlier"))));
        assert!(disabled());
        // el segundo ya no "desactiva" (y por tanto no vuelve a avisar)
        assert!(!note_outcome("conexión", Err(Box::new(String::from("otra vez")))));
        assert!(disabled());
        reset();
    }

    #[test]
    fn un_error_normal_no_desactiva() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(!note_outcome("conexión", Ok(Err("sin dispositivo".into()))));
        assert!(!disabled());
        assert!(ERROR_LOGGED.load(Ordering::Relaxed));
        assert!(!note_outcome("conexión", Ok(Ok(()))));
        reset();
    }

    #[test]
    fn desactivado_no_lanza_hilo() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        DISABLED.store(true, Ordering::Relaxed);
        play_notes("prueba", CONNECT.to_vec());
        assert!(!BUSY.load(Ordering::Relaxed), "con el sonido desactivado no se ocupa nada");
        reset();
    }

    #[test]
    fn transponer_una_octava_dobla_la_frecuencia() {
        let up = transpose(CONNECT, 12);
        for (a, b) in CONNECT.iter().zip(&up) {
            assert!((b.0 - 2.0 * a.0).abs() < 1e-2, "{} → {}", a.0, b.0);
        }
        assert_eq!(transpose(CONNECT, 0), CONNECT.to_vec(), "cero semitonos: tal cual");
    }

    #[test]
    fn transponer_conserva_las_duraciones() {
        for n in SEMITONES_BY_PLAYER {
            assert_eq!(total_ms(&transpose(DISCONNECT, n)), 340);
        }
    }

    #[test]
    fn semitonos_por_jugador() {
        assert_eq!([1u8, 2, 3, 4].map(semitones_for), [0, 4, 7, 12]);
        assert_eq!(semitones_for(0), 0, "sin jugador: como el 1");
        assert_eq!(semitones_for(9), 12, "más de 4: como el 4");
    }
}
