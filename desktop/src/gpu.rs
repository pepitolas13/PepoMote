//! Windows: con qué pinta la ventana del receptor.
//!
//! OpenGL (glow) es lo de siempre. Direct3D 12 (wgpu; con WARP, el
//! Direct3D por CPU que trae Windows 10 y 11, si no hay GPU) es el rescate
//! para los PCs donde OpenGL no sirve: sin driver de la gráfica solo existe
//! el OpenGL 1.1 de Microsoft y eframe no pinta («egui_glow requires opengl
//! 2.0+»: el receptor se cerraba sin decir nada), y un driver malo puede
//! colgarse o reventar dentro de OpenGL, cosa que no se puede cortar desde
//! el mismo proceso.
//!
//! Por eso OpenGL se prueba FUERA y ANTES: `PepoMote.exe --probe-window`
//! (la sonda) crea la ventana invisible con OpenGL, pasa por lo que se
//! cuelga o revienta en un driver malo (formato de píxel, contexto, carga de
//! GL, compilación de shaders) y sale antes del primer fotograma, sin
//! enseñar nada, contestando una línea. El receptor la lanza en paralelo con
//! su arranque y, si no contesta en `PROBE_BUDGET`, pinta con Direct3D. Lo
//! que salió se recuerda por la huella de la gráfica (tarjetas, su driver y
//! la versión de PepoMote): con la misma huella no hay sonda. Una marca en
//! settings.json delata el arranque que no llegó a pintar (colgado o
//! cerrado a la fuerza), y la copia colgada deja el sitio a la siguiente
//! (ver `singleton.rs`).
#![cfg(windows)]

use crate::state::{LockTolerant, SharedState, WindowRenderer};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Argumento del modo sonda (solo como primer argumento).
pub const PROBE_ARG: &str = "--probe-window";
/// Lo que se espera a la sonda desde que arranca (lo normal: 0,3-1 s,
/// solapados con el resto del arranque).
pub const PROBE_BUDGET: Duration = Duration::from_secs(3);
/// Cuántas huellas se recuerdan (el escritorio remoto cambia las tarjetas).
const REMEMBERED: usize = 4;
/// La copia colgada deja el sitio a otra como pronto a los 5 s de empezar
/// su intento de ventana (y nunca antes del doble de lo que tardó la sonda).
const HANDOVER_MIN: Duration = Duration::from_secs(5);
/// Forzar el renderer (soporte y pruebas): `opengl` o `direct3d`, sin
/// sonda, sin recuerdo y sin cadena.
pub const ENV_RENDERER: &str = "PEPOMOTE_UI_RENDERER";
/// Pruebas: sin sonda (OpenGL y, si falla, Direct3D en el mismo proceso).
pub const ENV_NO_PROBE: &str = "PEPOMOTE_NO_WINDOW_PROBE";
/// Pruebas: el intento OpenGL pide el OpenGL sin aceleración de Windows, el
/// 1.1 de Microsoft: exactamente lo que tiene un PC sin driver.
pub const ENV_FAKE_NO_OPENGL: &str = "PEPOMOTE_FAKE_NO_OPENGL";
/// Pruebas: Direct3D sin ningún backend (falla al crear la superficie).
pub const ENV_FAKE_NO_DIRECT3D: &str = "PEPOMOTE_FAKE_NO_DIRECT3D";
/// Pruebas: `1` cuelga la sonda dentro de OpenGL; `ventana`, la ventana de
/// verdad en su creador (con OpenGL ya creado y antes del primer fotograma).
pub const ENV_FAKE_GL_HANG: &str = "PEPOMOTE_FAKE_GL_HANG";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renderer {
    OpenGl,
    Direct3d,
}

impl Renderer {
    /// Cómo se guarda en settings.json.
    pub fn key(self) -> &'static str {
        match self {
            Renderer::OpenGl => "opengl",
            Renderer::Direct3d => "direct3d",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Renderer::OpenGl => "OpenGL",
            Renderer::Direct3d => "Direct3D 12",
        }
    }

    pub fn parse(s: &str) -> Option<Renderer> {
        match s.trim().to_ascii_lowercase().as_str() {
            "opengl" | "gl" | "glow" => Some(Renderer::OpenGl),
            "direct3d" | "d3d" | "d3d12" | "dx12" | "wgpu" => Some(Renderer::Direct3d),
            _ => None,
        }
    }
}

/// Los intentos cuando se sabe con qué pinta esta gráfica: con OpenGL, y si
/// aun así falla antes de pintar, Direct3D; con Direct3D, solo Direct3D (un
/// OpenGL que falló o se colgó no se vuelve a tocar).
pub fn chain_after(r: Renderer) -> Vec<Renderer> {
    match r {
        Renderer::OpenGl => vec![Renderer::OpenGl, Renderer::Direct3d],
        Renderer::Direct3d => vec![Renderer::Direct3d],
    }
}

