//! Arranque de la ventana con red de seguridad. Si eframe no puede abrirla
//! (sin EGL/GL usable, backend Wayland roto con X11 sano, biblioteca que
//! falta…), el error queda en receptor.log y, en Linux, el proceso se
//! relanza a sí mismo con render por software y, si hace falta, con X11; si
//! nada funciona, avisa por el escritorio. Lo puro (qué intento toca ahora)
//! se prueba en cualquier SO.

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Intento en curso (1..=3); lo pone el propio receptor al relanzarse.
pub const ENV_ATTEMPT: &str = "PEPOMOTE_UI_ATTEMPT";
/// Backend de ventana forzado: `x11` | `wayland`; vacío = el que elija winit.
pub const ENV_BACKEND: &str = "PEPOMOTE_UI_BACKEND";
/// Desactiva los relanzamientos (para ver el error tal cual, o pruebas).
#[cfg(target_os = "linux")]
pub const ENV_NO_FALLBACK: &str = "PEPOMOTE_NO_UI_FALLBACK";
/// Modo humo: salir con 0 tantos ms después del primer fotograma ("" o "1"
/// = 1500). Si a los 30 s no hay fotograma, salir con 3.
pub const ENV_SMOKE: &str = "PEPOMOTE_SMOKE";
/// Solo CI: el intento 1 falla a propósito para probar el relanzamiento.
pub const ENV_SMOKE_FAIL_FIRST: &str = "PEPOMOTE_SMOKE_FAIL_FIRST";
/// Mesa: render por software (llvmpipe) en vez de la GPU.
pub const ENV_SOFTWARE_GL: &str = "LIBGL_ALWAYS_SOFTWARE";
/// Sin primer fotograma pasado esto, el modo humo se rinde.
const SMOKE_DEADLINE: Duration = Duration::from_secs(30);
/// Cuándo apunta el vigilante del primer fotograma que la ventana sigue sin
/// pintar (siempre activo, en todos los sistemas).
const PAINT_CHECKS: [Duration; 2] = [Duration::from_secs(15), Duration::from_secs(60)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    Auto,
    X11,
    Wayland,
}

impl Backend {
    pub fn parse(s: Option<&str>) -> Backend {
        match s.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            Some("x11") => Backend::X11,
            Some("wayland") => Backend::Wayland,
            _ => Backend::Auto,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Backend::Auto => "automático",
            Backend::X11 => "X11",
            Backend::Wayland => "Wayland",
        }
    }
}

/// Con qué se está intentando abrir la ventana.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Attempt {
    pub n: u8,
    pub software_gl: bool,
    pub backend: Backend,
}

/// El intento actual, leído del entorno (el relanzamiento lo deja puesto).
pub fn current_attempt(env: impl Fn(&str) -> Option<String>) -> Attempt {
    let n = env(ENV_ATTEMPT)
        .and_then(|v| v.trim().parse::<u8>().ok())
        .filter(|n| (1..=3).contains(n))
        .unwrap_or(1);
    let software_gl = env(ENV_SOFTWARE_GL).is_some_and(|v| {
        let v = v.trim();
        !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
    });
    let backend = Backend::parse(env(ENV_BACKEND).as_deref());
    Attempt { n, software_gl, backend }
}

static ATTEMPT: OnceLock<Attempt> = OnceLock::new();

pub fn attempt() -> &'static Attempt {
    ATTEMPT.get_or_init(|| current_attempt(|k| std::env::var(k).ok()))
}

/// Qué probar después de que `cur` haya fallado antes de pintar: primero
/// render por software con el mismo backend; después X11 (con software),
/// solo si la sesión tiene Wayland y también X11 (XWayland) y no se forzó
/// un backend. Tres intentos como mucho; luego, nada.
#[cfg(any(target_os = "linux", test))]
pub fn next_attempt(cur: &Attempt, wayland: bool, x11: bool) -> Option<Attempt> {
    if cur.n >= 3 {
        return None;
    }
    if !cur.software_gl {
        return Some(Attempt { n: cur.n + 1, software_gl: true, backend: cur.backend });
    }
    if cur.backend == Backend::Auto && wayland && x11 {
        return Some(Attempt { n: cur.n + 1, software_gl: true, backend: Backend::X11 });
    }
    None
}

