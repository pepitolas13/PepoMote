//! RetroArch: el móvil es un RetroPad (o un mando de NES, o una pistola de
//! luz) por el «mando en red» de RetroArch, y sus teclas rápidas van por la
//! interfaz de comandos. Sin driver de mando virtual y sin DSU: RetroArch
//! trae las dos cosas de serie en Windows, Linux, macOS y Android; solo hay
//! que activarlas en `retroarch.cfg`, y eso lo hace PepoMote con RetroArch
//! cerrado (lo reescribe entero al salir), igual que con Eden.
mod cfg;
mod link;
mod mapping;
mod paths;
mod protocol;

use crate::state::{CfgStatus, Config, LockTolerant, Mode, SharedState};
use crate::tr;
pub use link::{link, ports, start, Live, Ports};
pub use mapping::{RetroPadKind, GUN_MOUSE_BITS};
pub use paths::running_exe;
pub use protocol::Activity;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static CONFIGURE_LOCK: Mutex<()> = Mutex::new(());

fn read_config(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("cfg.pepomote.bak")
}

/// Escribe las claves de PepoMote en `path` (copia de seguridad la primera
/// vez). Devuelve si hubo que cambiar algo.
fn write_config(path: &Path, ports: Ports) -> Result<bool, String> {
    let original = read_config(path)?;
    let values = cfg::managed_values(ports);
    if !cfg::needs_change(&original, &values) {
        return Ok(false);
    }
    let new = cfg::apply(&original, &values);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let bak = backup_path(path);
    if !bak.exists() {
        crate::ini::atomic_write(&bak, &original)?;
    }
    crate::ini::atomic_write(path, &new)?;
    Ok(true)
}

fn restore_config(path: &Path, ports: Ports) -> Result<bool, String> {
    let bak = backup_path(path);
    if !bak.is_file() {
        return Ok(false);
    }
    let current = read_config(path)?;
    let backup = read_config(&bak)?;
    let new = cfg::restore(&current, &backup, &cfg::managed_keys(ports));
    if current == new {
        return Ok(false);
    }
    crate::ini::atomic_write(path, &new)?;
    Ok(true)
}

/// ¿Alguno de los `retroarch.cfg` conocidos necesita cambios?
fn any_needs_change(files: &[paths::ConfigFile], ports: Ports) -> bool {
    let values = cfg::managed_values(ports);
    files.iter().any(|f| read_config(&f.path).map(|t| cfg::needs_change(&t, &values)).unwrap_or(true))
}

