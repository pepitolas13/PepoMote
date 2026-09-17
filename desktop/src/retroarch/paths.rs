//! Dónde vive `retroarch.cfg` y si RetroArch está abierto. Sigue el orden
//! de búsqueda del propio RetroArch (`configuration.c`,
//! `config_load_file_salamander` / `open_default_config_file`):
//!
//! - Windows: `retroarch.cfg` junto a `retroarch.exe` (portable: el zip
//!   oficial y la versión de Steam) y, si no existe, `%APPDATA%\RetroArch\`.
//!   Sin ninguno, RetroArch lo crea junto al ejecutable.
//! - Linux: `$XDG_CONFIG_HOME/retroarch/` (o `~/.config/retroarch/`), luego
//!   `~/.retroarch.cfg`; el Flatpak y el Snap tienen su propio `$HOME`.
//! - macOS: `~/Library/Application Support/RetroArch/config/`.
use crate::state::Config;
use crate::tr;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct Env {
    /// Carpetas con el ejecutable (proceso visto, Steam, Ajustes).
    pub exe_dirs: Vec<PathBuf>,
    /// Windows: `%APPDATA%`.
    pub appdata: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub home: Option<PathBuf>,
    /// macOS: `~/Library/Application Support`.
    pub app_support: Option<PathBuf>,
    pub os: Os,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Os {
    #[default]
    Windows,
    Linux,
    MacOs,
}

#[derive(Debug, Clone)]
pub(super) struct ConfigFile {
    pub path: PathBuf,
    pub why: &'static str,
}

fn push_unique(found: &mut Vec<ConfigFile>, path: PathBuf, why: &'static str) {
    if !found.iter().any(|c| c.path == path) {
        found.push(ConfigFile { path, why });
    }
}

/// Los `retroarch.cfg` a los que hay que escribir. Los que existen, todos;
/// si no existe ninguno, el que RetroArch crearía al arrancar.
pub(super) fn resolve_config_files(env: &Env) -> Vec<ConfigFile> {
    let mut found = Vec::new();
    let config_home = env
        .xdg_config_home
        .as_ref()
        .filter(|p| !p.as_os_str().is_empty())
        .cloned()
        .or_else(|| env.home.as_ref().map(|h| h.join(".config")));
    match env.os {
        Os::Windows => {
            for exe in &env.exe_dirs {
                let p = exe.join("retroarch.cfg");
                if p.is_file() {
                    push_unique(&mut found, p, tr!("retroarch.why_portable"));
                }
            }
            if let Some(appdata) = &env.appdata {
                let p = appdata.join("RetroArch").join("retroarch.cfg");
                if p.is_file() {
                    push_unique(&mut found, p, tr!("retroarch.why_roaming"));
                }
            }
            if found.is_empty() {
                if let Some(exe) = env.exe_dirs.iter().find(|d| d.is_dir()) {
                    push_unique(&mut found, exe.join("retroarch.cfg"), tr!("retroarch.why_portable"));
                }
            }
        }
        Os::Linux => {
            let mut candidates: Vec<(PathBuf, &'static str)> = Vec::new();
            if let Some(ch) = &config_home {
                candidates.push((ch.join("retroarch/retroarch.cfg"), tr!("retroarch.why_roaming")));
            }
            if let Some(home) = &env.home {
                candidates.push((
                    home.join(".var/app/org.libretro.RetroArch/config/retroarch/retroarch.cfg"),
                    tr!("retroarch.why_flatpak"),
                ));
                candidates.push((
                    home.join("snap/retroarch/current/.config/retroarch/retroarch.cfg"),
                    tr!("retroarch.why_snap"),
                ));
                candidates.push((home.join(".retroarch.cfg"), tr!("retroarch.why_roaming")));
            }
            for exe in &env.exe_dirs {
                candidates.push((exe.join("retroarch.cfg"), tr!("retroarch.why_portable")));
            }
            for (p, why) in &candidates {
                if p.is_file() {
                    push_unique(&mut found, p.clone(), why);
                }
            }
            if found.is_empty() {
                if let Some(ch) = &config_home {
                    push_unique(&mut found, ch.join("retroarch/retroarch.cfg"), tr!("retroarch.why_roaming"));
                }
            }
        }
        Os::MacOs => {
            if let Some(sup) = &env.app_support {
                push_unique(&mut found, sup.join("RetroArch/config/retroarch.cfg"), tr!("retroarch.why_roaming"));
            }
        }
    }
    found
}

fn os() -> Os {
    if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::MacOs
    } else {
        Os::Linux
    }
}

/// Carpetas de Steam con RetroArch instalado (`steamapps/common/RetroArch`).
fn steam_dirs() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(p) = std::env::var_os(var) {
                roots.push(PathBuf::from(p).join("Steam"));
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().to_owned()) {
            roots.push(home.join(".steam/steam"));
            roots.push(home.join(".local/share/Steam"));
            roots.push(home.join("Library/Application Support/Steam"));
        }
    }
    let mut libs: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        libs.push(root.clone());
        // libraryfolders.vdf: las demás bibliotecas ("path" "D:\\SteamLibrary")
        if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            for line in vdf.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("\"path\"") {
                    let v = rest.trim().trim_matches('"').replace("\\\\", "\\");
                    if !v.is_empty() {
                        libs.push(PathBuf::from(v));
                    }
                }
            }
        }
    }
    libs.into_iter()
        .map(|l| l.join("steamapps/common/RetroArch"))
        .filter(|d| has_exe(d))
        .collect()
}