/// Variables de entorno que describen `a` para el proceso relanzado.
#[cfg(any(target_os = "linux", test))]
pub fn env_for(a: &Attempt) -> Vec<(&'static str, String)> {
    let mut v = vec![(ENV_ATTEMPT, a.n.to_string())];
    if a.software_gl {
        v.push((ENV_SOFTWARE_GL, "1".to_owned()));
    }
    match a.backend {
        Backend::Auto => {}
        Backend::X11 => v.push((ENV_BACKEND, "x11".to_owned())),
        Backend::Wayland => v.push((ENV_BACKEND, "wayland".to_owned())),
    }
    v
}

pub fn describe(a: &Attempt) -> String {
    format!(
        "intento {} · backend {} · GL {}",
        a.n,
        a.backend.name(),
        if a.software_gl { "por software" } else { "de la GPU" }
    )
}

static FIRST_FRAME: AtomicBool = AtomicBool::new(false);

/// La ventana ha pintado: a partir de aquí un fallo ya no se relanza.
pub fn mark_first_frame() {
    FIRST_FRAME.store(true, Ordering::SeqCst);
}

pub fn first_frame_done() -> bool {
    FIRST_FRAME.load(Ordering::SeqCst)
}

/// `Some(espera)` en modo humo: "" o "1" = 1500 ms, otro número = esos ms.
pub fn smoke_linger(env: Option<&str>) -> Option<Duration> {
    let v = env?.trim();
    let ms = if v.is_empty() || v == "1" { 1500 } else { v.parse::<u64>().ok()? };
    Some(Duration::from_millis(ms))
}

/// Modo humo: un vigilante que sale con 3 si no llega el primer fotograma.
pub fn start_smoke_watchdog() {
    if smoke_linger(std::env::var(ENV_SMOKE).ok().as_deref()).is_none() {
        return;
    }
    let _ = crate::threads::spawn_once("smoke-watchdog", || {
        std::thread::sleep(SMOKE_DEADLINE);
        if !first_frame_done() {
            crate::log_line!("PEPOMOTE_SMOKE: sin primer fotograma a los {} s, salgo con 3", SMOKE_DEADLINE.as_secs());
            std::process::exit(3);
        }
    });
}

/// La línea del vigilante del primer fotograma, con lo que pueda estar
/// frenando la ventana si se sabe.
pub fn paint_watchdog_line(at: Duration, stuck: Option<String>) -> String {
    let mut s = format!("Ventana: sin primer fotograma a los {} s", at.as_secs());
    if let Some(n) = stuck {
        s.push_str(" · ");
        s.push_str(&n);
    }
    s
}

/// Vigilante siempre activo: si la ventana no ha pintado a los 15 y a los
/// 60 s, lo apunta en el log junto con lo que pueda estar frenándola (una
/// llamada al driver del mando virtual sin contestar: la ventana negra de la
/// 1.12). Solo apunta: sin diálogos ni relanzamientos, que una red que
/// funciona no se mata por una ventana que no pinta. Un cuelgue así nunca
/// llega a `finish`: sin esto no dejaba ni rastro.
pub fn start_paint_watchdog() {
    let _ = crate::threads::spawn_once("paint-watchdog", || {
        let start = Instant::now();
        for at in PAINT_CHECKS {
            while start.elapsed() < at {
                if first_frame_done() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            if first_frame_done() {
                return;
            }
            crate::log_line!("{}", paint_watchdog_line(at, crate::rumble::stuck_note()));
        }
    });
}

/// Cómo acabó `eframe::run_native`.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// El usuario cerró la ventana (o el modo humo terminó).
    Closed,
    /// Falló sin llegar a pintar: candidato a relanzamiento.
    FailedBeforeFrame(String),
    /// Falló con la ventana ya pintada (contexto GL perdido, compositor
    /// caído): nunca se relanza.
    FailedAfterFrame(String),
}

/// `result` = lo que devolvió `catch_unwind(run_native)`.
pub fn classify(result: Result<Result<(), String>, Box<dyn Any + Send>>, first_frame: bool) -> Outcome {
    let why = match result {
        Ok(Ok(())) => return Outcome::Closed,
        Ok(Err(e)) => e,
        Err(payload) => format!("pánico: {}", crate::log::payload_text(&*payload)),
    };
    if first_frame {
        Outcome::FailedAfterFrame(why)
    } else {
        Outcome::FailedBeforeFrame(why)
    }
}