fn describe(files: &[paths::ConfigFile]) -> String {
    files
        .iter()
        .map(|f| format!("{} ({})", f.path.display(), f.why))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn has_selected_config(path: &Path) -> bool {
    (path.file_name().is_some_and(|n| n.eq_ignore_ascii_case("retroarch.cfg")) && path.is_file())
        || path.join("retroarch.cfg").is_file()
}

/// La carpeta del ejecutable se aprende al verlo abierto: en Windows la
/// configuración portable vive ahí.
pub(crate) fn learn_dir(shared: &SharedState, dir: Option<PathBuf>) {
    let Some(dir) = dir else { return };
    if !cfg!(windows) {
        return;
    }
    let mut s = shared.lock_tolerant();
    if s.config.retroarch_dir.trim().is_empty() && !has_selected_config(Path::new(&s.config.retroarch_dir)) {
        s.config.retroarch_dir = dir.to_string_lossy().into_owned();
        s.config.save();
    }
}

fn observed() -> (bool, Option<PathBuf>) {
    if std::env::var_os("PEPOMOTE_ASSUME_EMULATOR_CLOSED").is_some() {
        (false, None)
    } else {
        running_exe()
    }
}

fn status(shared: &SharedState, ok: bool, text: String, phone: &str) {
    crate::log_line!("RetroArch: {text}");
    shared.lock_tolerant().retroarch_cfg_status = Some(CfgStatus { ok, text });
    crate::net::notify_all(phone);
}

fn configure(cfg: &Config, ports: Ports) -> Result<(bool, String), String> {
    let files = paths::config_files_with(cfg);
    if files.is_empty() {
        return Err(tr!("retroarch.not_found").to_owned());
    }
    let mut changed = false;
    for f in &files {
        changed |= write_config(&f.path, ports)?;
    }
    Ok((changed, describe(&files)))
}

fn run_configure(shared: &SharedState, after_close: bool, manual: bool) {
    let _serial = CONFIGURE_LOCK.lock_tolerant();
    if manual {
        let mut s = shared.lock_tolerant();
        s.retroarch_restore_pending = false;
        if s.config.retroarch_restore_pending {
            s.config.retroarch_restore_pending = false;
            s.config.save();
        }
    }
    let (cfg, mode, restoring) = {
        let s = shared.lock_tolerant();
        (s.config.clone(), s.mode, s.retroarch_restore_pending)
    };
    if !manual && (!cfg.auto_retroarch || mode != Mode::RetroArch || restoring) {
        if after_close {
            shared.lock_tolerant().retroarch_pending = false;
        }
        return;
    }
    let ports = ports();
    let (running, dir) = observed();
    learn_dir(shared, dir);
    let cfg = shared.lock_tolerant().config.clone();
    let files = paths::config_files_with(&cfg);
    if files.is_empty() {
        let text = tr!("retroarch.error", tr!("retroarch.not_found"));
        status(shared, false, text.clone(), &text);
        return;
    }
    if !any_needs_change(&files, ports) {
        // Ya estaba: no hace falta cerrar nada
        {
            let mut s = shared.lock_tolerant();
            s.retroarch_pending = false;
            s.retroarch_manual_pending = false;
        }
        status(
            shared,
            true,
            format!("{} {}", tr!("retroarch.already"), describe(&files)),
            tr!("retroarch.phone_configured"),
        );
        return;
    }
    if running {
        {
            let mut s = shared.lock_tolerant();
            s.retroarch_pending = true;
            s.retroarch_manual_pending |= manual;
        }
        status(shared, false, tr!("retroarch.open").to_owned(), tr!("retroarch.phone_open"));
        return;
    }
    {
        let mut s = shared.lock_tolerant();
        s.retroarch_pending = false;
        s.retroarch_manual_pending = false;
    }
    match configure(&cfg, ports) {
        Ok((_, details)) => {
            let prefix = if after_close { tr!("retroarch.configured_after_close") } else { tr!("retroarch.configured") };
            status(shared, true, format!("{prefix} {details}"), tr!("retroarch.phone_configured"));
        }
        Err(e) => {
            let text = tr!("retroarch.error", e);
            status(shared, false, text.clone(), &text);
        }
    }
}

pub fn maybe_auto_configure(shared: &SharedState) {
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("retroarch-configure", move || run_configure(&shared, false, false));
}

pub fn configure_now(shared: &SharedState) {
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("retroarch-configure", move || run_configure(&shared, false, true));
}

pub fn apply_pending(shared: &SharedState) {
    let (restore, manual) = {
        let s = shared.lock_tolerant();
        (s.retroarch_restore_pending, s.retroarch_manual_pending)
    };
    if restore {
        run_restore(shared);
    } else {
        run_configure(shared, true, manual);
    }
}

fn run_restore(shared: &SharedState) {
    let _serial = CONFIGURE_LOCK.lock_tolerant();
    if !shared.lock_tolerant().retroarch_restore_pending {
        return;
    }
    if observed().0 {
        {
            let mut s = shared.lock_tolerant();
            s.retroarch_pending = true;
            s.retroarch_restore_pending = true;
            s.retroarch_manual_pending = false;
        }
        status(shared, false, tr!("retroarch.restore_open").to_owned(), tr!("retroarch.restore_open"));
        return;
    }
    {
        let mut s = shared.lock_tolerant();
        s.retroarch_pending = false;
        s.retroarch_restore_pending = false;
    }
    let cfg = shared.lock_tolerant().config.clone();
    let ports = ports();
    let result = (|| {
        let files = paths::config_files_with(&cfg);
        if files.is_empty() {
            return Err(tr!("retroarch.not_found").to_owned());
        }
        let mut restored = false;
        for f in files {
            restored |= restore_config(&f.path, ports)?;
        }
        Ok(restored)
    })();
    match result {
        Ok(restored) => {
            {
                let mut s = shared.lock_tolerant();
                s.config.retroarch_restore_pending = false;
                s.config.save();
            }
            let text = if restored { tr!("retroarch.restored") } else { tr!("retroarch.nothing_restore") };
            status(shared, true, text.to_owned(), text);
        }
        Err(e) => {
            let text = tr!("retroarch.error", e);
            status(shared, false, text.clone(), &text);
        }
    }
}

pub fn restore_now(shared: &SharedState) {
    {
        let mut s = shared.lock_tolerant();
        s.config.auto_retroarch = false;
        s.config.retroarch_restore_pending = true;
        s.config.save();
        s.retroarch_restore_pending = true;
    }
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("retroarch-restore", move || run_restore(&shared));
}

pub fn detect_now(shared: &SharedState) {
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("retroarch-detect", move || {
        let (_, dir) = running_exe();
        let found = dir.into_iter().chain(paths::find_exe_dirs()).next();
        let mut s = shared.lock_tolerant();
        if let Some(d) = found {
            s.config.retroarch_dir = d.to_string_lossy().into_owned();
            s.config.save();
            s.retroarch_cfg_status = Some(CfgStatus::ok(tr!("retroarch.found", d.display())));
        } else {
            let files = paths::config_files_with(&s.config);
            if let Some(f) = files.first() {
                s.retroarch_cfg_status = Some(CfgStatus::ok(tr!("retroarch.found", f.path.display())));
            } else {
                s.retroarch_cfg_status = Some(CfgStatus::warn(tr!("retroarch.not_found").to_owned()));
            }
        }
    });
}

/// Activar la ventana de RetroArch antes de teclear texto desde el móvil.
#[cfg(windows)]
pub fn focus_keyboard() -> bool {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible,
    };
    unsafe fn is_retroarch(hwnd: HWND) -> bool {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut buf = [0u16; 2048];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return false;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
        path.rsplit(['\\', '/']).next().is_some_and(paths::is_exe_name)
    }
    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> BOOL {
        if IsWindowVisible(hwnd).as_bool() && is_retroarch(hwnd) {
            (*(param.0 as *mut Vec<HWND>)).push(hwnd);
        }
        BOOL(1)
    }
    unsafe {
        if is_retroarch(GetForegroundWindow()) {
            return true;
        }
        let mut windows = Vec::new();
        let _ = EnumWindows(Some(collect), LPARAM(&mut windows as *mut Vec<HWND> as isize));
        for hwnd in windows {
            if crate::input::windows_input::focus_for_text(hwnd) {
                return true;
            }
        }
    }
    false
}

