//! Registro de diagnóstico del receptor: `receptor.log` en la carpeta de
//! configuración, siempre activo y de bajo volumen (nunca una línea por
//! paquete). Y el hook de pánico: el perfil release desenrolla (no aborta),
//! así que un pánico en un hilo secundario ya no cierra el receptor, pero
//! queda aquí (hilo, mensaje, archivo:línea) y en la ventana.

use crate::state::SharedState;
use std::any::Any;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Tope del log; al superarlo se renombra a `receptor.log.1` (una sola
/// copia anterior) y se empieza otro.
const LOG_CAP: u64 = 512 * 1024;
const FILE_NAME: &str = "receptor.log";

static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static SHARED: OnceLock<SharedState> = OnceLock::new();
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Resuelve la ruta del log (una vez), crea la carpeta y engancha el `log`
/// de las bibliotecas.
pub fn init() {
    let _ = path();
    if logfacade::set_logger(&BRIDGE).is_ok() {
        logfacade::set_max_level(logfacade::LevelFilter::Info);
    }
}

/// Ruta del log (None si no hay carpeta de configuración).
pub fn path() -> Option<PathBuf> {
    PATH.get_or_init(|| {
        let dir = crate::state::config_dir()?;
        let _ = std::fs::create_dir_all(&dir);
        Some(dir.join(FILE_NAME))
    })
    .clone()
}

/// El estado compartido, para que el hook de pánico pueda avisar en la ventana.
pub fn attach_shared(shared: SharedState) {
    let _ = SHARED.set(shared);
}

/// Puente del `log` de las bibliotecas (eframe, glutin, egui_glow, winit vía
/// tracing…) a receptor.log: sin él, el «Exiting because of error: …» de
/// eframe y los «X11 error» de winit se perdían. Avisos y errores de todo;
/// lo informativo, solo de la pila de ventana.
struct Bridge;

fn bridged(level: logfacade::Level, target: &str) -> bool {
    level <= logfacade::Level::Warn
        || (level == logfacade::Level::Info
            && (target.starts_with("eframe") || target.starts_with("egui_glow") || target.starts_with("egui_wgpu")))
}

impl logfacade::Log for Bridge {
    fn enabled(&self, m: &logfacade::Metadata) -> bool {
        bridged(m.level(), m.target())
    }

    fn log(&self, r: &logfacade::Record) {
        if self.enabled(r.metadata()) {
            line(&format!("[{}] {}", r.target(), r.args()));
        }
    }

    fn flush(&self) {}
}

static BRIDGE: Bridge = Bridge;

/// Una línea al log y a stderr.
pub fn line(msg: &str) {
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("?");
    let (secs, millis) = now();
    if let Some(p) = path() {
        append(&p, LOG_CAP, &format_line(secs, millis, name, msg));
    }
    // stderr DESPUÉS del archivo y sin `eprintln!`: con stderr cerrado
    // (terminal que se fue, tubería rota) `eprintln!` entra en pánico, y si
    // eso pasa dentro del hook de pánico el proceso aborta sin dejar rastro
    let _ = writeln!(std::io::stderr(), "[pepomote] {msg}");
}

#[macro_export]
macro_rules! log_line {
    ($($t:tt)*) => {
        $crate::log::line(&format!($($t)*))
    };
}

/// Últimas `n` líneas del log (para `--diag`).
pub fn tail(n: usize) -> Vec<String> {
    path().map(|p| tail_of(&p, n)).unwrap_or_default()
}

fn tail_of(path: &Path, n: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let skip = lines.len().saturating_sub(n);
    lines[skip..].iter().map(|s| (*s).to_owned()).collect()
}

fn now() -> (u64, u32) {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs(), d.subsec_millis()))
        .unwrap_or((0, 0))
}

fn format_line(secs: u64, millis: u32, thread: &str, msg: &str) -> String {
    format!("{secs}.{millis:03} [{thread}] {msg}")
}

/// Si el log supera `cap` se renombra a `.1` (pisando la copia anterior).
/// Devuelve si rotó.
fn rotate_if_needed(path: &Path, cap: u64) -> bool {
    match std::fs::metadata(path) {
        Ok(m) if m.len() > cap => {
            let mut old = path.as_os_str().to_owned();
            old.push(".1");
            std::fs::rename(path, PathBuf::from(old)).is_ok()
        }
        _ => false,
    }
}

/// Añade una línea. Nunca falla ni entra en pánico (un log que no se puede
/// escribir no es motivo para nada).
fn append(path: &Path, cap: u64, text: &str) {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    rotate_if_needed(path, cap);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{text}");
    }
}

/// Texto del mensaje de un pánico (`&str` o `String`; si no, sin mensaje).
pub fn payload_text(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(sin mensaje)".to_owned()
    }
}

/// Línea de log de un pánico.
fn describe_panic(thread: &str, payload: &(dyn Any + Send), location: Option<(&str, u32)>) -> String {
    let msg = payload_text(payload);
    match location {
        Some((file, line)) => format!("PÁNICO en hilo «{thread}»: {msg} ({file}:{line})"),
        None => format!("PÁNICO en hilo «{thread}»: {msg}"),
    }
}