/// Deja el desenlace en el log y devuelve el código de salida del proceso;
/// en Linux, antes de rendirse, relanza (no vuelve si el `exec` funciona).
pub fn finish(outcome: Outcome, attempt: &Attempt) -> i32 {
    match outcome {
        Outcome::Closed => {
            crate::log_line!("Ventana cerrada: salgo");
            0
        }
        Outcome::FailedAfterFrame(why) => {
            crate::log_line!("Ventana: {} falló después de pintar: {why}", describe(attempt));
            1
        }
        Outcome::FailedBeforeFrame(why) => {
            crate::log_line!("Ventana: {} falló antes de pintar: {why}", describe(attempt));
            fallback_or_notify(attempt)
        }
    }
}

#[cfg(target_os = "linux")]
fn fallback_or_notify(attempt: &Attempt) -> i32 {
    if std::env::var_os(ENV_NO_FALLBACK).is_none() {
        let set = |k: &str| std::env::var(k).is_ok_and(|v| !v.trim().is_empty());
        if let Some(next) = next_attempt(attempt, set("WAYLAND_DISPLAY"), set("DISPLAY")) {
            crate::log_line!("Ventana: relanzo ({})", describe(&next));
            let e = reexec(&next);
            crate::log_line!("Ventana: no se pudo relanzar: {e}");
        }
    }
    let log = crate::log::path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "receptor.log".to_owned());
    notify_failure(&crate::tr!("ui.start_failed_title"), &crate::tr!("ui.start_failed_body", log));
    2
}

#[cfg(not(target_os = "linux"))]
fn fallback_or_notify(_attempt: &Attempt) -> i32 {
    1
}

/// Fuerza el backend de winit (por defecto elige Wayland siempre que haya
/// `WAYLAND_DISPLAY`, aunque falle y X11 funcione).
#[cfg(target_os = "linux")]
pub fn event_loop_hook(backend: Backend) -> Option<eframe::EventLoopBuilderHook> {
    match backend {
        Backend::Auto => None,
        Backend::X11 => Some(Box::new(|b: &mut eframe::EventLoopBuilder<eframe::UserEvent>| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            b.with_x11();
        })),
        Backend::Wayland => Some(Box::new(|b: &mut eframe::EventLoopBuilder<eframe::UserEvent>| {
            use winit::platform::wayland::EventLoopBuilderExtWayland;
            b.with_wayland();
        })),
    }
}

/// Sustituye este proceso por otro igual con el entorno de `a`. Con `exec`
/// (nunca spawn + salir): dentro de un AppImage el runtime padre mantiene el
/// montaje mientras viva este PID. Los sockets y uinput llevan CLOEXEC y se
/// liberan solos. Solo vuelve si el `exec` falla.
#[cfg(target_os = "linux")]
pub fn reexec(a: &Attempt) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return e,
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(std::env::args_os().skip(1));
    for (k, v) in env_for(a) {
        cmd.env(k, v);
    }
    cmd.exec()
}