#[cfg(windows)]
fn has_exe(dir: &Path) -> bool {
    dir.join("retroarch.exe").is_file()
}

#[cfg(not(windows))]
fn has_exe(dir: &Path) -> bool {
    dir.join("retroarch").is_file() || dir.join("RetroArch").is_file()
}

/// Ajustes → Carpeta de RetroArch: la del ejecutable, la que contiene
/// `retroarch.cfg`, o el propio archivo.
fn manual_file(dir: &str) -> Option<ConfigFile> {
    let manual = PathBuf::from(dir.trim());
    if dir.trim().is_empty() {
        return None;
    }
    if manual.is_file() && manual.file_name().is_some_and(|n| n.eq_ignore_ascii_case("retroarch.cfg")) {
        return Some(ConfigFile { path: manual, why: tr!("retroarch.why_manual") });
    }
    if manual.join("retroarch.cfg").is_file() {
        return Some(ConfigFile { path: manual.join("retroarch.cfg"), why: tr!("retroarch.why_manual") });
    }
    None
}

pub(super) fn config_files_with(cfg: &Config) -> Vec<ConfigFile> {
    if let Some(dir) = std::env::var_os("PEPOMOTE_RETROARCH_DIR") {
        return vec![ConfigFile {
            path: PathBuf::from(dir).join("retroarch.cfg"),
            why: tr!("retroarch.why_manual"),
        }];
    }
    if let Some(f) = manual_file(&cfg.retroarch_dir) {
        return vec![f];
    }
    let mut dirs = steam_dirs();
    let manual = PathBuf::from(cfg.retroarch_dir.trim());
    if !cfg.retroarch_dir.trim().is_empty() && manual.is_dir() && !dirs.contains(&manual) {
        dirs.insert(0, manual);
    }
    let base = directories::BaseDirs::new();
    let env = Env {
        exe_dirs: dirs,
        appdata: if cfg!(windows) { std::env::var_os("APPDATA").map(PathBuf::from) } else { None },
        xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        home: base.as_ref().map(|b| b.home_dir().to_owned()),
        app_support: base.as_ref().map(|b| b.home_dir().join("Library/Application Support")),
        os: os(),
    };
    resolve_config_files(&env)
}

/// Carpeta de datos (`playlists/`, `info/`) de un `retroarch.cfg`. Windows:
/// la del cfg (portable junto al exe; instalador `%APPDATA%\RetroArch`).
/// Linux: la del cfg (`~/.config/retroarch`, Flatpak, Snap), y para
/// `~/.retroarch.cfg` `~/.config/retroarch`. macOS: el cfg está en
/// `…/RetroArch/config/`, los datos en `…/RetroArch/`.
pub(super) fn data_dir_of(cfg: &Path, os: Os, home: Option<&Path>) -> PathBuf {
    let parent = cfg.parent().map(Path::to_path_buf).unwrap_or_default();
    match os {
        Os::Windows => parent,
        Os::Linux => {
            if cfg.file_name().is_some_and(|n| n == ".retroarch.cfg") {
                home.map(|h| h.join(".config").join("retroarch")).unwrap_or(parent)
            } else {
                parent
            }
        }
        Os::MacOs => {
            if parent.file_name().is_some_and(|n| n == "config") {
                parent.parent().map(Path::to_path_buf).unwrap_or(parent)
            } else {
                parent
            }
        }
    }
}

