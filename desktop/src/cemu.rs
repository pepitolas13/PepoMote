//! Auto-configuración de Cemu (modo Wii U): un perfil de mando emulado por
//! móvil conectado en `controllerProfiles/controller{N}.xml` (N = jugador − 1),
//! leyendo del pad DSU del móvil: el Jugador 1 es el Wii U GamePad (con
//! movimiento y pantalla táctil), los demás Pro Controller, y el que lo pida
//! un Mando Wii (Wiimote emulado con MotionPlus, puntero y su Nunchuk).
//!
//! Formato y rutas verificados contra el código fuente de Cemu 2.x:
//! - config en `%APPDATA%\Cemu` (Windows), `$XDG_CONFIG_HOME/Cemu` (Linux),
//!   `~/.var/app/info.cemu.Cemu/config/Cemu` (Flatpak), o `portable/` junto
//!   al ejecutable (y el propio directorio del exe en instalaciones antiguas
//!   con `settings.xml` al lado);
//! - la ip/puerto del cliente DSU va DENTRO del `<controller>` del perfil,
//!   así que `settings.xml` no se toca;
//! - Cemu re-guarda el perfil al cerrar (sin comentarios) pero conserva
//!   `<profile>`, que es nuestra marca de autoría.
//!
//! Nunca escribe con Cemu abierto (su config se sobreescribe al salir) y
//! deja backup `.pepomote.bak` de cualquier perfil ajeno que sustituya.

use crate::state::{cemu_layout, CemuPlayer, Config, Mode, PadKind, SharedState};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Dos móviles conectando a la vez disparan dos configuraciones: en serie.
static CONFIGURE_LOCK: Mutex<()> = Mutex::new(());

/// Marca de autoría que sobrevive al re-guardado de Cemu.
const PROFILE_MARK: &str = "<profile>PepoMote</profile>";
const DSU_IP: &str = "127.0.0.1";
const DSU_PORT: u16 = 26760;
/// Cemu admite hasta 8 mandos (`InputManager::kMaxController`).
const MAX_CONTROLLERS: u8 = 8;

/// Botones y ejes del `DSUController` de Cemu: botón i = bit i del byte 36
/// del PadData, 8+i = bit i del byte 37, 16 = Touch (byte 39); los ejes
/// empiezan en 38 (`kAxisXP`) en el orden de `Buttons2` de Controller.h.
mod dsu {
    pub const SHARE: u32 = 0;
    pub const L3: u32 = 1;
    pub const R3: u32 = 2;
    pub const OPTIONS: u32 = 3;
    pub const UP: u32 = 4;
    pub const RIGHT: u32 = 5;
    pub const DOWN: u32 = 6;
    pub const LEFT: u32 = 7;
    pub const L2: u32 = 8;
    pub const R2: u32 = 9;
    pub const L1: u32 = 10;
    pub const R1: u32 = 11;
    pub const TRIANGLE: u32 = 12;
    pub const CIRCLE: u32 = 13;
    pub const CROSS: u32 = 14;
    pub const SQUARE: u32 = 15;
    pub const TOUCH: u32 = 16;
    pub const AXIS_XP: u32 = 38;
    pub const AXIS_YP: u32 = 39;
    pub const ROT_XP: u32 = 40;
    pub const ROT_YP: u32 = 41;
    pub const TRIG_XP: u32 = 42;
    pub const TRIG_YP: u32 = 43;
    pub const AXIS_XN: u32 = 44;
    pub const AXIS_YN: u32 = 45;
    pub const ROT_XN: u32 = 46;
    pub const ROT_YN: u32 = 47;
}
use dsu::*;
use crate::state::CfgStatus;
use crate::tr;

