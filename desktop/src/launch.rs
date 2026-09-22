//! Arranque de la ventana con red de seguridad. Si eframe no puede abrirla
//! (sin EGL/GL usable, backend Wayland roto con X11 sano, biblioteca que
//! falta…), el error queda en receptor.log y, en Linux, el proceso se
//! relanza a sí mismo con render por software y, si hace falta, con X11; si
//! nada funciona, avisa por el escritorio. Lo puro (qué intento toca ahora)
//! se prueba en cualquier SO.

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
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
/// Solo pruebas: el hilo de la ventana se duerme para siempre (lo que haría
/// un `reg.exe` que no vuelve o un candado retenido) en el tercer fotograma,
/// o en el fotograma que diga el valor (`=40`): así se comprueba que el
/// vigilante lo apunta, que `--diag` lo cuenta y que el modo humo sale con 4
/// en vez de quedarse colgado.
pub const ENV_FAKE_UI_HANG: &str = "PEPOMOTE_FAKE_UI_HANG";
/// Sin primer fotograma pasado esto, el modo humo se rinde.
const SMOKE_DEADLINE: Duration = Duration::from_secs(30);
/// Modo humo: la ventana colgada después de pintar sale con 4 pasado esto
/// (después del primer aviso del vigilante, que tiene que quedar en el log).
const SMOKE_STALL: Duration = Duration::from_secs(20);
/// Cuándo apunta el vigilante que la ventana, pintada y a la vista, lleva
/// sin repintar: a los 10 s, a los 60 s y después cada 10 min.
const STALL_FIRST: Duration = Duration::from_secs(10);
const STALL_SECOND: Duration = Duration::from_secs(60);
const STALL_EVERY: Duration = Duration::from_secs(600);
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

/// Qué estaba haciendo el hilo de la ventana la última vez que se supo: la
/// miga que sale en el log (y en `--diag`) cuando deja de repintar. Todo lo
/// que puede esperar en ese hilo se marca antes de llamarlo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Step {
    /// egui: medir, pintar, presentar y atender la ventana (fuera de nuestro código)
    Egui = 0,
    /// el candado del estado compartido
    StateLock = 1,
    /// la foto de los mandos virtuales (`rumble::ui_lines`)
    Rumble = 2,
    /// escribir settings.json
    SaveConfig = 3,
    /// el aviso de versión nueva y la instalación de la actualización
    Updater = 4,
    /// escribir en receptor.log
    Log = 5,
    /// `PEPOMOTE_FAKE_UI_HANG`
    TestHook = 6,
}

impl Step {
    pub fn name(self) -> &'static str {
        match self {
            Step::Egui => "egui (pintar y atender la ventana)",
            Step::StateLock => "el candado del estado",
            Step::Rumble => "la foto de los mandos virtuales",
            Step::SaveConfig => "guardar ajustes",
            Step::Updater => "el aviso de versión nueva",
            Step::Log => "escribir en receptor.log",
            Step::TestHook => "gancho de prueba",
        }
    }

    fn from_u8(v: u8) -> Step {
        match v {
            1 => Step::StateLock,
            2 => Step::Rumble,
            3 => Step::SaveConfig,
            4 => Step::Updater,
            5 => Step::Log,
            6 => Step::TestHook,
            _ => Step::Egui,
        }
    }
}

static UI_STEP: AtomicU8 = AtomicU8::new(0);
/// Instantes del último `update()` y del último «mostrar» tras estar oculta,
/// en ms desde el arranque más uno (0 = nunca).
static LAST_FRAME_MS: AtomicU64 = AtomicU64::new(0);
static SHOWN_MS: AtomicU64 = AtomicU64::new(0);
/// La ventana está escondida en la bandeja: no repinta, y no es un cuelgue.
static WINDOW_HIDDEN: AtomicBool = AtomicBool::new(false);
static EPOCH: OnceLock<Instant> = OnceLock::new();

fn now_ms() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

fn age_of(stamp: u64) -> Option<Duration> {
    (stamp != 0).then(|| Duration::from_millis(now_ms().saturating_sub(stamp)))
}

/// La ventana entra en `update()`: latido, y la miga vuelve a «egui».
pub fn mark_frame() {
    LAST_FRAME_MS.store(now_ms(), Ordering::SeqCst);
    UI_STEP.store(Step::Egui as u8, Ordering::SeqCst);
}

/// Lo que el hilo de la ventana va a hacer ahora (y podría no volver).
pub fn ui_step(step: Step) {
    UI_STEP.store(step as u8, Ordering::SeqCst);
}

pub fn last_step() -> Step {
    Step::from_u8(UI_STEP.load(Ordering::SeqCst))
}

/// Cuánto hace del último `update()` (None: ninguno todavía).
pub fn last_frame_age() -> Option<Duration> {
    age_of(LAST_FRAME_MS.load(Ordering::SeqCst))
}