/// Qué hacer al arrancar, antes de saber nada de la sonda.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Start {
    /// Cadena decidida sin sonda; `remember` = lo que hay que apuntar para
    /// esta huella; `why` = el motivo, para el log.
    Fixed { chain: Vec<Renderer>, remember: Option<Renderer>, why: String },
    /// Hace falta la sonda.
    Probe,
}

/// La decisión pura: variable de entorno, copia colgada que dejó el sitio,
/// marca del arranque anterior, recuerdo de esta huella y, si nada lo
/// decide, la sonda.
pub fn decide_start(
    forced: Option<Renderer>,
    took_over: bool,
    pending: Option<Renderer>,
    remembered: Option<Renderer>,
    probe_disabled: bool,
) -> Start {
    if let Some(r) = forced {
        return Start::Fixed { chain: vec![r], remember: None, why: format!("forzado con {ENV_RENDERER}") };
    }
    if took_over {
        return Start::Fixed {
            chain: vec![Renderer::Direct3d],
            remember: Some(Renderer::Direct3d),
            why: "la copia anterior se colgó antes de pintar con OpenGL".to_owned(),
        };
    }
    match pending {
        Some(Renderer::OpenGl) => {
            return Start::Fixed {
                chain: vec![Renderer::Direct3d],
                remember: Some(Renderer::Direct3d),
                why: "el arranque anterior no llegó a pintar con OpenGL".to_owned(),
            }
        }
        Some(Renderer::Direct3d) => {
            return Start::Fixed {
                chain: vec![Renderer::Direct3d],
                remember: None,
                why: "el arranque anterior no llegó a pintar con Direct3D 12; se vuelve a intentar".to_owned(),
            }
        }
        None => {}
    }
    if let Some(r) = remembered {
        return Start::Fixed { chain: chain_after(r), remember: None, why: format!("recordado para esta gráfica: {}", r.name()) };
    }
    if probe_disabled {
        return Start::Fixed { chain: chain_after(Renderer::OpenGl), remember: None, why: format!("sin sonda ({ENV_NO_PROBE})") };
    }
    Start::Probe
}

/// Lo que dijo (o no dijo) la sonda.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeResult {
    /// OpenGL bien: `GL_RENDERER · GL_VERSION`.
    Ok(String),
    /// OpenGL no sirve (el OpenGL 1.1, un pánico, un driver que revienta).
    Failed(String),
    /// No contestó a tiempo: se la mata.
    TimedOut,
    /// No se pudo lanzar (se sigue como antes: OpenGL y luego Direct3D).
    NotRun(String),
}

/// La línea de la sonda: `OK <info>` o `ERR <motivo>`.
pub fn parse_probe_line(line: &str) -> ProbeResult {
    let line = line.trim();
    if line == "OK" || line.starts_with("OK ") {
        ProbeResult::Ok(line[2..].trim().to_owned())
    } else if line == "ERR" || line.starts_with("ERR ") {
        ProbeResult::Failed(line[3..].trim().to_owned())
    } else {
        ProbeResult::Failed(format!("respuesta que no se entiende: {line:?}"))
    }
}

/// Cadena y recuerdo según la sonda.
pub fn after_probe(result: &ProbeResult) -> (Vec<Renderer>, Option<Renderer>) {
    match result {
        ProbeResult::Ok(_) => (chain_after(Renderer::OpenGl), Some(Renderer::OpenGl)),
        ProbeResult::Failed(_) | ProbeResult::TimedOut => (chain_after(Renderer::Direct3d), Some(Renderer::Direct3d)),
        ProbeResult::NotRun(_) => (chain_after(Renderer::OpenGl), None),
    }
}

/// Lo recordado para una huella.
pub fn recall(list: &[WindowRenderer], fingerprint: &str) -> Option<Renderer> {
    list.iter().rev().find(|e| e.fingerprint == fingerprint).and_then(|e| Renderer::parse(&e.renderer))
}

/// Apunta el renderer de una huella (la más reciente, al final; como mucho
/// `REMEMBERED`). Devuelve si cambió algo.
pub fn remember(list: &mut Vec<WindowRenderer>, fingerprint: &str, r: Renderer) -> bool {
    if list.last().is_some_and(|e| e.fingerprint == fingerprint && e.renderer == r.key()) {
        return false;
    }
    list.retain(|e| e.fingerprint != fingerprint);
    list.push(WindowRenderer { fingerprint: fingerprint.to_owned(), renderer: r.key().to_owned() });
    while list.len() > REMEMBERED {
        list.remove(0);
    }
    true
}

/// ¿Deja esta copia el sitio a la que acaba de abrirse? Solo con OpenGL (un
/// Direct3D colgado no tiene a dónde ir), sin primer fotograma, con el hilo
/// de la ventana dentro de eframe (no en una espera nuestra, que el vigilante
/// ya cuenta), fuera del arranque de comprobación tras una actualización
/// (ahí manda el actualizador) y con su intento empezado hace al menos
/// `HANDOVER_MIN` y el doble de lo que tardó la sonda: una ventana lenta pero
/// sana no se sacrifica.
pub fn handover_allowed(
    current: Option<Renderer>,
    first_frame: bool,
    attempt_age: Option<Duration>,
    probe_took: Duration,
    ui_in_egui: bool,
    update_health: bool,
) -> bool {
    let Some(age) = attempt_age else {
        return false;
    };
    current == Some(Renderer::OpenGl)
        && !first_frame
        && ui_in_egui
        && !update_health
        && age >= HANDOVER_MIN.max(probe_took * 2)
}