/// Las instalaciones de RetroArch conocidas (mismo orden que
/// `config_files_with`): dónde leer el historial y las fichas. La «carpeta
/// de la aplicación» (`:` en las rutas del cfg) es la del ejecutable si se
/// conoce (Ajustes o aprendida), si no la de datos.
pub(crate) fn data_dirs(cfg: &Config) -> Vec<super::history::DataDir> {
    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_owned());
    let manual = PathBuf::from(cfg.retroarch_dir.trim());
    let exe_dir = (!cfg.retroarch_dir.trim().is_empty()
        && (manual.join("retroarch.exe").is_file() || manual.join("retroarch").is_file()))
    .then_some(manual);
    config_files_with(cfg)
        .into_iter()
        .map(|f| {
            let dir = data_dir_of(&f.path, os(), home.as_deref());
            let app_dir = exe_dir.clone().unwrap_or_else(|| dir.clone());
            super::history::DataDir { cfg: f.path, dir, app_dir }
        })
        .collect()
}

/// Carpetas con RetroArch que se ven sin abrirlo (Steam), para «Detectar».
pub(super) fn find_exe_dirs() -> Vec<PathBuf> {
    steam_dirs()
}

/// (abierto, carpeta del ejecutable si se pudo saber).
#[cfg(windows)]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return (false, None);
        };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut running = false;
        let mut dir = None;
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let name: String = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len())],
                )
                .to_lowercase();
                if is_exe_name(&name) {
                    running = true;
                    if dir.is_none() {
                        if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, entry.th32ProcessID) {
                            let mut buf = [0u16; 1024];
                            let mut len = buf.len() as u32;
                            if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok() {
                                let exe = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
                                dir = exe.parent().map(|p| p.to_path_buf());
                            }
                            let _ = CloseHandle(h);
                        }
                    }
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        (running, dir)
    }
}

/// Nombres del ejecutable (minúsculas): `retroarch.exe`, `retroarch_debug.exe`.
pub fn is_exe_name(name: &str) -> bool {
    matches!(name, "retroarch.exe" | "retroarch_debug.exe" | "retroarch")
}

#[cfg(target_os = "linux")]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return (false, None);
    };
    let mut running = false;
    let mut dir = None;
    for e in entries.flatten() {
        let Ok(comm) = std::fs::read_to_string(e.path().join("comm")) else {
            continue;
        };
        // retroarch (paquete, AppImage, Flatpak y Snap por igual)
        if comm.trim() != "retroarch" {
            continue;
        }
        running = true;
        if dir.is_some() {
            continue;
        }
        if let Ok(env) = std::fs::read(e.path().join("environ")) {
            for var in env.split(|b| *b == 0) {
                if let Some(v) = var.strip_prefix(b"APPIMAGE=") {
                    dir = PathBuf::from(String::from_utf8_lossy(v).to_string()).parent().map(|p| p.to_path_buf());
                }
            }
        }
        if dir.is_none() {
            if let Ok(exe) = std::fs::read_link(e.path().join("exe")) {
                if !exe.to_string_lossy().contains("/.mount_") {
                    dir = exe.parent().map(|p| p.to_path_buf());
                }
            }
        }
    }
    (running, dir)
}