/// Cerrar a la bandeja / volver a mostrar. Al volver a la vista empieza de
/// cero la cuenta del cuelgue: oculta no ha podido repintar.
pub fn set_window_hidden(hidden: bool) {
    let was_hidden = WINDOW_HIDDEN.swap(hidden, Ordering::SeqCst);
    if was_hidden && !hidden {
        SHOWN_MS.store(now_ms(), Ordering::SeqCst);
    }
}

pub fn window_hidden() -> bool {
    WINDOW_HIDDEN.load(Ordering::SeqCst)
}

/// Cuánto lleva la ventana, ya pintada y a la vista, sin repintar (None:
/// sin primer fotograma, o escondida en la bandeja).
fn stall_age() -> Option<Duration> {
    if !first_frame_done() || window_hidden() {
        return None;
    }
    age_of(LAST_FRAME_MS.load(Ordering::SeqCst).max(SHOWN_MS.load(Ordering::SeqCst)))
}

/// La línea del vigilante cuando la ventana lleva `age` sin repintar.
pub fn stall_line(age: Duration, step: Step) -> String {
    format!("Ventana: sin repintar desde hace {} s · último paso: {}", age.as_secs(), step.name())
}

/// Umbral del aviso número `reported` (0 = el primero): 10 s, 60 s y
/// después cada 10 min, que un cuelgue de horas no llene el log.
pub fn stall_threshold(reported: u32) -> Duration {
    match reported {
        0 => STALL_FIRST,
        1 => STALL_SECOND,
        n => STALL_SECOND + STALL_EVERY * (n - 1),
    }
}

/// El estado de la ventana en una línea: para el log cuando otra copia
/// pide mostrarla y para `--diag` (por el cerrojo de instancia única).
pub fn window_status_line(first_frame: bool, frame_age: Option<Duration>, step: Step, hidden: bool) -> String {
    match (first_frame, frame_age) {
        (_, None) => "la ventana no se ha creado o no ha entrado en su primer fotograma".to_owned(),
        (false, Some(age)) => format!(
            "primer fotograma empezado hace {} s y sin terminar · último paso: {}",
            age.as_secs(),
            step.name()
        ),
        (true, Some(age)) => format!(
            "último fotograma hace {} s · último paso: {} · ventana {}",
            age.as_secs(),
            step.name(),
            if hidden { "oculta (en la bandeja)" } else { "a la vista" }
        ),
    }
}

pub fn window_status() -> String {
    window_status_line(first_frame_done(), last_frame_age(), last_step(), window_hidden())
}

/// En qué fotograma se cuelga la ventana según `PEPOMOTE_FAKE_UI_HANG`:
/// None sin la variable; el tercero (ya con el QR a la vista) con `1` o
/// cualquier cosa que no sea un número mayor; ese número si lo es.
pub fn fake_hang_frame(env: Option<&str>) -> Option<u32> {
    let v = env?.trim();
    Some(v.parse::<u32>().ok().filter(|n| *n > 3).unwrap_or(3))
}