// ------------------------------------------------------------------------
// Huella de la gráfica

/// Una tarjeta gráfica tal como la ve Windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Adapter {
    pub name: String,
    pub id: String,
    pub driver: Option<String>,
    /// El driver trae su propio OpenGL (`OpenGLDriverName` en su clave).
    /// Sin él solo está el OpenGL 1.1 de Windows.
    pub vendor_gl: bool,
}

/// `\Registry\Machine\System\...` → `System\...` (para HKEY_LOCAL_MACHINE).
pub fn registry_subkey(device_key: &str) -> Option<&str> {
    const PREFIX: &str = "\\registry\\machine\\";
    let head = device_key.get(..PREFIX.len())?;
    head.eq_ignore_ascii_case(PREFIX).then(|| &device_key[PREFIX.len()..])
}

/// La huella: tarjetas (nombre, id y versión del driver, sin repetir) y la
/// versión de PepoMote.
pub fn fingerprint_of(adapters: &[Adapter]) -> String {
    let mut parts: Vec<String> = adapters
        .iter()
        .map(|a| format!("{}|{}|{}", a.name, a.id, a.driver.as_deref().unwrap_or("?")))
        .collect();
    parts.sort();
    parts.dedup();
    let cards = if parts.is_empty() { "sin tarjetas".to_owned() } else { parts.join(" + ") };
    format!("{cards} · PepoMote {}", env!("CARGO_PKG_VERSION"))
}

/// Las tarjetas en una línea, para el log y `--diag`.
pub fn describe_adapters(adapters: &[Adapter]) -> String {
    if adapters.is_empty() {
        return "ninguna tarjeta a la vista".to_owned();
    }
    adapters
        .iter()
        .map(|a| {
            format!(
                "{} (driver {}; OpenGL del fabricante: {})",
                a.name,
                a.driver.as_deref().unwrap_or("?"),
                if a.vendor_gl { "sí" } else { "no, solo el OpenGL 1.1 de Windows" }
            )
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Las tarjetas de este PC (`EnumDisplayDevicesW` da una por pantalla: se
/// quitan las repetidas y las de réplica).
pub fn adapters() -> Vec<Adapter> {
    use windows::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_MIRRORING_DRIVER};
    let wide = |s: &[u16]| {
        let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        String::from_utf16_lossy(&s[..n]).trim().to_owned()
    };
    let mut out: Vec<Adapter> = Vec::new();
    for i in 0..32u32 {
        let mut d = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        if !unsafe { EnumDisplayDevicesW(windows::core::PCWSTR::null(), i, &mut d, 0) }.as_bool() {
            break;
        }
        if d.StateFlags & DISPLAY_DEVICE_MIRRORING_DRIVER != 0 {
            continue;
        }
        let name = wide(&d.DeviceString);
        let id = wide(&d.DeviceID);
        if name.is_empty() || out.iter().any(|a| a.name == name && a.id == id) {
            continue;
        }
        let key = wide(&d.DeviceKey);
        let (driver, vendor_gl) = match registry_subkey(&key) {
            Some(sub) => (reg_string(sub, "DriverVersion"), reg_exists(sub, "OpenGLDriverName")),
            None => (None, false),
        };
        out.push(Adapter { name, id, driver, vendor_gl });
    }
    out
}

fn wide_z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_string(subkey: &str, value: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let (sub, val) = (wide_z(subkey), wide_z(value));
    let mut buf = [0u16; 256];
    let mut len = (buf.len() * 2) as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(sub.as_ptr()),
            PCWSTR(val.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            Some(&mut len),
        )
    }
    .is_ok();
    ok.then(|| {
        let n = (len as usize / 2).min(buf.len());
        String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').trim().to_owned()
    })
    .filter(|s| !s.is_empty())
}

fn reg_exists(subkey: &str, value: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_ANY};
    let (sub, val) = (wide_z(subkey), wide_z(value));
    let mut len = 0u32;
    unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, PCWSTR(sub.as_ptr()), PCWSTR(val.as_ptr()), RRF_RT_ANY, None, None, Some(&mut len)) }
        .is_ok()
}

// ------------------------------------------------------------------------
// La sonda (proceso hijo)

/// ¿Es este proceso la sonda? (`PepoMote.exe --probe-window`)
pub fn probe_from_args() -> bool {
    std::env::args_os().nth(1).is_some_and(|a| a == PROBE_ARG)
}