/// Lo que hace el hook: al log siempre; a la ventana si el estado se puede
/// coger AHORA (`try_lock`: si el hilo que revienta tenía el candado, un
/// `lock()` aquí sería un interbloqueo). Los pánicos del audio los gestiona
/// `sound.rs` (solo quedan en el log). Devuelve la línea escrita.
fn on_panic(
    thread: &str,
    payload: &(dyn Any + Send),
    location: Option<(&str, u32)>,
    shared: Option<&SharedState>,
) -> String {
    let text = describe_panic(thread, payload, location);
    line(&text);
    let audio = thread.starts_with("cpal") || thread == "pmp-sound";
    if !audio {
        if let Some(shared) = shared {
            if let Ok(mut s) = shared.try_lock() {
                let where_ = path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "el log".to_owned());
                s.last_error = Some(format!(
                    "Fallo interno en «{thread}»: {} — detalles en {where_}",
                    payload_text(payload)
                ));
            }
        }
    }
    text
}

/// Instala el hook de pánico: lo primero de `main`. No puede entrar en
/// pánico él mismo (sin unwrap, sin lock bloqueante).
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("?").to_owned();
        let location = info.location().map(|l| (l.file().to_owned(), l.line()));
        on_panic(
            &name,
            info.payload(),
            location.as_ref().map(|(f, l)| (f.as_str(), *l)),
            SHARED.get(),
        );
    }));
}

/// Solo tests: silencia el hook de pánico por defecto, para que los tests
/// que provocan pánicos a propósito no llenen la salida.
#[cfg(test)]
pub fn quiet_panics() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| std::panic::set_hook(Box::new(|_| {})));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pepomote-log-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn el_puente_deja_pasar_avisos_y_lo_informativo_de_la_ventana() {
        use logfacade::Level::*;
        assert!(bridged(Error, "mdns_sd"));
        assert!(bridged(Warn, "winit::platform_impl"));
        assert!(bridged(Info, "eframe::native::run"));
        assert!(bridged(Info, "egui_glow::painter"));
        assert!(bridged(Info, "egui_wgpu"), "qué adaptador de Direct3D eligió (Windows sin OpenGL)");
        assert!(!bridged(Info, "wgpu_core::instance"));
        assert!(!bridged(Info, "mdns_sd"));
        assert!(!bridged(Debug, "eframe"));
    }

    #[test]
    fn la_linea_lleva_tiempo_e_hilo() {
        assert_eq!(
            format_line(1_700_000_000, 7, "pmp-control", "hola"),
            "1700000000.007 [pmp-control] hola"
        );
    }

    #[test]
    fn rota_al_superar_el_tope() {
        let d = temp_dir("rota");
        let p = d.join("receptor.log");
        for i in 0..40 {
            append(&p, 1024, &format!("línea {i} {}", "x".repeat(40)));
        }
        let old = d.join("receptor.log.1");
        assert!(old.exists(), "no rotó");
        assert!(std::fs::metadata(&p).unwrap().len() < 1024);
        assert!(std::fs::metadata(&old).unwrap().len() > 1024);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn tail_devuelve_las_ultimas_n() {
        let d = temp_dir("tail");
        let p = d.join("receptor.log");
        for i in 0..10 {
            append(&p, 1 << 20, &format!("l{i}"));
        }
        assert_eq!(tail_of(&p, 3), vec!["l7", "l8", "l9"]);
        assert_eq!(tail_of(&p, 50).len(), 10);
        assert!(tail_of(&d.join("no-existe.log"), 3).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn describe_panic_con_str_string_y_desconocido() {
        let s: &(dyn Any + Send) = &"se rompió";
        assert_eq!(
            describe_panic("pmp-x", s, Some(("src/a.rs", 12))),
            "PÁNICO en hilo «pmp-x»: se rompió (src/a.rs:12)"
        );
        let s: &(dyn Any + Send) = &String::from("otra");
        assert_eq!(describe_panic("main", s, None), "PÁNICO en hilo «main»: otra");
        let s: &(dyn Any + Send) = &42_i32;
        assert!(describe_panic("main", s, None).ends_with("(sin mensaje)"));
    }

    #[test]
    fn el_hook_no_bloquea_con_el_estado_cogido_y_avisa_si_puede() {
        let shared = crate::state::new_shared();
        // candado cogido: el hook no debe esperar (try_lock) ni entrar en pánico
        let guard = shared.lock().unwrap();
        let s: &(dyn Any + Send) = &"boom";
        let text = on_panic("pmp-control", s, None, Some(&shared));
        assert!(text.contains("boom"));
        drop(guard);
        assert!(shared.lock().unwrap().last_error.is_none());
        // libre: avisa en la ventana, salvo los hilos de audio
        on_panic("pmp-control", s, None, Some(&shared));
        let err = shared.lock().unwrap().last_error.clone().unwrap();
        assert!(err.starts_with("Fallo interno en «pmp-control»: boom"));
        shared.lock().unwrap().last_error = None;
        on_panic("cpal_alsa_out", s, None, Some(&shared));
        assert!(shared.lock().unwrap().last_error.is_none());
    }
}