/// Wii U GamePad (`VPADController::ButtonId`): (mapping, botón DSU). El
/// PadData de PepoMote en modo Wii U pone A→Cross, B→Circle, X→Square,
/// Y→Triangle, L/R→L1/R1, ZL/ZR→gatillos analógicos, Mic→L2, Pantalla→R2,
/// clicks→L3/R3, +/−→Options/Share, Home→Touch (dsu/mapping.rs).
const GAMEPAD: &[(u32, u32)] = &[
    (1, CROSS),   // A
    (2, CIRCLE),  // B
    (3, SQUARE),  // X
    (4, TRIANGLE), // Y
    (5, L1),      // L
    (6, R1),      // R
    (7, TRIG_XP), // ZL
    (8, TRIG_YP), // ZR
    (9, OPTIONS), // +
    (10, SHARE),  // −
    (11, UP),
    (12, DOWN),
    (13, LEFT),
    (14, RIGHT),
    (15, L3), // click stick izq
    (16, R3), // click stick dcho
    (17, AXIS_YP), // stick izq arriba
    (18, AXIS_YN),
    (19, AXIS_XN),
    (20, AXIS_XP),
    (21, ROT_YP), // stick dcho arriba
    (22, ROT_YN),
    (23, ROT_XN),
    (24, ROT_XP),
    (25, L2),    // soplar al micro
    (26, R2),    // mostrar pantalla del GamePad
    (27, TOUCH), // Home
];

/// Wii U Pro Controller (`ProController::ButtonId`).
const PRO: &[(u32, u32)] = &[
    (1, CROSS),
    (2, CIRCLE),
    (3, SQUARE),
    (4, TRIANGLE),
    (5, L1),
    (6, R1),
    (7, TRIG_XP),
    (8, TRIG_YP),
    (9, OPTIONS),
    (10, SHARE),
    (11, TOUCH), // Home
    (12, UP),
    (13, DOWN),
    (14, LEFT),
    (15, RIGHT),
    (16, L3),
    (17, R3),
    (18, AXIS_YP),
    (19, AXIS_YN),
    (20, AXIS_XN),
    (21, AXIS_XP),
    (22, ROT_YP),
    (23, ROT_YN),
    (24, ROT_XN),
    (25, ROT_XP),
];

/// Mando Wii emulado (`WiimoteController::ButtonId`), parte del propio móvil.
const WIIMOTE: &[(u32, u32)] = &[
    (1, CROSS),    // A
    (2, CIRCLE),   // B
    (3, SQUARE),   // 1
    (4, TRIANGLE), // 2
    (7, OPTIONS),  // +
    (8, SHARE),    // −
    (9, UP),
    (10, DOWN),
    (11, LEFT),
    (12, RIGHT),
    (17, TOUCH), // Home
];

/// Parte del Nunchuk (el OTRO móvil, su propio pad DSU).
const NUNCHUK: &[(u32, u32)] = &[
    (5, CIRCLE),   // Z
    (6, CROSS),    // C
    (13, AXIS_YP), // stick arriba
    (14, AXIS_YN),
    (15, AXIS_XN),
    (16, AXIS_XP),
];

/// `WPADDeviceType` de Cemu: MotionPlus solo / MotionPlus + Nunchuk.
const DEVICE_MPLS: u8 = 5;
const DEVICE_MPLS_NUNCHUK: u8 = 6;

pub type Layout = [CemuPlayer];

fn controller_node(out: &mut String, uuid: u8, name: &str, motion: bool, mappings: &[(u32, u32)]) {
    out.push_str("\t<controller>\n\t\t<api>DSUController</api>\n");
    let _ = writeln!(out, "\t\t<uuid>{uuid}</uuid>");
    let _ = writeln!(out, "\t\t<display_name>{name}</display_name>");
    if motion {
        out.push_str("\t\t<motion>true</motion>\n");
    }
    // El móvil ya aplica su zona muerta (8 %): la de Cemu, mínima
    out.push_str("\t\t<axis>\n\t\t\t<deadzone>0.1</deadzone>\n\t\t\t<range>1</range>\n\t\t</axis>\n");
    out.push_str("\t\t<rotation>\n\t\t\t<deadzone>0.1</deadzone>\n\t\t\t<range>1</range>\n\t\t</rotation>\n");
    out.push_str("\t\t<trigger>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</trigger>\n");
    let _ = writeln!(out, "\t\t<ip>{DSU_IP}</ip>\n\t\t<port>{DSU_PORT}</port>");
    out.push_str("\t\t<mappings>\n");
    for (mapping, button) in mappings {
        let _ = writeln!(
            out,
            "\t\t\t<entry>\n\t\t\t\t<mapping>{mapping}</mapping>\n\t\t\t\t<button>{button}</button>\n\t\t\t</entry>"
        );
    }
    out.push_str("\t\t</mappings>\n\t</controller>\n");
}