/// El modo sonda: abre OpenGL como la ventana de verdad, contesta una línea
/// por stdout y termina. Nunca vuelve.
pub fn run_probe() -> ! {
    // Solo pruebas: un driver que se cuelga al abrir OpenGL. Antes de tocar
    // OpenGL, para que valga también en un Windows sin GPU
    if std::env::var(ENV_FAKE_GL_HANG).is_ok_and(|v| v.trim() == "1") {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([460.0, 640.0])
        .with_min_inner_size([360.0, 480.0])
        .with_title("PepoMote sonda");
    let options = native_options(Renderer::OpenGl, viewport);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eframe::run_native(
            "PepoMote sonda",
            options,
            Box::new(|cc| {
                // Aquí eframe ya tiene formato de píxel, contexto, GL cargado
                // y los shaders compilados, y la ventana sigue invisible (la
                // enseña tras el primer fotograma, que la sonda no pinta)
                let info = gl_info(cc).unwrap_or_else(|| "sin datos".to_owned());
                answer(&format!("OK {info}"))
            }),
        )
    }));
    match result {
        Ok(Ok(())) => answer("ERR la ventana de prueba se cerró sin llegar a OpenGL"),
        Ok(Err(e)) => answer(&format!("ERR {e}")),
        Err(p) => answer(&format!("ERR pánico: {}", crate::log::payload_text(&*p))),
    }
}

/// Una línea por stdout y fuera, sin pasar por las descargas de DLL del
/// driver (una salida normal puede colgarse ahí con un driver malo).
fn answer(line: &str) -> ! {
    let one = line.replace(['\r', '\n'], " ");
    {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{one}");
        let _ = out.flush();
    }
    unsafe {
        use windows::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
        let _ = TerminateProcess(GetCurrentProcess(), 0);
    }
    std::process::exit(0)
}

// ------------------------------------------------------------------------
// La sonda (lado del receptor)

pub struct Probe {
    rx: Receiver<Option<String>>,
    child: Option<std::process::Child>,
    started: Instant,
}

/// Lanza la sonda: el mismo exe con `--probe-window`, sin consola, sin las
/// variables del actualizador ni las del modo humo.
pub fn spawn_probe() -> Result<Probe, String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg(PROBE_ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // CREATE_NO_WINDOW: una compilación de depuración es de consola
        .creation_flags(0x0800_0000);
    for k in [
        "PEPOMOTE_UPDATE_HEALTH",
        "PEPOMOTE_UPDATE_RESULT",
        "PEPOMOTE_SMOKE",
        "PEPOMOTE_SMOKE_FAIL_FIRST",
        "PEPOMOTE_FAKE_UI_HANG",
    ] {
        cmd.env_remove(k);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let out = child.stdout.take().ok_or_else(|| "la sonda no dejó salida que leer".to_owned())?;
    let (tx, rx) = std::sync::mpsc::channel();
    crate::threads::spawn_once("pmp-probe-read", move || {
        let mut line = String::new();
        let read = std::io::BufReader::new(out).read_line(&mut line);
        let _ = tx.send((read.is_ok() && !line.trim().is_empty()).then_some(line));
    })
    .map_err(|e| e.to_string())?;
    Ok(Probe { rx, child: Some(child), started: Instant::now() })
}

impl Probe {
    /// La respuesta, esperando como mucho lo que quede de `budget` desde
    /// que arrancó. La sonda nunca se espera en este hilo: si no contestó
    /// se la mata, y un hilo aparte la recoge.
    pub fn wait(mut self, budget: Duration) -> (ProbeResult, Duration) {
        let left = budget.saturating_sub(self.started.elapsed());
        let result = match self.rx.recv_timeout(left) {
            Ok(Some(line)) => parse_probe_line(&line),
            Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                let code = self.child.as_mut().and_then(|c| {
                    let t = Instant::now();
                    loop {
                        match c.try_wait() {
                            Ok(Some(st)) => break st.code(),
                            Ok(None) if t.elapsed() < Duration::from_millis(300) => std::thread::sleep(Duration::from_millis(20)),
                            _ => break None,
                        }
                    }
                });
                ProbeResult::Failed(match code {
                    Some(c) => format!("la sonda terminó sin contestar (código {c:#x})"),
                    None => "la sonda terminó sin contestar".to_owned(),
                })
            }
            Err(RecvTimeoutError::Timeout) => ProbeResult::TimedOut,
        };
        let took = self.started.elapsed();
        if let Some(mut child) = self.child.take() {
            let kill = result == ProbeResult::TimedOut;
            let _ = crate::threads::spawn_once("pmp-probe-reap", move || {
                if kill {
                    let _ = child.kill();
                }
                let t = Instant::now();
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) | Err(_) => return,
                        Ok(None) if t.elapsed() >= Duration::from_secs(30) => {
                            // Un hilo atascado dentro del driver (espera del
                            // núcleo) la tiene viva: queda otro «PepoMote» en
                            // el Administrador de tareas, que se sepa por qué
                            crate::log_line!(
                                "Sonda de la ventana: sigue viva 30 s después de matarla (PID {}): atascada dentro del driver de la gráfica",
                                child.id()
                            );
                            return;
                        }
                        Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                    }
                }
            });
        }
        (result, took)
    }
}