#[cfg(target_os = "macos")]
pub fn focus_keyboard() -> bool {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    for (pid, path) in crate::procs::list() {
        let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !name.starts_with("retroarch") {
            continue;
        }
        unsafe {
            if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32) {
                if app.isActive() {
                    return true;
                }
                if app.activateWithOptions(NSApplicationActivationOptions::NSApplicationActivateIgnoringOtherApps) {
                    for _ in 0..20 {
                        if app.isActive() {
                            return true;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
            }
        }
    }
    false
}

#[cfg(target_os = "linux")]
pub fn focus_keyboard() -> bool {
    // Sin API global de foco en Wayland: RetroArch tiene que estar delante.
    running_exe().0
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn focus_keyboard() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORTS: Ports = Ports { base: 55400, cmd: 55355 };

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pepomote-ra-mod-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn escribe_con_copia_de_seguridad_una_sola_vez_y_restaura() {
        let d = tmp("write");
        let path = d.join("retroarch.cfg");
        let original = "video_fullscreen = \"true\"\r\nnetwork_cmd_enable = \"false\"\r\n";
        std::fs::write(&path, original).unwrap();
        assert!(write_config(&path, PORTS).unwrap(), "hubo cambios");
        let bak = backup_path(&path);
        assert_eq!(bak, d.join("retroarch.cfg.pepomote.bak"));
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), original, "copia byte a byte");
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.starts_with("video_fullscreen = \"true\"\r\nnetwork_cmd_enable = \"true\"\r\n"));
        assert!(written.contains("network_remote_enable = \"true\"\r\n"));
        assert!(!write_config(&path, PORTS).unwrap(), "segunda pasada: nada que cambiar");
        // RetroArch reescribe el archivo (añade claves) y la copia no se pisa
        std::fs::write(&path, format!("{written}audio_volume = \"0.5\"\r\n")).unwrap();
        assert!(!write_config(&path, PORTS).unwrap());
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), original);
        assert!(restore_config(&path, PORTS).unwrap());
        let restored = std::fs::read_to_string(&path).unwrap();
        assert_eq!(cfg::get(&restored, "network_cmd_enable").as_deref(), Some("false"));
        assert_eq!(cfg::get(&restored, "network_remote_enable"), None);
        assert_eq!(cfg::get(&restored, "audio_volume").as_deref(), Some("0.5"), "lo de RetroArch se queda");
        assert!(!restore_config(&path, PORTS).unwrap(), "ya restaurado: nada");
        // sin copia no hay nada que restaurar
        let other = d.join("otro.cfg");
        std::fs::write(&other, "a = \"1\"\n").unwrap();
        assert!(!restore_config(&other, PORTS).unwrap());
    }

    #[test]
    fn un_archivo_que_no_existe_se_crea_con_lo_nuestro() {
        let d = tmp("create");
        let path = d.join("sub/retroarch.cfg");
        assert!(write_config(&path, PORTS).unwrap());
        let t = std::fs::read_to_string(&path).unwrap();
        assert!(!cfg::needs_change(&t, &cfg::managed_values(PORTS)));
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), "");
    }

    #[test]
    fn saber_si_hace_falta_cambiar_antes_de_tocar_nada() {
        let d = tmp("needs");
        let path = d.join("retroarch.cfg");
        std::fs::write(&path, cfg::apply("", &cfg::managed_values(PORTS))).unwrap();
        let files = vec![paths::ConfigFile { path: path.clone(), why: "test" }];
        assert!(!any_needs_change(&files, PORTS));
        assert!(any_needs_change(&files, Ports { base: 55410, cmd: 55355 }));
        let missing = vec![paths::ConfigFile { path: d.join("no.cfg"), why: "test" }];
        assert!(any_needs_change(&missing, PORTS), "un archivo que no existe hay que crearlo");
    }

    #[test]
    fn la_carpeta_de_ajustes_se_respeta() {
        let d = tmp("settings");
        std::fs::write(d.join("retroarch.cfg"), "").unwrap();
        let cfg = Config { retroarch_dir: d.to_string_lossy().into_owned(), ..Config::default() };
        let files = paths::config_files_with(&cfg);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, d.join("retroarch.cfg"));
        assert_eq!(files[0].why, tr!("retroarch.why_manual"));
    }
}