/// El perfil `controller{N}.xml` de un jugador.
pub fn profile_xml(pl: &CemuPlayer) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<emulated_controller>\n");
    let player = pl.index + 1;
    match pl.kind {
        PadKind::GamePad => {
            out.push_str("\t<type>Wii U GamePad</type>\n");
            let _ = writeln!(out, "\t{PROFILE_MARK}");
            controller_node(&mut out, pl.dsu_slot, &format!("PepoMote J{player} GamePad"), true, GAMEPAD);
        }
        PadKind::Pro => {
            out.push_str("\t<type>Wii U Pro Controller</type>\n");
            let _ = writeln!(out, "\t{PROFILE_MARK}");
            controller_node(&mut out, pl.dsu_slot, &format!("PepoMote J{player} Pro"), false, PRO);
        }
        PadKind::Wiimote => {
            out.push_str("\t<type>Wiimote</type>\n");
            let _ = writeln!(out, "\t{PROFILE_MARK}");
            let device = if pl.nunchuk_slot.is_some() { DEVICE_MPLS_NUNCHUK } else { DEVICE_MPLS };
            let _ = writeln!(out, "\t<device_type>{device}</device_type>");
            controller_node(&mut out, pl.dsu_slot, &format!("PepoMote J{player} Mando Wii"), true, WIIMOTE);
            if let Some(ns) = pl.nunchuk_slot {
                controller_node(&mut out, ns, &format!("PepoMote J{player} Nunchuk"), false, NUNCHUK);
            }
        }
    }
    out.push_str("</emulated_controller>\n");
    out
}

fn is_ours(content: &str) -> bool {
    content.contains(PROFILE_MARK)
}

fn profile_path(dir: &Path, index: u8) -> PathBuf {
    dir.join(format!("controller{index}.xml"))
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("xml.pepomote.bak")
}