// ------------------------------------------------------------------------
// Las opciones de la ventana y lo que pintó

/// Las opciones de eframe para un renderer. Con la feature `wgpu` activada,
/// eframe elegiría wgpu por defecto: el renderer va SIEMPRE explícito.
pub fn native_options(renderer: Renderer, viewport: egui::ViewportBuilder) -> eframe::NativeOptions {
    let mut o = eframe::NativeOptions { viewport, ..Default::default() };
    match renderer {
        Renderer::OpenGl => {
            o.renderer = eframe::Renderer::Glow;
            if std::env::var_os(ENV_FAKE_NO_OPENGL).is_some() {
                o.hardware_acceleration = eframe::HardwareAcceleration::Off;
            }
        }
        Renderer::Direct3d => {
            o.renderer = eframe::Renderer::Wgpu;
            o.wgpu_options = direct3d_options();
        }
    }
    o
}

/// Direct3D 12 y nada más (wgpu también trae Vulkan y GL, que cargarían el
/// mismo driver que acaba de fallar), sin despertar la gráfica dedicada de
/// un portátil (a WARP se llega igual si no hay GPU) y con memoria y límites
/// de una ventana pequeña: con WARP la memoria de la GPU es la RAM.
pub fn direct3d_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::wgpu;
    let backends = if std::env::var_os(ENV_FAKE_NO_DIRECT3D).is_some() {
        wgpu::Backends::empty()
    } else {
        wgpu::Backends::DX12
    };
    eframe::egui_wgpu::WgpuConfiguration {
        supported_backends: backends,
        power_preference: wgpu::PowerPreference::LowPower,
        device_descriptor: std::sync::Arc::new(|adapter: &wgpu::Adapter| wgpu::DeviceDescriptor {
            label: Some("PepoMote"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
        }),
        ..Default::default()
    }
}

fn gl_info(cc: &eframe::CreationContext<'_>) -> Option<String> {
    use eframe::glow::HasContext as _;
    let gl = cc.gl.as_ref()?;
    let (renderer, version) =
        unsafe { (gl.get_parameter_string(eframe::glow::RENDERER), gl.get_parameter_string(eframe::glow::VERSION)) };
    Some(format!("{renderer} · {version}").replace(['\r', '\n'], " "))
}

fn wgpu_info(cc: &eframe::CreationContext<'_>) -> Option<String> {
    use eframe::wgpu::DeviceType;
    let info = cc.wgpu_render_state.as_ref()?.adapter.get_info();
    let kind = match info.device_type {
        DeviceType::Cpu => "por CPU, WARP",
        DeviceType::DiscreteGpu => "GPU dedicada",
        DeviceType::IntegratedGpu => "GPU integrada",
        DeviceType::VirtualGpu => "GPU virtual",
        DeviceType::Other => "otra",
    };
    Some(format!("{} ({kind})", info.name))
}

// ------------------------------------------------------------------------
// El intento en curso (para el log, el traspaso y la marca)

static CURRENT: Mutex<Option<(u8, Renderer)>> = Mutex::new(None);
static INFO: Mutex<Option<String>> = Mutex::new(None);
static ATTEMPT_STARTED: Mutex<Option<Instant>> = Mutex::new(None);
static PROBE_TOOK_MS: AtomicU64 = AtomicU64::new(0);
static FINGERPRINT: OnceLock<String> = OnceLock::new();
static FORCED: AtomicBool = AtomicBool::new(false);

/// «intento N · OpenGL · <gráfica y versión>» del intento en curso.
pub fn describe_current() -> Option<String> {
    let (n, r) = (*CURRENT.lock_tolerant())?;
    Some(match INFO.lock_tolerant().clone() {
        Some(info) => format!("intento {n} · {} · {info}", r.name()),
        None => format!("intento {n} · {}", r.name()),
    })
}

/// El renderer del intento en curso.
pub fn current() -> Option<Renderer> {
    CURRENT.lock_tolerant().map(|(_, r)| r)
}

/// En el creador de la app: qué ha dado el renderer (para la línea del
/// primer fotograma).
pub fn note_painter(cc: &eframe::CreationContext<'_>) {
    *INFO.lock_tolerant() = gl_info(cc).or_else(|| wgpu_info(cc));
}