/// Último recurso: stderr, una notificación de escritorio (notify-send) y,
/// si no la hay, un diálogo (zenity). Todo best effort.
#[cfg(target_os = "linux")]
pub fn notify_failure(title: &str, body: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let _ = writeln!(std::io::stderr(), "{title}\n{body}");
    fn quiet(c: &mut Command) -> &mut Command {
        c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
    }
    let notified = quiet(Command::new("notify-send").args(["-u", "critical", "-a", "PepoMote", title, body]))
        .status()
        .is_ok_and(|s| s.success());
    if !notified {
        let _ = quiet(Command::new("zenity").args(["--error", "--title", title, "--text", body])).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| (*v).to_owned())
    }

    #[test]
    fn el_intento_se_lee_del_entorno() {
        assert_eq!(current_attempt(env_of(&[])), Attempt { n: 1, software_gl: false, backend: Backend::Auto });
        let a = current_attempt(env_of(&[(ENV_ATTEMPT, "2"), (ENV_SOFTWARE_GL, "1")]));
        assert_eq!(a, Attempt { n: 2, software_gl: true, backend: Backend::Auto });
        let a = current_attempt(env_of(&[(ENV_ATTEMPT, "3"), (ENV_BACKEND, "X11"), (ENV_SOFTWARE_GL, "true")]));
        assert_eq!(a, Attempt { n: 3, software_gl: true, backend: Backend::X11 });
        let a = current_attempt(env_of(&[(ENV_ATTEMPT, "9"), (ENV_SOFTWARE_GL, "0"), (ENV_BACKEND, "raro")]));
        assert_eq!(a, Attempt { n: 1, software_gl: false, backend: Backend::Auto });
    }

    #[test]
    fn la_cadena_de_relanzamientos() {
        let a1 = Attempt { n: 1, software_gl: false, backend: Backend::Auto };
        let a2 = next_attempt(&a1, true, true).unwrap();
        assert_eq!(a2, Attempt { n: 2, software_gl: true, backend: Backend::Auto });
        let a3 = next_attempt(&a2, true, true).unwrap();
        assert_eq!(a3, Attempt { n: 3, software_gl: true, backend: Backend::X11 });
        assert_eq!(next_attempt(&a3, true, true), None);
        // sesión solo X11 (o solo Wayland): tras el software no hay más
        assert_eq!(next_attempt(&a2, false, true), None);
        assert_eq!(next_attempt(&a2, true, false), None);
        // backend forzado por el usuario: solo el software, sin cambiar de backend
        let f = Attempt { n: 1, software_gl: false, backend: Backend::X11 };
        let f2 = next_attempt(&f, true, true).unwrap();
        assert_eq!(f2, Attempt { n: 2, software_gl: true, backend: Backend::X11 });
        assert_eq!(next_attempt(&f2, true, true), None);
        // ya venía con software: directo a X11 si se puede
        let s = Attempt { n: 1, software_gl: true, backend: Backend::Auto };
        assert_eq!(next_attempt(&s, true, true), Some(Attempt { n: 2, software_gl: true, backend: Backend::X11 }));
        assert_eq!(next_attempt(&s, false, true), None);
    }

    #[test]
    fn el_entorno_del_relanzamiento_y_su_descripcion() {
        let a = Attempt { n: 3, software_gl: true, backend: Backend::X11 };
        assert_eq!(
            env_for(&a),
            vec![(ENV_ATTEMPT, "3".to_owned()), (ENV_SOFTWARE_GL, "1".to_owned()), (ENV_BACKEND, "x11".to_owned())]
        );
        assert_eq!(env_for(&Attempt { n: 2, software_gl: true, backend: Backend::Auto }).len(), 2);
        assert_eq!(describe(&a), "intento 3 · backend X11 · GL por software");
        assert_eq!(
            describe(&Attempt { n: 1, software_gl: false, backend: Backend::Auto }),
            "intento 1 · backend automático · GL de la GPU"
        );
    }

    #[test]
    fn el_modo_humo_y_su_espera() {
        assert_eq!(smoke_linger(None), None);
        assert_eq!(smoke_linger(Some("")), Some(Duration::from_millis(1500)));
        assert_eq!(smoke_linger(Some("1")), Some(Duration::from_millis(1500)));
        assert_eq!(smoke_linger(Some("800")), Some(Duration::from_millis(800)));
        assert_eq!(smoke_linger(Some("abc")), None);
    }

    #[test]
    fn la_linea_del_vigilante_del_primer_fotograma() {
        assert_eq!(paint_watchdog_line(Duration::from_secs(15), None), "Ventana: sin primer fotograma a los 15 s");
        assert_eq!(
            paint_watchdog_line(Duration::from_secs(60), Some("el driver del mando virtual no contesta".into())),
            "Ventana: sin primer fotograma a los 60 s · el driver del mando virtual no contesta"
        );
        assert_eq!(PAINT_CHECKS[0], Duration::from_secs(15));
        assert!(PAINT_CHECKS[1] > PAINT_CHECKS[0]);
    }

    #[test]
    fn el_desenlace_segun_haya_pintado_o_no() {
        assert_eq!(classify(Ok(Ok(())), false), Outcome::Closed);
        assert_eq!(classify(Ok(Err("sin EGL".into())), false), Outcome::FailedBeforeFrame("sin EGL".into()));
        assert_eq!(classify(Ok(Err("GL perdido".into())), true), Outcome::FailedAfterFrame("GL perdido".into()));
        let payload: Box<dyn Any + Send> = Box::new("failed to find a matching configuration");
        assert_eq!(
            classify(Err(payload), false),
            Outcome::FailedBeforeFrame("pánico: failed to find a matching configuration".into())
        );
    }
}
