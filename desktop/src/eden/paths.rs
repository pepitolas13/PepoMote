//! Localización conservadora de instalaciones y procesos de Eden.
use crate::state::Config;
use crate::tr;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct Env {
    pub exe_dirs: Vec<PathBuf>,
    pub appdata: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(super) struct ConfigFile {
    pub path: PathBuf,
    pub why: &'static str,
}

pub(super) fn resolve_config_files(env: &Env) -> Vec<ConfigFile> {
    let mut found = Vec::new();
    let mut non_portable = false;
    for exe in &env.exe_dirs {
        let user = exe.join("user");
        if user.is_dir() {
            let path = user.join("config/qt-config.ini");
            if !found.iter().any(|c: &ConfigFile| c.path == path) {
                found.push(ConfigFile {
                    path,
                    why: tr!("eden.why_portable"),
                });
            }
        } else {
            non_portable = true;
        }
    }
    let roaming = if let Some(appdata) = &env.appdata {
        Some(appdata.join("eden/config/qt-config.ini"))
    } else {
        env.xdg_config_home
            .as_ref()
            .filter(|p| !p.as_os_str().is_empty())
            .cloned()
            .or_else(|| env.home.as_ref().map(|p| p.join(".config")))
            .map(|p| p.join("eden/qt-config.ini"))
    };
    if let Some(path) = roaming {
        let evidence = path.parent().is_some_and(|p| p.is_dir());
        if non_portable || evidence {
            found.push(ConfigFile {
                path,
                why: tr!("eden.why_roaming"),
            });
        }
    }
    found
}

pub(super) fn config_files_with(cfg: &Config) -> Vec<ConfigFile> {
    if let Some(dir) = std::env::var_os("PEPOMOTE_EDEN_DIR") {
        return vec![ConfigFile {
            path: PathBuf::from(dir).join("qt-config.ini"),
            why: tr!("eden.why_manual"),
        }];
    }
    let manual = PathBuf::from(cfg.eden_dir.trim());
    if !cfg.eden_dir.trim().is_empty() {
        // Ajustes acepta tanto la carpeta del ejecutable como la de config.
        if manual.is_file() && manual.file_name().is_some_and(|n| n == "qt-config.ini") {
            return vec![ConfigFile {
                path: manual,
                why: tr!("eden.why_manual"),
            }];
        }
        if manual.join("qt-config.ini").is_file()
            || manual.file_name().is_some_and(|n| n == "config")
        {
            return vec![ConfigFile {
                path: manual.join("qt-config.ini"),
                why: tr!("eden.why_manual"),
            }];
        }
    }
    let mut dirs = find_exe_dirs();
    if !cfg.eden_dir.trim().is_empty() && manual.is_dir() && !dirs.contains(&manual) {
        dirs.insert(0, manual);
    }
    let env = Env {
        exe_dirs: dirs,
        appdata: if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            None
        },
        xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        home: directories::BaseDirs::new().map(|b| b.home_dir().to_owned()),
    };
    resolve_config_files(&env)
}

fn name_has_eden(p: &Path) -> bool {
    p.file_name()
        .map(|n| n.to_string_lossy().to_lowercase().contains("eden"))
        .unwrap_or(false)
}

/// ¿Este directorio contiene el ejecutable de Eden?
#[cfg(windows)]
fn has_exe(dir: &Path) -> bool {
    dir.join("eden.exe").is_file() || dir.join("eden-cli.exe").is_file()
}

#[cfg(not(windows))]
fn has_exe(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.flatten().any(|e| {
        let n = e.file_name().to_string_lossy().to_lowercase();
        e.path().is_file()
            && (n == "eden" || n == "eden-cli" || (n.contains("eden") && n.ends_with(".appimage")))
    })
}