/// macOS: `RetroArch.app/Contents/MacOS/RetroArch` (por libproc).
#[cfg(target_os = "macos")]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    crate::procs::running_with_prefix("retroarch")
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pepomote-ra-paths-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn windows_portable_antes_que_appdata_y_todos_los_que_existen() {
        let root = tmp("win");
        let exe = root.join("RetroArch-Win64");
        let steam = root.join("steamapps/common/RetroArch");
        let appdata = root.join("appdata");
        std::fs::create_dir_all(&exe).unwrap();
        std::fs::create_dir_all(&steam).unwrap();
        std::fs::create_dir_all(appdata.join("RetroArch")).unwrap();
        std::fs::write(exe.join("retroarch.cfg"), "").unwrap();
        std::fs::write(steam.join("retroarch.cfg"), "").unwrap();
        std::fs::write(appdata.join("RetroArch/retroarch.cfg"), "").unwrap();
        let env = Env { exe_dirs: vec![exe.clone(), steam.clone()], appdata: Some(appdata.clone()), os: Os::Windows, ..Default::default() };
        let files = resolve_config_files(&env);
        let paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
        assert_eq!(paths, vec![exe.join("retroarch.cfg"), steam.join("retroarch.cfg"), appdata.join("RetroArch/retroarch.cfg")]);
        assert_eq!(files[0].why, tr!("retroarch.why_portable"));
        assert_eq!(files[2].why, tr!("retroarch.why_roaming"));
        // sin ninguno: el que RetroArch crearía junto al ejecutable
        let empty = tmp("win-empty");
        let env = Env { exe_dirs: vec![empty.clone()], appdata: Some(root.join("nada")), os: Os::Windows, ..Default::default() };
        let files = resolve_config_files(&env);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, empty.join("retroarch.cfg"));
        // sin ejecutable conocido ni appdata: nada
        let env = Env { os: Os::Windows, ..Default::default() };
        assert!(resolve_config_files(&env).is_empty());
    }

    #[test]
    fn linux_xdg_flatpak_snap_y_home() {
        let home = tmp("linux");
        std::fs::create_dir_all(home.join(".config/retroarch")).unwrap();
        std::fs::create_dir_all(home.join(".var/app/org.libretro.RetroArch/config/retroarch")).unwrap();
        std::fs::write(home.join(".config/retroarch/retroarch.cfg"), "").unwrap();
        std::fs::write(home.join(".var/app/org.libretro.RetroArch/config/retroarch/retroarch.cfg"), "").unwrap();
        std::fs::write(home.join(".retroarch.cfg"), "").unwrap();
        let env = Env { home: Some(home.clone()), os: Os::Linux, ..Default::default() };
        let files = resolve_config_files(&env);
        let paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                home.join(".config/retroarch/retroarch.cfg"),
                home.join(".var/app/org.libretro.RetroArch/config/retroarch/retroarch.cfg"),
                home.join(".retroarch.cfg"),
            ]
        );
        assert_eq!(files[1].why, tr!("retroarch.why_flatpak"));
        // XDG_CONFIG_HOME manda sobre ~/.config
        let xdg = tmp("linux-xdg");
        std::fs::create_dir_all(xdg.join("retroarch")).unwrap();
        std::fs::write(xdg.join("retroarch/retroarch.cfg"), "").unwrap();
        let env = Env { home: Some(tmp("linux-home2")), xdg_config_home: Some(xdg.clone()), os: Os::Linux, ..Default::default() };
        let files = resolve_config_files(&env);
        assert_eq!(files[0].path, xdg.join("retroarch/retroarch.cfg"));
        // sin nada: el que RetroArch creará en ~/.config
        let home = tmp("linux-empty");
        let env = Env { home: Some(home.clone()), os: Os::Linux, ..Default::default() };
        let files = resolve_config_files(&env);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, home.join(".config/retroarch/retroarch.cfg"));
    }

    #[test]
    fn macos_application_support_aunque_no_exista() {
        let sup = tmp("mac");
        let env = Env { app_support: Some(sup.clone()), os: Os::MacOs, ..Default::default() };
        let files = resolve_config_files(&env);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, sup.join("RetroArch/config/retroarch.cfg"));
    }

    #[test]
    fn la_carpeta_manual_acepta_exe_config_o_archivo() {
        let d = tmp("manual");
        assert!(manual_file("").is_none());
        assert!(manual_file(d.to_str().unwrap()).is_none(), "carpeta sin retroarch.cfg: no vale");
        std::fs::write(d.join("retroarch.cfg"), "").unwrap();
        assert_eq!(manual_file(d.to_str().unwrap()).unwrap().path, d.join("retroarch.cfg"));
        assert_eq!(manual_file(d.join("retroarch.cfg").to_str().unwrap()).unwrap().path, d.join("retroarch.cfg"));
    }

    #[test]
    fn nombres_del_ejecutable() {
        assert!(is_exe_name("retroarch.exe"));
        assert!(is_exe_name("retroarch_debug.exe"));
        assert!(!is_exe_name("retroarch_updater.exe"));
        assert!(!is_exe_name("eden.exe"));
    }
}