/// Escribe el perfil solo si cambia. Un perfil AJENO (un mando real del
/// usuario) se guarda antes como `.pepomote.bak`, una sola vez.
fn write_if_changed(path: &Path, new: &str) -> Result<bool, String> {
    let original = std::fs::read_to_string(path).unwrap_or_default();
    if original == new {
        return Ok(false);
    }
    if !original.is_empty() && !is_ours(&original) {
        let bak = backup_path(path);
        if !bak.exists() {
            std::fs::copy(path, bak).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(path, new).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Un perfil nuestro en un índice sin jugador: fuera, y si sustituyó a uno
/// ajeno, ese vuelve.
fn remove_ours(path: &Path) {
    let Ok(content) = std::fs::read_to_string(path) else { return };
    if !is_ours(&content) {
        return;
    }
    let bak = backup_path(path);
    if bak.exists() {
        let _ = std::fs::rename(&bak, path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// `controllerProfiles/controller{N}.xml` por jugador; los índices sin
/// jugador se limpian si eran nuestros.
pub fn write_profiles(cfg_dir: &Path, layout: &Layout) -> Result<(), String> {
    let dir = cfg_dir.join("controllerProfiles");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for pl in layout {
        write_if_changed(&profile_path(&dir, pl.index), &profile_xml(pl))?;
    }
    for index in 0..MAX_CONTROLLERS {
        if !layout.iter().any(|p| p.index == index) {
            remove_ours(&profile_path(&dir, index));
        }
    }
    Ok(())
}

/// `settings.xml`: `<open_pad>true</open_pad>`, es decir, Cemu abre su ventana
/// «GamePad View» (la segunda pantalla) al arrancar; el receptor la captura y
/// la manda al móvil GamePad. Se conserva todo lo demás del archivo; si Cemu
/// nunca se ha abierto (no hay archivo) se crea uno mínimo que Cemu completa.
pub fn ensure_pad_window(cfg_dir: &Path) -> Result<(), String> {
    let path = cfg_dir.join("settings.xml");
    let original = std::fs::read_to_string(&path).unwrap_or_default();
    let new = if original.is_empty() {
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<content>\n    <open_pad>true</open_pad>\n</content>\n".to_owned()
    } else if original.contains("<open_pad>true</open_pad>") {
        return Ok(());
    } else if original.contains("<open_pad>false</open_pad>") {
        original.replacen("<open_pad>false</open_pad>", "<open_pad>true</open_pad>", 1)
    } else if let Some(i) = original.find("<content>") {
        let at = i + "<content>".len();
        format!("{}\n    <open_pad>true</open_pad>{}", &original[..at], &original[at..])
    } else {
        return Err(tr!("cemu.settings_no_content").to_owned());
    };
    if !original.is_empty() {
        let bak = path.with_extension("xml.pepomote.bak");
        if !bak.exists() {
            let _ = std::fs::copy(&path, bak);
        }
    }
    std::fs::write(&path, new).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Dónde está Cemu
// ---------------------------------------------------------------------------

fn name_has_cemu(p: &Path) -> bool {
    p.file_name()
        .map(|n| n.to_string_lossy().to_lowercase().contains("cemu"))
        .unwrap_or(false)
}

/// ¿Este directorio contiene el ejecutable de Cemu?
#[cfg(windows)]
fn has_exe(dir: &Path) -> bool {
    dir.join("Cemu.exe").is_file()
}

#[cfg(not(windows))]
fn has_exe(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false };
    rd.flatten().any(|e| {
        let n = e.file_name().to_string_lossy().to_lowercase();
        e.path().is_file() && (n == "cemu" || n == "cemu_release" || (n.contains("cemu") && n.ends_with(".appimage")))
    })
}

/// Sitios habituales donde vive un Cemu descomprimido (dos niveles bajo las
/// carpetas del usuario, uno bajo las del sistema).
fn search_roots() -> Vec<(PathBuf, u8)> {
    let mut roots: Vec<(PathBuf, u8)> = Vec::new();
    if let Some(u) = directories::UserDirs::new() {
        for d in [u.desktop_dir(), u.download_dir(), u.document_dir()].into_iter().flatten() {
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
        for d in ["C:\\", "C:\\Games", "C:\\Program Files", "C:\\Program Files (x86)", "D:\\", "D:\\Games"] {
            roots.push((PathBuf::from(d), 1));
        }
    }
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

/// Directorios que contienen a Cemu, encontrados por búsqueda.
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
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten().take(400) {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if has_exe(&p) {
                push(p.clone());
            } else if depth >= 2 && name_has_cemu(&p) {
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

/// Carpeta de configuración "roaming" de Cemu para instalaciones normales.
fn roaming_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Cemu"))
    }
    #[cfg(not(windows))]
    {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(x) if !x.is_empty() => Some(PathBuf::from(x).join("Cemu")),
            _ => directories::BaseDirs::new().map(|b| b.home_dir().join(".config").join("Cemu")),
        }
    }
}

#[cfg(not(windows))]
fn flatpak_dir() -> Option<PathBuf> {
    let app = directories::BaseDirs::new()?.home_dir().join(".var/app/info.cemu.Cemu");
    app.is_dir().then(|| app.join("config").join("Cemu"))
}

/// Carpetas de config de Cemu donde escribir los perfiles (todas las
/// instalaciones a la vista), a partir de los directorios del ejecutable.
pub fn config_dirs_from(exe_dirs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    fn push(dirs: &mut Vec<PathBuf>, d: PathBuf) {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for d in exe_dirs {
        let portable = d.join("portable");
        if portable.is_dir() {
            push(&mut dirs, portable);
        } else if d.join("settings.xml").is_file() {
            push(&mut dirs, d.clone()); // instalación antigua (anterior a 2.0-89): portable de serie
        }
    }
    let roaming = roaming_dir();
    let mut evidence = !exe_dirs.is_empty() || roaming.as_ref().is_some_and(|r| r.exists());
    #[cfg(not(windows))]
    {
        if let Some(f) = flatpak_dir() {
            push(&mut dirs, f);
            evidence = true;
        }
        if let Some(b) = directories::BaseDirs::new() {
            if b.home_dir().join(".local/share/Cemu").is_dir() {
                evidence = true;
            }
        }
    }
    #[cfg(windows)]
    {
        evidence |= false;
    }
    // Sin instalación portable a la vista, Cemu usa la carpeta roaming (que
    // puede no existir aún si nunca se ha abierto: se crea)
    if let Some(r) = roaming {
        if evidence && (dirs.is_empty() || r.exists()) {
            push(&mut dirs, r);
        }
    }
    if dirs.is_empty() {
        return Err(tr!("cemu.not_found").to_owned());
    }
    Ok(dirs)
}

/// Directorios del ejecutable: el de Ajustes (si lo hay) y los encontrados.
fn exe_dirs(cfg: &Config) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if !cfg.cemu_dir.trim().is_empty() {
        dirs.push(PathBuf::from(cfg.cemu_dir.trim()));
    }
    for d in find_exe_dirs() {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    dirs
}

// ---------------------------------------------------------------------------
// ¿Está Cemu abierto? (y dónde vive)
// ---------------------------------------------------------------------------

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
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut running = false;
        let mut dir = None;
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let name: String = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len())],
                )
                .to_lowercase();
                // Cemu.exe, Cemu_release.exe…
                if name.starts_with("cemu") && name.ends_with(".exe") {
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

#[cfg(target_os = "linux")]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return (false, None);
    };
    let mut running = false;
    let mut dir = None;
    for e in entries.flatten() {
        let Ok(comm) = std::fs::read_to_string(e.path().join("comm")) else { continue };
        // Cemu, Cemu_release, cemu (AppImage, Flatpak y paquetes)
        if !comm.trim().to_lowercase().starts_with("cemu") {
            continue;
        }
        running = true;
        if dir.is_some() {
            continue;
        }
        // AppImage: el exe vive en el montaje FUSE; la ruta real está en APPIMAGE
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

#[cfg(not(any(windows, target_os = "linux")))]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    (false, None)
}

// ---------------------------------------------------------------------------
// Orquestación
// ---------------------------------------------------------------------------

fn describe(layout: &Layout) -> String {
    layout
        .iter()
        .map(|p| {
            let kind = match p.kind {
                PadKind::GamePad => tr!("cemu.kind_gamepad"),
                PadKind::Pro => tr!("cemu.kind_pro"),
                PadKind::Wiimote => {
                    if p.nunchuk_slot.is_some() {
                        tr!("cemu.kind_wiimote_nunchuk")
                    } else {
                        tr!("cemu.kind_wiimote")
                    }
                }
            };
            tr!("cemu.player_kind", p.index + 1, kind)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Escribe los perfiles en todas las instalaciones de Cemu a la vista.
pub fn configure(cfg: &Config, layout: &Layout) -> Result<String, String> {
    let dirs = config_dirs_from(&exe_dirs(cfg))?;
    let gamepad = layout.iter().any(|p| p.kind == PadKind::GamePad);
    for dir in &dirs {
        write_profiles(dir, layout)?;
        if gamepad {
            // la segunda pantalla del GamePad vive en la ventana GamePad View
            ensure_pad_window(dir)?;
        }
    }
    Ok(format!(
        "{}{}",
        describe(layout),
        if dirs.len() > 1 { tr!("cemu.installs", dirs.len()) } else { String::new() }
    ))
}

/// Al ver a Cemu abierto se aprende su carpeta (para configurarlo cerrado
/// aunque viva en un sitio raro).
pub(crate) fn learn_dir(shared: &SharedState, dir: Option<PathBuf>) {
    let Some(dir) = dir else { return };
    let dir_s = dir.to_string_lossy().to_string();
    let mut s = shared.lock().unwrap();
    if s.config.cemu_dir != dir_s {
        s.config.cemu_dir = dir_s;
        s.config.save();
    }
}

/// Escribe (o deja pendiente si Cemu está abierto). `after_close`: viene
/// del vigilante, Cemu se acaba de cerrar.
fn run_configure(shared: &SharedState, layout: &Layout, after_close: bool) {
    let _serial = CONFIGURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Solo para los e2e en el propio equipo: escribir aunque el emulador esté abierto
    let (running, dir) = if std::env::var_os("PEPOMOTE_ASSUME_EMULATOR_CLOSED").is_some() {
        (false, None)
    } else {
        running_exe()
    };
    learn_dir(shared, dir);
    let (ok, msg) = if running {
        // Cemu sobreescribe sus perfiles al salir: se escribe en cuanto se
        // cierre (vigilante de auto_mode)
        shared.lock().unwrap().cemu_pending = true;
        (false, tr!("cemu.open").to_owned())
    } else {
        shared.lock().unwrap().cemu_pending = false;
        let cfg = shared.lock().unwrap().config.clone();
        match configure(&cfg, layout) {
            Ok(details) => {
                let prefix = if after_close { tr!("cemu.configured_after_close") } else { tr!("cemu.configured") };
                (true, format!("{prefix} {details}"))
            }
            Err(e) => (false, tr!("cemu.error", e)),
        }
    };
    shared.lock().unwrap().cemu_cfg_status = Some(CfgStatus { ok, text: msg.clone() });
    crate::net::notify_all(&msg);
}

/// Disparo automático (conexión/desconexión/cambio de modo o de tipo de mando).
pub fn maybe_auto_configure(shared: &SharedState) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let (auto, mode, layout) = {
            let s = shared.lock().unwrap();
            (s.config.auto_cemu, s.mode, cemu_layout(&s.players))
        };
        if auto && mode == Mode::Cemu && !layout.is_empty() {
            run_configure(&shared, &layout, false);
        }
    });
}

/// Lo pendiente (Cemu estaba abierto) se aplica cuando ya está cerrado:
/// lo llama el vigilante de `auto_mode` al ver a Cemu cerrado.
pub fn apply_pending(shared: &SharedState) {
    let (auto, mode, layout) = {
        let s = shared.lock().unwrap();
        (s.config.auto_cemu, s.mode, cemu_layout(&s.players))
    };
    if auto && mode == Mode::Cemu && !layout.is_empty() {
        run_configure(shared, &layout, true);
    } else {
        shared.lock().unwrap().cemu_pending = false;
    }
}

/// Botón manual de la ventana.
pub fn configure_now(shared: &SharedState) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let mut layout = cemu_layout(&shared.lock().unwrap().players);
        if layout.is_empty() {
            layout.push(CemuPlayer { index: 0, kind: PadKind::GamePad, dsu_slot: 0, nunchuk_slot: None });
        }
        run_configure(&shared, &layout, false);
    });
}

/// Botón «Detectar» de Ajustes: busca Cemu y guarda su carpeta.
pub fn detect_now(shared: &SharedState) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let (_, dir) = running_exe();
        let found = dir.into_iter().chain(find_exe_dirs()).next();
        let mut s = shared.lock().unwrap();
        match found {
            Some(d) => {
                s.config.cemu_dir = d.to_string_lossy().to_string();
                s.config.save();
                s.cemu_cfg_status = Some(CfgStatus::ok(tr!("cemu.found", d.display())));
            }
            None => {
                s.cemu_cfg_status = Some(CfgStatus::warn(tr!("cemu.not_found_manual").to_owned()));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pepomote-cemu-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn gamepad(index: u8, slot: u8) -> CemuPlayer {
        CemuPlayer { index, kind: PadKind::GamePad, dsu_slot: slot, nunchuk_slot: None }
    }

    fn pro(index: u8, slot: u8) -> CemuPlayer {
        CemuPlayer { index, kind: PadKind::Pro, dsu_slot: slot, nunchuk_slot: None }
    }

    fn wii(index: u8, slot: u8, nunchuk: Option<u8>) -> CemuPlayer {
        CemuPlayer { index, kind: PadKind::Wiimote, dsu_slot: slot, nunchuk_slot: nunchuk }
    }

    /// (mapping → button) de un XML, en orden.
    fn mappings(xml: &str) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let mut rest = xml;
        while let Some(i) = rest.find("<mapping>") {
            let m: u32 = rest[i + 9..].split('<').next().unwrap().parse().unwrap();
            let j = rest[i..].find("<button>").unwrap() + i;
            let b: u32 = rest[j + 8..].split('<').next().unwrap().parse().unwrap();
            out.push((m, b));
            rest = &rest[j + 8..];
        }
        out
    }

    /// Comprobación mínima de XML bien formado: etiquetas equilibradas.
    fn well_formed(xml: &str) -> bool {
        let mut stack: Vec<&str> = Vec::new();
        for tag in xml.split('<').skip(1) {
            let tag = tag.split('>').next().unwrap();
            if tag.starts_with('?') {
                continue;
            }
            if let Some(name) = tag.strip_prefix('/') {
                if stack.pop() != Some(name) {
                    return false;
                }
            } else {
                stack.push(tag.split_whitespace().next().unwrap());
            }
        }
        stack.is_empty()
    }

    #[test]
    fn perfil_gamepad_jugador_1() {
        let xml = profile_xml(&gamepad(0, 0));
        assert!(well_formed(&xml));
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<emulated_controller>\n\t<type>Wii U GamePad</type>"));
        assert!(xml.contains(PROFILE_MARK));
        assert!(!xml.contains("device_type"));
        assert!(xml.contains("<api>DSUController</api>\n\t\t<uuid>0</uuid>"));
        assert!(xml.contains("<motion>true</motion>"), "el GamePad lleva giroscopio");
        assert!(xml.contains("<ip>127.0.0.1</ip>\n\t\t<port>26760</port>"));
        let m = mappings(&xml);
        assert_eq!(m.len(), 27, "los 27 mapeos del GamePad");
        assert!(m.contains(&(1, 14)), "A → Cross");
        assert!(m.contains(&(2, 13)), "B → Circle");
        assert!(m.contains(&(3, 15)), "X → Square");
        assert!(m.contains(&(4, 12)), "Y → Triangle");
        assert!(m.contains(&(7, 42)) && m.contains(&(8, 43)), "ZL/ZR → gatillos analógicos");
        assert!(m.contains(&(17, 39)) && m.contains(&(18, 45)) && m.contains(&(19, 44)) && m.contains(&(20, 38)));
        assert!(m.contains(&(21, 41)) && m.contains(&(22, 47)) && m.contains(&(23, 46)) && m.contains(&(24, 40)));
        assert!(m.contains(&(25, 8)) && m.contains(&(26, 9)), "Mic → L2, Pantalla → R2");
        assert!(m.contains(&(27, 16)), "Home → Touch");
        assert_eq!(xml.matches("<controller>").count(), 1);
    }

    #[test]
    fn perfil_pro_jugador_2() {
        let xml = profile_xml(&pro(1, 1));
        assert!(well_formed(&xml));
        assert!(xml.contains("<type>Wii U Pro Controller</type>"));
        assert!(xml.contains("<uuid>1</uuid>"));
        assert!(!xml.contains("<motion>"), "un Pro Controller no tiene movimiento");
        let m = mappings(&xml);
        assert_eq!(m.len(), 25);
        assert!(m.contains(&(11, 16)), "Home (11 en el Pro) → Touch");
        assert!(m.contains(&(18, 39)) && m.contains(&(25, 40)));
    }

    #[test]
    fn perfil_mando_wii_con_y_sin_nunchuk() {
        let xml = profile_xml(&wii(0, 0, Some(3)));
        assert!(well_formed(&xml));
        assert!(xml.contains("<type>Wiimote</type>"));
        assert!(xml.contains("<device_type>6</device_type>"), "MotionPlus + Nunchuk");
        assert_eq!(xml.matches("<controller>").count(), 2, "el Nunchuk es el otro móvil");
        assert!(xml.contains("<uuid>0</uuid>") && xml.contains("<uuid>3</uuid>"));
        let m = mappings(&xml);
        assert!(m.contains(&(1, 14)) && m.contains(&(2, 13)) && m.contains(&(3, 15)) && m.contains(&(4, 12)));
        assert!(m.contains(&(17, 16)), "Home → Touch");
        assert!(m.contains(&(5, 13)) && m.contains(&(6, 14)), "Z → Circle, C → Cross del pad 3");
        assert!(m.contains(&(13, 39)) && m.contains(&(14, 45)) && m.contains(&(15, 44)) && m.contains(&(16, 38)));
        // el movimiento solo en el mando (el Nunchuk de Cemu no lee un segundo IMU)
        assert_eq!(xml.matches("<motion>true</motion>").count(), 1);
        let solo = profile_xml(&wii(1, 1, None));
        assert!(solo.contains("<device_type>5</device_type>"));
        assert_eq!(solo.matches("<controller>").count(), 1);
    }

    #[test]
    fn escribe_por_jugador_y_limpia_los_suyos() {
        let dir = tmp_dir("perfiles");
        write_profiles(&dir, &[gamepad(0, 0), pro(1, 1)]).unwrap();
        let profiles = dir.join("controllerProfiles");
        assert!(profiles.join("controller0.xml").is_file());
        assert!(profiles.join("controller1.xml").is_file());
        // se va el jugador 2: su perfil (nuestro) desaparece; el 1 sigue
        write_profiles(&dir, &[gamepad(0, 0)]).unwrap();
        assert!(profiles.join("controller0.xml").is_file());
        assert!(!profiles.join("controller1.xml").exists());
        // un perfil ajeno en un índice sin jugador no se toca
        std::fs::write(profiles.join("controller2.xml"), "<emulated_controller><type>Wii U Pro Controller</type></emulated_controller>").unwrap();
        write_profiles(&dir, &[gamepad(0, 0)]).unwrap();
        assert!(profiles.join("controller2.xml").is_file());
    }

    #[test]
    fn backup_del_ajeno_y_restauracion_al_irse() {
        let dir = tmp_dir("backup");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        let ajeno = "<emulated_controller><type>Wii U GamePad</type><controller><api>XInput</api></controller></emulated_controller>";
        std::fs::write(profiles.join("controller1.xml"), ajeno).unwrap();
        write_profiles(&dir, &[gamepad(0, 0), pro(1, 1)]).unwrap();
        let bak = profiles.join("controller1.xml.pepomote.bak");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), ajeno, "el ajeno queda a salvo");
        assert!(is_ours(&std::fs::read_to_string(profiles.join("controller1.xml")).unwrap()));
        // segunda pasada: el backup no se pisa con nuestra versión
        write_profiles(&dir, &[gamepad(0, 0), pro(1, 1)]).unwrap();
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), ajeno);
        // se va el jugador 2: vuelve su mando real
        write_profiles(&dir, &[gamepad(0, 0)]).unwrap();
        assert_eq!(std::fs::read_to_string(profiles.join("controller1.xml")).unwrap(), ajeno);
        assert!(!bak.exists());
    }

    #[test]
    fn sin_cambios_no_se_reescribe() {
        let dir = tmp_dir("nochange");
        let path = dir.join("controller0.xml");
        let xml = profile_xml(&gamepad(0, 0));
        assert!(write_if_changed(&path, &xml).unwrap());
        assert!(!write_if_changed(&path, &xml).unwrap());
        assert!(!backup_path(&path).exists(), "lo nuestro no se respalda");
    }

    #[test]
    fn ventana_del_gamepad_en_settings_xml() {
        let dir = tmp_dir("openpad");
        let path = dir.join("settings.xml");
        // sin archivo: uno mínimo
        ensure_pad_window(&dir).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("<content>\n    <open_pad>true</open_pad>\n</content>"));
        // false → true, conservando el resto y con backup del original
        let cfg = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<content>\n    <logflag>0</logflag>\n    <open_pad>false</open_pad>\n    <pad_size>\n        <x>-1</x>\n    </pad_size>\n</content>\n";
        std::fs::write(&path, cfg).unwrap();
        let _ = std::fs::remove_file(path.with_extension("xml.pepomote.bak"));
        ensure_pad_window(&dir).unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains("<open_pad>true</open_pad>") && out.contains("<logflag>0</logflag>") && out.contains("<pad_size>"));
        assert_eq!(std::fs::read_to_string(path.with_extension("xml.pepomote.bak")).unwrap(), cfg);
        // ya true: no se toca
        ensure_pad_window(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), out);
        // sin la clave: se inserta tras <content>
        std::fs::write(&path, "<content>\n    <logflag>0</logflag>\n</content>\n").unwrap();
        ensure_pad_window(&dir).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("<content>\n    <open_pad>true</open_pad>\n    <logflag>0</logflag>"));
    }

    #[test]
    fn carpetas_de_config_segun_instalacion() {
        let base = tmp_dir("dirs");
        // portable: manda portable/
        let portable = base.join("cemu-portable");
        std::fs::create_dir_all(portable.join("portable")).unwrap();
        let dirs = config_dirs_from(&[portable.clone()]).unwrap();
        assert_eq!(dirs[0], portable.join("portable"));
        // instalación antigua: settings.xml junto al exe
        let old = base.join("cemu-old");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("settings.xml"), "<content/>").unwrap();
        let dirs = config_dirs_from(&[old.clone()]).unwrap();
        assert_eq!(dirs[0], old);
        // instalación normal (2.0-89+): carpeta roaming, aunque no exista aún
        let normal = base.join("Cemu_2.6");
        std::fs::create_dir_all(&normal).unwrap();
        let dirs = config_dirs_from(&[normal]).unwrap();
        let roaming = roaming_dir().unwrap();
        assert!(dirs.contains(&roaming), "{dirs:?}");
    }
}