/// Solo pruebas (`PEPOMOTE_FAKE_GL_HANG=ventana`): la ventana de verdad se
/// cuelga en su creador, con OpenGL ya creado y antes del primer fotograma
/// (con Direct3D no: lo que se prueba es un driver de OpenGL que se cuelga).
pub fn fake_window_hang_if_requested() {
    if current() == Some(Renderer::OpenGl) && std::env::var(ENV_FAKE_GL_HANG).is_ok_and(|v| v.trim() == "ventana") {
        crate::log_line!("PEPOMOTE_FAKE_GL_HANG: la ventana se cuelga a propósito antes de su primer fotograma");
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}

/// Empieza un intento: queda la marca en settings.json hasta que pinte o
/// falle limpio.
pub fn begin_attempt(shared: &SharedState, renderer: Renderer, n: u8) {
    *CURRENT.lock_tolerant() = Some((n, renderer));
    *INFO.lock_tolerant() = None;
    *ATTEMPT_STARTED.lock_tolerant() = Some(Instant::now());
    if let Some(fp) = FINGERPRINT.get() {
        let mut s = shared.lock_tolerant();
        s.config.window_pending = Some(WindowRenderer { fingerprint: fp.clone(), renderer: renderer.key().to_owned() });
        s.config.save();
    }
}

/// `run_native` ha vuelto (cierre o fallo limpio): la marca fuera, que no
/// se tome por un cuelgue en el próximo arranque.
pub fn end_attempt(shared: &SharedState) {
    let mut s = shared.lock_tolerant();
    if s.config.window_pending.take().is_some() {
        s.config.save();
    }
}

/// El intento falló antes de pintar sin crear la app: si era OpenGL, esta
/// gráfica queda apuntada para Direct3D.
pub fn failed_before_frame(shared: &SharedState, renderer: Renderer) {
    if renderer != Renderer::OpenGl || FORCED.load(Ordering::SeqCst) {
        return;
    }
    if let Some(fp) = FINGERPRINT.get() {
        let mut s = shared.lock_tolerant();
        if remember(&mut s.config.window_renderer, fp, Renderer::Direct3d) {
            s.config.save();
        }
    }
}

/// Primer fotograma pintado: la marca fuera y el renderer que ha pintado,
/// recordado para esta gráfica.
pub fn painted(shared: &SharedState) {
    let mut s = shared.lock_tolerant();
    let mut changed = s.config.window_pending.take().is_some();
    if let (Some(fp), Some(r)) = (FINGERPRINT.get(), current()) {
        if !FORCED.load(Ordering::SeqCst) {
            changed |= remember(&mut s.config.window_renderer, fp, r);
        }
    }
    if changed {
        s.config.save();
    }
}

/// Lo que mira `singleton` cuando otra copia pide la ventana.
pub fn handover_now() -> bool {
    let age = ATTEMPT_STARTED.lock_tolerant().map(|t| t.elapsed());
    handover_allowed(
        current(),
        crate::launch::first_frame_done(),
        age,
        Duration::from_millis(PROBE_TOOK_MS.load(Ordering::SeqCst)),
        crate::launch::last_step() == crate::launch::Step::Egui,
        std::env::var_os("PEPOMOTE_UPDATE_HEALTH").is_some(),
    )
}

/// Cuánto hace que empezó el intento en curso (para el log del traspaso).
pub fn attempt_age() -> Option<Duration> {
    ATTEMPT_STARTED.lock_tolerant().map(|t| t.elapsed())
}

// ------------------------------------------------------------------------
// El plan de la ventana

/// Lo que se decide al arrancar y se completa al ir a abrir la ventana.
pub struct WindowPlan {
    fingerprint: String,
    start: Start,
    probe: Option<Probe>,
    probe_error: Option<String>,
}

impl WindowPlan {
    /// Tras el cerrojo: la gráfica al log, la huella, el recuerdo y la marca.
    /// Si la ventana se va a abrir ya y hace falta, la sonda arranca aquí,
    /// en paralelo con la red y la bandeja.
    pub fn begin(shared: &SharedState, took_over: bool, window_now: bool) -> WindowPlan {
        let cards = adapters();
        crate::log_line!("Gráfica: {}", describe_adapters(&cards));
        let fingerprint = fingerprint_of(&cards);
        let forced = std::env::var(ENV_RENDERER).ok().and_then(|v| Renderer::parse(&v));
        FORCED.store(forced.is_some(), Ordering::SeqCst);
        let (pending, remembered) = {
            let s = shared.lock_tolerant();
            let pending = s
                .config
                .window_pending
                .as_ref()
                .filter(|p| p.fingerprint == fingerprint)
                .and_then(|p| Renderer::parse(&p.renderer));
            (pending, recall(&s.config.window_renderer, &fingerprint))
        };
        let probe_disabled = std::env::var_os(ENV_NO_PROBE).is_some();
        let start = decide_start(forced, took_over, pending, remembered, probe_disabled);
        let _ = FINGERPRINT.set(fingerprint.clone());
        let mut plan = WindowPlan { fingerprint, start, probe: None, probe_error: None };
        if window_now {
            plan.ensure_probe();
        }
        plan
    }

    /// Lanza la sonda si hace falta y aún no se lanzó (con `--minimized`, al
    /// pedirse la ventana).
    pub fn ensure_probe(&mut self) {
        if self.start == Start::Probe && self.probe.is_none() && self.probe_error.is_none() {
            match spawn_probe() {
                Ok(p) => self.probe = Some(p),
                Err(e) => self.probe_error = Some(e),
            }
        }
    }

    /// La cadena de intentos: espera a la sonda (con tope), lo deja en el
    /// log y lo recuerda para esta gráfica.
    pub fn chain(mut self, shared: &SharedState) -> Vec<Renderer> {
        self.ensure_probe();
        let (chain, remember_now, why) = match std::mem::replace(&mut self.start, Start::Probe) {
            Start::Fixed { chain, remember, why } => (chain, remember, why),
            Start::Probe => {
                let (result, took) = match self.probe.take() {
                    Some(p) => p.wait(PROBE_BUDGET),
                    None => (ProbeResult::NotRun(self.probe_error.take().unwrap_or_default()), Duration::ZERO),
                };
                PROBE_TOOK_MS.store(took.as_millis() as u64, Ordering::SeqCst);
                let ms = took.as_millis();
                let why = match &result {
                    ProbeResult::Ok(info) => format!("sonda de OpenGL bien en {ms} ms ({info})"),
                    ProbeResult::Failed(e) => format!("sonda de OpenGL mal en {ms} ms: {e}"),
                    ProbeResult::TimedOut => {
                        format!("la sonda de OpenGL no contesta en {} s: se la mata", PROBE_BUDGET.as_secs())
                    }
                    ProbeResult::NotRun(e) => format!("sin sonda (no se pudo lanzar: {e})"),
                };
                let (chain, remember) = after_probe(&result);
                (chain, remember, why)
            }
        };
        crate::log_line!(
            "Ventana: {why}; se pinta con {}",
            chain.iter().map(|r| r.name()).collect::<Vec<_>>().join(" y, si falla, ")
        );
        if let Some(r) = remember_now {
            let mut s = shared.lock_tolerant();
            if remember(&mut s.config.window_renderer, &self.fingerprint, r) {
                s.config.save();
            }
        }
        chain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Renderer::{Direct3d, OpenGl};

    #[test]
    fn el_renderer_se_lee_y_se_guarda() {
        assert_eq!(Renderer::parse("opengl"), Some(OpenGl));
        assert_eq!(Renderer::parse(" Direct3D "), Some(Direct3d));
        assert_eq!(Renderer::parse("dx12"), Some(Direct3d));
        assert_eq!(Renderer::parse("vulkan"), None);
        for r in [OpenGl, Direct3d] {
            assert_eq!(Renderer::parse(r.key()), Some(r));
        }
        assert_eq!(Direct3d.name(), "Direct3D 12");
    }

    #[test]
    fn la_decision_al_arrancar() {
        // forzado: solo eso, sin recordar nada
        assert!(matches!(
            decide_start(Some(Direct3d), true, Some(OpenGl), Some(OpenGl), false),
            Start::Fixed { ref chain, remember: None, .. } if chain == &vec![Direct3d]
        ));
        // la copia anterior se colgó: Direct3D y se recuerda
        assert!(matches!(
            decide_start(None, true, None, Some(OpenGl), false),
            Start::Fixed { ref chain, remember: Some(Direct3d), .. } if chain == &vec![Direct3d]
        ));
        // marca de OpenGL: el arranque anterior no pintó con OpenGL
        assert!(matches!(
            decide_start(None, false, Some(OpenGl), Some(OpenGl), false),
            Start::Fixed { ref chain, remember: Some(Direct3d), .. } if chain == &vec![Direct3d]
        ));
        // marca de Direct3D: nunca de vuelta a OpenGL
        assert!(matches!(
            decide_start(None, false, Some(Direct3d), None, false),
            Start::Fixed { ref chain, remember: None, .. } if chain == &vec![Direct3d]
        ));
        // recordado: sin sonda
        assert!(matches!(
            decide_start(None, false, None, Some(OpenGl), false),
            Start::Fixed { ref chain, .. } if chain == &vec![OpenGl, Direct3d]
        ));
        assert!(matches!(
            decide_start(None, false, None, Some(Direct3d), false),
            Start::Fixed { ref chain, .. } if chain == &vec![Direct3d]
        ));
        // nada sabido: sonda; sin sonda: OpenGL y luego Direct3D
        assert_eq!(decide_start(None, false, None, None, false), Start::Probe);
        assert!(matches!(
            decide_start(None, false, None, None, true),
            Start::Fixed { ref chain, remember: None, .. } if chain == &vec![OpenGl, Direct3d]
        ));
    }

    #[test]
    fn la_linea_de_la_sonda() {
        assert_eq!(parse_probe_line("OK NVIDIA GeForce · 4.6.0\r\n"), ProbeResult::Ok("NVIDIA GeForce · 4.6.0".into()));
        assert_eq!(parse_probe_line("OK"), ProbeResult::Ok(String::new()));
        assert_eq!(
            parse_probe_line("ERR egui_glow requires opengl 2.0+.\n"),
            ProbeResult::Failed("egui_glow requires opengl 2.0+.".into())
        );
        assert!(matches!(parse_probe_line("OKAY"), ProbeResult::Failed(_)));
        assert!(matches!(parse_probe_line(""), ProbeResult::Failed(_)));
    }

    #[test]
    fn la_cadena_segun_la_sonda() {
        assert_eq!(after_probe(&ProbeResult::Ok("x".into())), (vec![OpenGl, Direct3d], Some(OpenGl)));
        assert_eq!(after_probe(&ProbeResult::Failed("x".into())), (vec![Direct3d], Some(Direct3d)));
        assert_eq!(after_probe(&ProbeResult::TimedOut), (vec![Direct3d], Some(Direct3d)));
        assert_eq!(after_probe(&ProbeResult::NotRun("x".into())), (vec![OpenGl, Direct3d], None));
    }

    #[test]
    fn se_recuerdan_las_ultimas_cuatro_huellas() {
        let mut list = Vec::new();
        assert!(remember(&mut list, "a", OpenGl));
        assert!(!remember(&mut list, "a", OpenGl), "lo mismo otra vez no cambia nada");
        assert!(remember(&mut list, "a", Direct3d));
        assert_eq!(list.len(), 1);
        assert_eq!(recall(&list, "a"), Some(Direct3d));
        for fp in ["b", "c", "d", "e"] {
            remember(&mut list, fp, OpenGl);
        }
        assert_eq!(list.len(), REMEMBERED);
        assert_eq!(recall(&list, "a"), None, "la más vieja se olvida");
        assert_eq!(recall(&list, "e"), Some(OpenGl));
        assert_eq!(recall(&list, "zz"), None);
    }

    #[test]
    fn el_traspaso_solo_con_opengl_colgado_de_verdad() {
        let s = Duration::from_secs;
        assert!(handover_allowed(Some(OpenGl), false, Some(s(6)), s(1), true, false));
        // pronto: una ventana lenta pero sana no se sacrifica
        assert!(!handover_allowed(Some(OpenGl), false, Some(s(4)), s(0), true, false));
        // la sonda tardó 4 s: hace falta el doble
        assert!(!handover_allowed(Some(OpenGl), false, Some(s(6)), s(4), true, false));
        assert!(handover_allowed(Some(OpenGl), false, Some(s(9)), s(4), true, false));
        // ya pintó, Direct3D, en una espera nuestra, tras actualizar, sin intento
        assert!(!handover_allowed(Some(OpenGl), true, Some(s(60)), s(0), true, false));
        assert!(!handover_allowed(Some(Direct3d), false, Some(s(60)), s(0), true, false));
        assert!(!handover_allowed(Some(OpenGl), false, Some(s(60)), s(0), false, false));
        assert!(!handover_allowed(Some(OpenGl), false, Some(s(60)), s(0), true, true));
        assert!(!handover_allowed(Some(OpenGl), false, None, s(0), true, false));
        assert!(!handover_allowed(None, false, Some(s(60)), s(0), true, false));
    }

    #[test]
    fn la_clave_del_registro_de_cada_tarjeta() {
        assert_eq!(
            registry_subkey("\\Registry\\Machine\\System\\CurrentControlSet\\Control\\Video\\{X}\\0000"),
            Some("System\\CurrentControlSet\\Control\\Video\\{X}\\0000")
        );
        assert_eq!(registry_subkey("\\REGISTRY\\MACHINE\\a"), Some("a"));
        assert_eq!(registry_subkey("\\Registry\\User\\a"), None);
        assert_eq!(registry_subkey(""), None);
    }

    #[test]
    fn la_huella_ignora_el_orden_y_las_repetidas() {
        let a = Adapter { name: "Intel".into(), id: "PCI\\VEN_8086".into(), driver: Some("31.0".into()), vendor_gl: true };
        let b = Adapter { name: "NVIDIA".into(), id: "PCI\\VEN_10DE".into(), driver: None, vendor_gl: true };
        let one = fingerprint_of(&[a.clone(), b.clone()]);
        assert_eq!(one, fingerprint_of(&[b.clone(), a.clone(), a.clone()]));
        assert!(one.contains("Intel|PCI\\VEN_8086|31.0"));
        assert!(one.contains("NVIDIA|PCI\\VEN_10DE|?"));
        assert!(one.ends_with(&format!("PepoMote {}", env!("CARGO_PKG_VERSION"))));
        let other_driver = Adapter { driver: Some("32.0".into()), ..a.clone() };
        assert_ne!(one, fingerprint_of(&[other_driver, b]));
        assert!(fingerprint_of(&[]).starts_with("sin tarjetas"));
        let basic = Adapter { name: "Adaptador de pantalla básico de Microsoft".into(), id: "x".into(), driver: None, vendor_gl: false };
        assert!(describe_adapters(&[basic]).contains("solo el OpenGL 1.1 de Windows"));
    }

    #[test]
    fn las_tarjetas_de_este_pc_se_leen_sin_romper_nada() {
        // En cualquier Windows (con o sin driver): no revienta y da una huella
        let cards = adapters();
        let fp = fingerprint_of(&cards);
        assert!(fp.contains("PepoMote"));
        assert!(!describe_adapters(&cards).is_empty());
    }
}