/// Solo pruebas (`PEPOMOTE_FAKE_UI_HANG`): el hilo de la ventana se cuelga
/// a propósito en el fotograma pedido.
pub fn fake_ui_hang_if_requested(frames: u32) {
    if fake_hang_frame(std::env::var(ENV_FAKE_UI_HANG).ok().as_deref()) == Some(frames) {
        ui_step(Step::TestHook);
        crate::log_line!("PEPOMOTE_FAKE_UI_HANG: el hilo de la ventana se cuelga a propósito en el fotograma {frames}");
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}

/// `Some(espera)` en modo humo: "" o "1" = 1500 ms, otro número = esos ms.
pub fn smoke_linger(env: Option<&str>) -> Option<Duration> {
    let v = env?.trim();
    let ms = if v.is_empty() || v == "1" { 1500 } else { v.parse::<u64>().ok()? };
    Some(Duration::from_millis(ms))
}

/// Modo humo: un vigilante que sale con 3 si no llega el primer fotograma
/// y con 4 si la ventana, ya pintada, deja de repintar (la salida limpia
/// con 0 la da `update()`, que en ese caso no vuelve a correr).
pub fn start_smoke_watchdog() {
    if smoke_linger(std::env::var(ENV_SMOKE).ok().as_deref()).is_none() {
        return;
    }
    let _ = crate::threads::spawn_once("smoke-watchdog", || {
        let start = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(250));
            if !first_frame_done() {
                if start.elapsed() >= SMOKE_DEADLINE {
                    crate::log_line!("PEPOMOTE_SMOKE: sin primer fotograma a los {} s, salgo con 3", SMOKE_DEADLINE.as_secs());
                    std::process::exit(3);
                }
                continue;
            }
            if let Some(age) = stall_age() {
                if age >= SMOKE_STALL {
                    crate::log_line!(
                        "PEPOMOTE_SMOKE: la ventana no repinta desde hace {} s (último paso: {}), salgo con 4",
                        age.as_secs(),
                        last_step().name()
                    );
                    std::process::exit(4);
                }
            }
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

/// Vigilante siempre activo. Antes del primer fotograma: si la ventana no
/// ha pintado a los 15 y a los 60 s, lo apunta en el log junto con lo que
/// pueda estar frenándola (una llamada al driver del mando virtual sin
/// contestar: la ventana negra de la 1.12). Después: si la ventana, a la
/// vista, deja de repintar, apunta cuánto lleva y en qué paso se quedó
/// (a los 10 s, a los 60 s y luego cada 10 min), y cuándo vuelve. Solo
/// apunta: sin diálogos ni relanzamientos, que una red que funciona no se
/// mata por una ventana que no pinta. Un cuelgue así nunca llega a
/// `finish`: sin esto no dejaba ni rastro.
pub fn start_paint_watchdog() {
    let _ = crate::threads::spawn_once("paint-watchdog", || {
        let start = Instant::now();
        let mut paint_checks = PAINT_CHECKS.iter().copied().peekable();
        let mut reported = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(250));
            if !first_frame_done() {
                if let Some(&at) = paint_checks.peek() {
                    if start.elapsed() >= at {
                        crate::log_line!("{}", paint_watchdog_line(at, crate::rumble::stuck_note()));
                        paint_checks.next();
                    }
                }
                continue;
            }
            if reported > 0 && last_frame_age().is_some_and(|age| age < STALL_FIRST) {
                crate::log_line!("Ventana: vuelve a repintar");
                reported = 0;
            }
            if let Some(age) = stall_age() {
                if age >= stall_threshold(reported) {
                    crate::log_line!("{}", stall_line(age, last_step()));
                    reported += 1;
                }
            }
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
    fn la_linea_del_cuelgue_y_sus_umbrales() {
        assert_eq!(
            stall_line(Duration::from_secs(12), Step::StateLock),
            "Ventana: sin repintar desde hace 12 s · último paso: el candado del estado"
        );
        assert_eq!(stall_threshold(0), Duration::from_secs(10));
        assert_eq!(stall_threshold(1), Duration::from_secs(60));
        assert_eq!(stall_threshold(2), Duration::from_secs(660));
        assert_eq!(stall_threshold(3), Duration::from_secs(1260));
        assert!(SMOKE_STALL > STALL_FIRST, "el humo no se rinde antes de que el vigilante lo apunte");
        for v in 0..=7u8 {
            assert_eq!(Step::from_u8(v) as u8, if v <= 6 { v } else { 0 });
        }
    }

    #[test]
    fn el_gancho_del_cuelgue_y_su_fotograma() {
        assert_eq!(fake_hang_frame(None), None);
        assert_eq!(fake_hang_frame(Some("1")), Some(3));
        assert_eq!(fake_hang_frame(Some("")), Some(3));
        assert_eq!(fake_hang_frame(Some("2")), Some(3), "antes del tercero no hay QR que ver");
        assert_eq!(fake_hang_frame(Some(" 40 ")), Some(40));
        assert_eq!(fake_hang_frame(Some("abc")), Some(3));
    }

    #[test]
    fn el_estado_de_la_ventana_en_una_linea() {
        assert_eq!(
            window_status_line(false, None, Step::Egui, false),
            "la ventana no se ha creado o no ha entrado en su primer fotograma"
        );
        assert_eq!(
            window_status_line(false, Some(Duration::from_secs(4)), Step::TestHook, false),
            "primer fotograma empezado hace 4 s y sin terminar · último paso: gancho de prueba"
        );
        assert_eq!(
            window_status_line(true, Some(Duration::ZERO), Step::Egui, false),
            "último fotograma hace 0 s · último paso: egui (pintar y atender la ventana) · ventana a la vista"
        );
        assert_eq!(
            window_status_line(true, Some(Duration::from_secs(90)), Step::SaveConfig, true),
            "último fotograma hace 90 s · último paso: guardar ajustes · ventana oculta (en la bandeja)"
        );
    }

    #[test]
    fn el_latido_la_miga_y_la_bandeja() {
        mark_frame();
        assert_eq!(last_step(), Step::Egui);
        ui_step(Step::Rumble);
        assert_eq!(last_step(), Step::Rumble);
        assert!(last_frame_age().unwrap() < Duration::from_secs(1));
        set_window_hidden(true);
        assert!(window_hidden());
        set_window_hidden(false);
        assert!(!window_hidden());
        assert!(window_status().contains("último paso: la foto de los mandos virtuales"));
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