/// Sitios habituales donde vive un Eden descomprimido (dos niveles bajo las
/// carpetas del usuario, uno bajo las del sistema).
fn search_roots() -> Vec<(PathBuf, u8)> {
    let mut roots: Vec<(PathBuf, u8)> = Vec::new();
    if let Some(u) = directories::UserDirs::new() {
        for d in [u.desktop_dir(), u.download_dir(), u.document_dir()]
            .into_iter()
            .flatten()
        {
            roots.push((d.to_path_buf(), 2));
        }
        roots.push((u.home_dir().to_path_buf(), 1));
        #[cfg(not(windows))]
        {
            roots.push((u.home_dir().join("Applications"), 1));
            roots.push((u.home_dir().join(".local/bin"), 1));
            roots.push((u.home_dir().join("Descargas"), 2));
            roots.push((u.home_dir().join("Downloads"), 2));
        }
    }
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            roots.push((PathBuf::from(local).join("Programs"), 1));
        }
        for d in [
            "C:\\",
            "C:\\Games",
            "C:\\Program Files",
            "C:\\Program Files (x86)",
            "D:\\",
            "D:\\Games",
        ] {
            roots.push((PathBuf::from(d), 1));
        }
    }
    #[cfg(target_os = "macos")]
    roots.push((PathBuf::from("/Applications"), 1));
    #[cfg(not(windows))]
    {
        roots.push((PathBuf::from("/opt"), 1));
        if let Some(path) = std::env::var_os("PATH") {
            for d in std::env::split_paths(&path) {
                roots.push((d, 0));
            }
        }
    }
    roots
}

/// macOS: `Eden.app` → `Eden.app/Contents/MacOS` si ahí está el ejecutable.
fn app_bundle_exe_dir(p: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if p.extension().is_some_and(|e| e == "app") {
            return crate::procs::bundle_exe_dir(p, has_exe);
        }
    }
    let _ = p;
    None
}

/// Directorios que contienen a Eden, encontrados por búsqueda.
pub fn find_exe_dirs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut push = |d: PathBuf| {
        if !found.contains(&d) {
            found.push(d);
        }
    };
    for (root, depth) in search_roots() {
        if has_exe(&root) {
            push(root.clone());
        }
        if depth == 0 {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in rd.flatten().take(400) {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if has_exe(&p) {
                push(p.clone());
            } else if let Some(inner) = app_bundle_exe_dir(&p) {
                push(inner);
            } else if depth >= 2 && name_has_eden(&p) {
                if let Ok(rd2) = std::fs::read_dir(&p) {
                    for e2 in rd2.flatten().take(100) {
                        let p2 = e2.path();
                        if p2.is_dir() && has_exe(&p2) {
                            push(p2);
                        }
                    }
                }
            }
        }
    }
    found
}

/// (abierto, carpeta del ejecutable si se pudo saber).
#[cfg(windows)]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return (false, None);
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut running = false;
        let mut dir = None;
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let name: String = String::from_utf16_lossy(
                    &entry.szExeFile[..entry
                        .szExeFile
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(entry.szExeFile.len())],
                )
                .to_lowercase();
                // Eden.exe, Eden_release.exe…
                if matches!(name.as_str(), "eden.exe" | "eden-cli.exe") {
                    running = true;
                    if dir.is_none() {
                        if let Ok(h) = OpenProcess(
                            PROCESS_QUERY_LIMITED_INFORMATION,
                            false,
                            entry.th32ProcessID,
                        ) {
                            let mut buf = [0u16; 1024];
                            let mut len = buf.len() as u32;
                            if QueryFullProcessImageNameW(
                                h,
                                PROCESS_NAME_WIN32,
                                PWSTR(buf.as_mut_ptr()),
                                &mut len,
                            )
                            .is_ok()
                            {
                                let exe =
                                    PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
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
        // Eden, Eden_release, eden (AppImage, Flatpak y paquetes)
        if !matches!(comm.trim().to_lowercase().as_str(), "eden" | "eden-cli") {
            continue;
        }
        running = true;
        if dir.is_some() {
            continue;
        }
        // En Unix Eden busca user/ en su cwd, no junto al binario.
        // Guardar la carpeta real de config también para la siguiente sesión.
        if let Ok(cwd) = std::fs::read_link(e.path().join("cwd")) {
            if cwd.join("user").is_dir() {
                dir = Some(cwd.join("user/config"));
                continue;
            }
        }
        // AppImage: el exe vive en el montaje FUSE; la ruta real está en APPIMAGE
        if let Ok(env) = std::fs::read(e.path().join("environ")) {
            for var in env.split(|b| *b == 0) {
                if let Some(v) = var.strip_prefix(b"APPIMAGE=") {
                    dir = PathBuf::from(String::from_utf8_lossy(v).to_string())
                        .parent()
                        .map(|p| p.to_path_buf());
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

/// macOS: `Eden.app/Contents/MacOS/Eden` (por libproc, sin bifurcar `ps`).
#[cfg(target_os = "macos")]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    crate::procs::running_with_prefix("eden")
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    (false, None)
}
