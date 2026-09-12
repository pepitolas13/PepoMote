//! Auto-configuración de Dolphin para multijugador: deja el adaptador
//! Bluetooth EMULADO en Dolphin.ini (con el acceso directo a un adaptador
//! real, Dolphin ignora los mandos emulados y el juego se cierra al arrancar),
//! escribe en WiimoteNew.ini tantos mandos emulados como móviles conectados
//! ([Wiimote1..N] con Source=1 + mapeo DSU validado en h3) y deja los demás
//! en Source=0: un mando emulado de más aparece en pantalla en los juegos y
//! molesta. Registra el servidor en DSUClient.ini. Nunca escribe con Dolphin
//! abierto (su config se sobreescribe al salir): lo deja pendiente y lo
//! aplica en cuanto se cierra. Siempre deja backup .pepomote.bak. Busca la
//! carpeta de usuario con la misma lógica que Dolphin (portable, registro,
//! Documentos, AppData; XDG/Flatpak en Linux).

use crate::state::{Mode, SharedState};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use crate::state::CfgStatus;
use crate::tr;

/// Dos móviles conectando a la vez disparan dos configuraciones: en serie,
/// o se pisan el leer-modificar-escribir del mismo INI.
static CONFIGURE_LOCK: Mutex<()> = Mutex::new(());

/// Mapeo validado contra Dolphin real (h3): botones por bytes analógicos,
/// cruceta "Pad N/S/W/E", IMU completo y recentrado por botón Touch.
/// {DEV} = índice de pad del cliente DSU (slot).
const MAPPING: &str = "Device = DSUClient/{DEV}/PepoMote
Source = 1
Buttons/A = Cross
Buttons/B = Circle
Buttons/1 = Square
Buttons/2 = Triangle
Buttons/- = Share
Buttons/+ = Options
Buttons/Home = PS
D-Pad/Up = `Pad N`
D-Pad/Down = `Pad S`
D-Pad/Left = `Pad W`
D-Pad/Right = `Pad E`
IMUAccelerometer/Up = `Accel Up`
IMUAccelerometer/Down = `Accel Down`
IMUAccelerometer/Left = `Accel Left`
IMUAccelerometer/Right = `Accel Right`
IMUAccelerometer/Forward = `Accel Forward`
IMUAccelerometer/Backward = `Accel Backward`
IMUGyroscope/Pitch Up = `Gyro Pitch Up`
IMUGyroscope/Pitch Down = `Gyro Pitch Down`
IMUGyroscope/Roll Left = `Gyro Roll Left`
IMUGyroscope/Roll Right = `Gyro Roll Right`
IMUGyroscope/Yaw Left = `Gyro Yaw Left`
IMUGyroscope/Yaw Right = `Gyro Yaw Right`
IMUGyroscope/Dead Zone = 3.
IMUPointer/Enabled = True
IMUPointer/Recenter = `Touch Button`
IMUPointer/Total Yaw = 25.000000000000000
IMUPointer/Total Pitch = 20.000000000000000
Options/Battery = `Battery`
";

/// Nunchuk emulado alimentado por el pad DSU del OTRO móvil ({NDEV} = su
/// slot): stick, C/Z (Cross/Circle de ese pad) y su acelerómetro real
/// (Dolphin lo usa tal cual para agitar/inclinar). Ver protocol/DSU.md.
const NUNCHUK_MAPPING: &str = "Extension = Nunchuk
Nunchuk/Buttons/C = `DSUClient/{NDEV}/PepoMote:Cross`
Nunchuk/Buttons/Z = `DSUClient/{NDEV}/PepoMote:Circle`
Nunchuk/Stick/Up = `DSUClient/{NDEV}/PepoMote:Left Y+`
Nunchuk/Stick/Down = `DSUClient/{NDEV}/PepoMote:Left Y-`
Nunchuk/Stick/Left = `DSUClient/{NDEV}/PepoMote:Left X-`
Nunchuk/Stick/Right = `DSUClient/{NDEV}/PepoMote:Left X+`
Nunchuk/IMUAccelerometer/Up = `DSUClient/{NDEV}/PepoMote:Accel Up`
Nunchuk/IMUAccelerometer/Down = `DSUClient/{NDEV}/PepoMote:Accel Down`
Nunchuk/IMUAccelerometer/Left = `DSUClient/{NDEV}/PepoMote:Accel Left`
Nunchuk/IMUAccelerometer/Right = `DSUClient/{NDEV}/PepoMote:Accel Right`
Nunchuk/IMUAccelerometer/Forward = `DSUClient/{NDEV}/PepoMote:Accel Forward`
Nunchuk/IMUAccelerometer/Backward = `DSUClient/{NDEV}/PepoMote:Accel Backward`
";

/// Jugadores a configurar: (slot DSU del Wiimote, slot DSU de su Nunchuk).
pub type Layout = [(u8, Option<u8>)];

const DSU_ENTRY: &str = "PepoMote:127.0.0.1:26760";

// ---------------------------------------------------------------------------
// Dónde guarda Dolphin su configuración (la misma lógica que Dolphin)
// ---------------------------------------------------------------------------
//
// Dolphin (UICommon::SetUserDirectory) elige su carpeta de usuario así:
//  Windows: `portable.txt` junto al exe → `<exe>\User`; registro
//    HKCU\Software\Dolphin Emulator: `LocalUserConfig` = 1 → `<exe>\User`,
//    `UserConfigPath` → esa ruta; `Documentos\Dolphin Emulator` si existe
//    (instalaciones antiguas); si no, `AppData\Roaming\Dolphin Emulator`.
//  Linux: `portable.txt` junto al binario → `<dir>/User`; `$DOLPHIN_EMU_USERPATH`;
//    `~/.dolphin-emu` si existe (antiguo); si no, `$XDG_CONFIG_HOME/dolphin-emu`.
//    Flatpak: `~/.var/app/org.DolphinEmu.dolphin-emu/config/dolphin-emu`.
// Antes solo se miraban Documentos y AppData, y solo si ya existían: un
// Dolphin portable (RetroBat, LaunchBox, una carpeta suelta) se quedaba sin
// configurar mientras el receptor decía «configurado» (escribía en una
// carpeta vieja), y Dolphin enseñaba el mando desconectado.

/// Lo que se sabe del sistema para decidir las carpetas (inyectable en tests).
pub struct Env {
    /// Carpetas con el ejecutable de Dolphin (en marcha, aprendida, buscadas).
    pub exe_dirs: Vec<PathBuf>,
    /// Windows: HKCU\Software\Dolphin Emulator\LocalUserConfig = 1.
    pub reg_local_user_config: bool,
    /// Windows: HKCU\Software\Dolphin Emulator\UserConfigPath.
    pub reg_user_config_path: Option<PathBuf>,
    /// Windows: Documentos\Dolphin Emulator (solo si existe).
    pub documents_legacy: Option<PathBuf>,
    /// Windows: AppData\Roaming\Dolphin Emulator (exista o no).
    pub appdata: Option<PathBuf>,
    /// Linux: $DOLPHIN_EMU_USERPATH.
    pub userpath_env: Option<PathBuf>,
    /// Linux: ~/.dolphin-emu (solo si existe).
    pub legacy_home: Option<PathBuf>,
    /// Linux: $XDG_CONFIG_HOME/dolphin-emu (exista o no).
    pub xdg_config: Option<PathBuf>,
    /// Linux: carpeta de config del Flatpak (solo si existe).
    pub flatpak: Option<PathBuf>,
}

/// Una carpeta `Config` de Dolphin y de dónde sale.
#[derive(Clone, Debug, PartialEq)]
pub struct UserDir {
    pub config: PathBuf,
    pub why: &'static str,
}

/// Carpetas `Config` donde escribir, por prioridad y sin repetir. Las que
/// dependen del ejecutable van primero (son las que usa ESE Dolphin). Solo
/// se devuelven carpetas que existen o que Dolphin crearía tal cual (AppData /
/// XDG) cuando hay rastro de Dolphin.
pub fn resolve_user_dirs(env: &Env) -> Vec<UserDir> {
    let mut out: Vec<UserDir> = Vec::new();
    let mut push = |config: PathBuf, why: &'static str| {
        if !out.iter().any(|d| d.config == config) {
            out.push(UserDir { config, why });
        }
    };
    for exe in &env.exe_dirs {
        if exe.join("portable.txt").is_file() {
            push(exe.join("User").join("Config"), tr!("dolphin.why_portable"));
        } else if env.reg_local_user_config {
            push(exe.join("User").join("Config"), tr!("dolphin.why_local"));
        }
    }
    if let Some(p) = &env.reg_user_config_path {
        push(p.join("Config"), tr!("dolphin.why_registry"));
    }
    if let Some(p) = &env.userpath_env {
        push(p.join("Config"), "DOLPHIN_EMU_USERPATH");
    }
    if let Some(p) = &env.flatpak {
        push(p.clone(), "Flatpak");
    }
    if let Some(p) = &env.legacy_home {
        push(p.join("Config"), "~/.dolphin-emu");
    } else if let Some(p) = &env.xdg_config {
        push(p.clone(), "~/.config/dolphin-emu");
    }
    if let Some(p) = &env.documents_legacy {
        push(p.join("Config"), tr!("dolphin.why_documents"));
    } else if let Some(p) = &env.appdata {
        push(p.join("Config"), "AppData\\Dolphin Emulator");
    }
    out
}

/// Lo que hay en esta máquina.
fn env_from_system(cfg_dolphin_dir: &str) -> Env {
    let mut exe_dirs: Vec<PathBuf> = Vec::new();
    let mut push_exe = |d: PathBuf| {
        if !exe_dirs.contains(&d) {
            exe_dirs.push(d);
        }
    };
    if let (_, Some(d)) = running_exe() {
        push_exe(d);
    }
    if !cfg_dolphin_dir.is_empty() {
        let d = PathBuf::from(cfg_dolphin_dir);
        if has_exe(&d) {
            push_exe(d);
        }
    }
    for d in find_exe_dirs() {
        push_exe(d);
    }
    #[cfg(windows)]
    {
        let (reg_local, reg_path) = registry_user_config();
        let documents = directories::UserDirs::new()
            .and_then(|u| u.document_dir().map(|d| d.join("Dolphin Emulator")))
            .filter(|d| d.is_dir());
        let appdata = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Dolphin Emulator"));
        Env {
            exe_dirs,
            reg_local_user_config: reg_local,
            reg_user_config_path: reg_path,
            documents_legacy: documents,
            appdata,
            userpath_env: None,
            legacy_home: None,
            xdg_config: None,
            flatpak: None,
        }
    }
    #[cfg(not(windows))]
    {
        let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
        let xdg = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(x) if !x.is_empty() => Some(PathBuf::from(x).join("dolphin-emu")),
            _ => home.as_ref().map(|h| h.join(".config").join("dolphin-emu")),
        };
        Env {
            exe_dirs,
            reg_local_user_config: false,
            reg_user_config_path: None,
            documents_legacy: None,
            appdata: None,
            userpath_env: std::env::var_os("DOLPHIN_EMU_USERPATH").map(PathBuf::from),
            legacy_home: home.as_ref().map(|h| h.join(".dolphin-emu")).filter(|d| d.is_dir()),
            xdg_config: xdg,
            flatpak: home
                .as_ref()
                .map(|h| h.join(".var/app/org.DolphinEmu.dolphin-emu/config/dolphin-emu"))
                .filter(|d| d.is_dir()),
        }
    }
}

#[cfg(windows)]
fn registry_user_config() -> (bool, Option<PathBuf>) {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ};
    unsafe {
        let mut local: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        let local_ok = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Dolphin Emulator"),
            w!("LocalUserConfig"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut local as *mut u32 as *mut core::ffi::c_void),
            Some(&mut len),
        )
        .is_ok()
            && local != 0;
        let mut buf = [0u16; 1024];
        let mut blen = (buf.len() * 2) as u32;
        let path = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Dolphin Emulator"),
            w!("UserConfigPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            Some(&mut blen),
        )
        .is_ok()
        .then(|| {
            let n = (blen as usize / 2).min(buf.len());
            let s = String::from_utf16_lossy(&buf[..n]);
            PathBuf::from(s.trim_end_matches('\0').trim())
        })
        .filter(|p| !p.as_os_str().is_empty());
        (local_ok, path)
    }
}

/// ¿Este directorio contiene el ejecutable de Dolphin?
#[cfg(windows)]
fn has_exe(dir: &Path) -> bool {
    ["Dolphin.exe", "DolphinQt.exe", "Dolphin-x64.exe", "DolphinQt2.exe"].iter().any(|n| dir.join(n).is_file())
}

#[cfg(not(windows))]
fn has_exe(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false };
    rd.flatten().any(|e| {
        let n = e.file_name().to_string_lossy().to_lowercase();
        e.path().is_file() && (n == "dolphin-emu" || n == "dolphin-emu-nogui" || (n.contains("dolphin") && n.ends_with(".appimage")))
    })
}

fn name_has_dolphin(p: &Path) -> bool {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase().contains("dolphin")).unwrap_or(false)
}

fn name_is_emulators(p: &Path) -> bool {
    p.file_name()
        .map(|n| {
            let n = n.to_string_lossy().to_lowercase();
            n.starts_with("emulator") || n.starts_with("emulador") || n == "emus" || n == "emu"
        })
        .unwrap_or(false)
}

/// Sitios habituales de un Dolphin suelto o de un frontend (RetroBat,
/// LaunchBox, EmuDeck…): dos niveles bajo las carpetas del usuario, uno
/// bajo las del sistema, y `emulators\dolphin*` en cualquiera de ellos.
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
            roots.push((PathBuf::from(d), 2));
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

/// Directorios que contienen a Dolphin, encontrados por búsqueda superficial.
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
            } else if depth >= 2 && (name_has_dolphin(&p) || name_is_emulators(&p)) {
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

/// Carpetas `Config` de Dolphin donde escribir (todas las instalaciones a la
/// vista). Solo para los e2e, `PEPOMOTE_DOLPHIN_DIR` las sustituye.
pub fn config_dirs_with(cfg_dolphin_dir: &str) -> Vec<UserDir> {
    if let Some(d) = std::env::var_os("PEPOMOTE_DOLPHIN_DIR") {
        let d = PathBuf::from(d);
        let _ = std::fs::create_dir_all(&d);
        return vec![UserDir { config: d, why: "PEPOMOTE_DOLPHIN_DIR" }];
    }
    let env = env_from_system(cfg_dolphin_dir);
    let evidence = !env.exe_dirs.is_empty();
    resolve_user_dirs(&env)
        .into_iter()
        .filter(|d| {
            // las carpetas que Dolphin crearía él mismo (AppData, XDG) solo
            // se crean aquí si hay rastro de Dolphin (ejecutable a la vista)
            if d.config.is_dir() {
                return true;
            }
            let creatable = d.why.starts_with("AppData") || d.why.starts_with("~/.config") || d.why.starts_with("portable") || d.why.contains("registro") || d.why == "DOLPHIN_EMU_USERPATH";
            creatable && evidence && std::fs::create_dir_all(&d.config).is_ok()
        })
        .collect()
}

/// `PepoMote --dolphin-dirs`: qué carpetas de Dolphin ve este equipo (para
/// diagnosticar un Dolphin que no se deja configurar). Devuelve true si el
/// argumento estaba (el receptor no arranca).
pub fn print_dirs_from_args() -> bool {
    if !std::env::args().any(|a| a == "--dolphin-dirs") {
        return false;
    }
    let cfg_dir = crate::state::Config::load().dolphin_dir;
    let (running, exe) = running_exe();
    println!("Dolphin abierto: {}{}", if running { "sí" } else { "no" }, exe.map(|e| format!(" ({})", e.display())).unwrap_or_default());
    println!("Carpeta de Dolphin en Ajustes: {}", if cfg_dir.is_empty() { "(automática)" } else { &cfg_dir });
    for d in find_exe_dirs() {
        println!("Ejecutable encontrado en: {}", d.display());
    }
    let dirs = config_dirs_with(&cfg_dir);
    if dirs.is_empty() {
        println!("Carpetas de configuración: ninguna (Dolphin no se ha abierto nunca aquí, o está en un sitio raro)");
    }
    for d in dirs {
        let src = std::fs::read_to_string(d.config.join("WiimoteNew.ini")).unwrap_or_default();
        let wm1 = src
            .split("[Wiimote1]")
            .nth(1)
            .and_then(|b| b.lines().find(|l| l.trim_start().starts_with("Source")))
            .map(|l| l.trim().to_owned())
            .unwrap_or_else(|| "sin Wiimote1".to_owned());
        println!("Config: {} ({}) · Wiimote1: {} · existe: {}", d.config.display(), d.why, wm1, d.config.is_dir());
    }
    true
}

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
                // Dolphin.exe, DolphinQt.exe, Dolphin-x64.exe… según el build
                if name.starts_with("dolphin") && name.ends_with(".exe") {
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
        if !comm.trim().starts_with("dolphin-emu") {
            continue;
        }
        running = true;
        if dir.is_none() {
            if let Ok(exe) = std::fs::read_link(e.path().join("exe")) {
                dir = exe.parent().map(|p| p.to_path_buf());
            }
        }
    }
    (running, dir)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    (false, None)
}

/// INI partido en (preámbulo, secciones ordenadas). Conserva líneas tal cual.
struct Ini {
    preamble: Vec<String>,
    sections: Vec<(String, Vec<String>)>,
}

fn parse_ini(text: &str) -> Ini {
    let mut ini = Ini {
        preamble: Vec::new(),
        sections: Vec::new(),
    };
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            ini.sections
                .push((t[1..t.len() - 1].to_owned(), Vec::new()));
        } else if let Some((_, body)) = ini.sections.last_mut() {
            body.push(line.to_owned());
        } else {
            ini.preamble.push(line.to_owned());
        }
    }
    ini
}

fn serialize_ini(ini: &Ini) -> String {
    let mut out = String::new();
    for l in &ini.preamble {
        out.push_str(l);
        out.push('\n');
    }
    for (name, body) in &ini.sections {
        out.push('[');
        out.push_str(name);
        out.push_str("]\n");
        for l in body {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

fn set_section(ini: &mut Ini, name: &str, body: Vec<String>) {
    if let Some((_, b)) = ini.sections.iter_mut().find(|(n, _)| n == name) {
        *b = body;
    } else {
        ini.sections.push((name.to_owned(), body));
    }
}

/// Clave de una línea `Clave = valor` de INI (None para comentarios/vacías).
fn ini_key(line: &str) -> Option<&str> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
        return None;
    }
    Some(t.split('=').next()?.trim())
}

/// Escribe el INI solo si cambia algo. El backup `.pepomote.bak` se hace UNA
/// vez (el archivo tal como estaba antes de que PepoMote lo tocara nunca):
/// re-copiarlo en cada pasada lo habría sustituido por nuestra propia versión.
fn write_if_changed(path: &Path, original: &str, new: String) -> Result<(), String> {
    if original == new {
        return Ok(());
    }
    if path.exists() {
        let bak = path.with_extension("ini.pepomote.bak");
        if !bak.exists() {
            let _ = std::fs::copy(path, bak);
        }
    }
    std::fs::write(path, new).map_err(|e| e.to_string())
}

/// Dolphin.ini: [BluetoothPassthrough] Enabled = False, es decir, "Emular el
/// adaptador Bluetooth de la Wii" en el diálogo de mandos. Con "Acceder
/// directamente a un adaptador de Bluetooth" marcado, Dolphin busca un
/// adaptador USB real, deshabilita los Wiimotes emulados y el juego se cierra
/// nada más arrancar. El resto de la sección (VID, PID, LinkKeys) y del
/// archivo se conservan.
pub fn ensure_emulated_adapter(cfg_dir: &Path) -> Result<(), String> {
    let path = cfg_dir.join("Dolphin.ini");
    let original = std::fs::read_to_string(&path).unwrap_or_default();
    let mut ini = parse_ini(&original);
    let mut body: Vec<String> = ini
        .sections
        .iter()
        .find(|(n, _)| n == "BluetoothPassthrough")
        .map(|(_, b)| b.clone())
        .unwrap_or_default();
    let mut found = false;
    for l in body.iter_mut() {
        if ini_key(l) == Some("Enabled") {
            *l = "Enabled = False".to_owned();
            found = true;
        }
    }
    if !found {
        body.insert(0, "Enabled = False".to_owned());
    }
    set_section(&mut ini, "BluetoothPassthrough", body);
    write_if_changed(&path, &original, serialize_ini(&ini))
}

/// [Wiimote1..n] con nuestro mapeo (Source=1) y [Wiimote n+1..4] con
/// Source=0 (Ninguno): exactamente un mando por móvil. En los slots que se
/// apagan solo se toca la clave Source; el resto de sus líneas (un mapeo
/// manual, por ejemplo) se conserva. Las demás secciones, intactas.
pub fn write_wiimotes(cfg_dir: &Path, layout: &Layout) -> Result<(), String> {
    let path = cfg_dir.join("WiimoteNew.ini");
    let original = std::fs::read_to_string(&path).unwrap_or_default();
    let mut ini = parse_ini(&original);
    let n = layout.len().min(crate::net::MAX_PLAYERS);
    for (i, (wslot, nslot)) in layout.iter().take(n).enumerate() {
        let mut body: Vec<String> = MAPPING
            .replace("{DEV}", &wslot.to_string())
            .lines()
            .map(|l| l.to_owned())
            .collect();
        match nslot {
            Some(ns) => body.extend(NUNCHUK_MAPPING.replace("{NDEV}", &ns.to_string()).lines().map(|l| l.to_owned())),
            None => body.push("Extension = None".to_owned()),
        }
        set_section(&mut ini, &format!("Wiimote{}", i + 1), body);
    }
    for slot in n..crate::net::MAX_PLAYERS {
        let name = format!("Wiimote{}", slot + 1);
        let mut body: Vec<String> = ini
            .sections
            .iter()
            .find(|(s, _)| *s == name)
            .map(|(_, b)| b.clone())
            .unwrap_or_default();
        let mut found = false;
        for l in body.iter_mut() {
            if ini_key(l) == Some("Source") {
                *l = "Source = 0".to_owned();
                found = true;
            }
        }
        if !found {
            body.insert(0, "Source = 0".to_owned());
        }
        set_section(&mut ini, &name, body);
    }
    write_if_changed(&path, &original, serialize_ini(&ini))
}

/// DSUClient.ini: [Server] Enabled=True + nuestra entrada, preservando las
/// demás entradas y cualquier otra clave de la sección.
pub fn ensure_dsu_server(cfg_dir: &Path) -> Result<(), String> {
    let path = cfg_dir.join("DSUClient.ini");
    let original = std::fs::read_to_string(&path).unwrap_or_default();
    let mut ini = parse_ini(&original);

    let mut entries = String::new();
    let mut others: Vec<String> = Vec::new();
    if let Some((_, body)) = ini.sections.iter().find(|(n, _)| n == "Server") {
        for l in body {
            match ini_key(l) {
                Some("Entries") => {
                    entries = l.split_once('=').map(|(_, v)| v.trim()).unwrap_or("").to_owned();
                }
                Some("Enabled") => {}
                _ => others.push(l.clone()),
            }
        }
    }
    if !entries.contains(DSU_ENTRY) {
        if !entries.is_empty() && !entries.ends_with(';') {
            entries.push(';');
        }
        entries.push_str(DSU_ENTRY);
        entries.push(';');
    }
    let mut body = vec![
        "Enabled = True".to_owned(),
        format!("Entries = {entries}"),
    ];
    body.extend(others);
    set_section(&mut ini, "Server", body);
    write_if_changed(&path, &original, serialize_ini(&ini))
}

/// Perfiles manuales PepoMote-P1..P4 (fallback si alguien mapea a mano).
fn write_profiles(cfg_dir: &Path, n_players: usize) {
    let dir = cfg_dir.join("Profiles").join("Wiimote");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    for slot in 0..n_players.min(crate::net::MAX_PLAYERS) {
        let body: String = MAPPING
            .replace("{DEV}", &slot.to_string())
            .lines()
            .filter(|l| !l.starts_with("Source"))
            .fold(String::from("[Profile]\n"), |mut acc, l| {
                acc.push_str(l);
                acc.push('\n');
                acc
            });
        let name = if slot == 0 {
            "PepoMote.ini".to_owned()
        } else {
            format!("PepoMote-P{}.ini", slot + 1)
        };
        let _ = std::fs::write(dir.join(name), body);
    }
}

/// Carpetas tocadas, para el estado: «carpeta: portable (…)» o «en 2
/// carpetas: AppData\Dolphin Emulator, portable (…)».
fn describe_dirs(dirs: &[UserDir]) -> String {
    if dirs.len() == 1 {
        tr!("dolphin.dirs_one", dirs[0].why)
    } else {
        tr!("dolphin.dirs_many", dirs.len(), dirs.iter().map(|d| d.why).collect::<Vec<_>>().join(", "))
    }
}

/// Configura todos los Dolphin encontrados: adaptador Bluetooth emulado,
/// servidor DSU y un mando emulado por móvil conectado (Jugador N = móvil N
/// por orden de conexión); los slots restantes quedan en Ninguno.
pub fn configure(cfg_dolphin_dir: &str, layout: &Layout) -> Result<String, String> {
    let dirs = config_dirs_with(cfg_dolphin_dir);
    if dirs.is_empty() {
        return Err(tr!("dolphin.not_found").to_owned());
    }
    let n = layout.len().clamp(1, crate::net::MAX_PLAYERS);
    let nunchuks = layout.iter().filter(|(_, n)| n.is_some()).count();
    for dir in &dirs {
        ensure_emulated_adapter(&dir.config)?;
        ensure_dsu_server(&dir.config)?;
        write_wiimotes(&dir.config, layout)?;
        write_profiles(&dir.config, crate::net::MAX_PLAYERS);
    }
    Ok(tr!(
        "dolphin.details",
        n,
        if nunchuks > 0 { tr!("dolphin.and_nunchuks", nunchuks) } else { String::new() },
        describe_dirs(&dirs)
    ))
}

/// Al ver a Dolphin abierto se aprende su carpeta (para configurarlo cerrado
/// aunque sea portable o viva en un sitio raro).
pub(crate) fn learn_dir(shared: &SharedState, dir: Option<PathBuf>) {
    let Some(dir) = dir else { return };
    let dir_s = dir.to_string_lossy().to_string();
    let mut s = shared.lock().unwrap();
    if s.config.dolphin_dir != dir_s {
        s.config.dolphin_dir = dir_s;
        s.config.save();
    }
}

/// Escribe (o deja pendiente si Dolphin está abierto). `after_close`: viene
/// del vigilante, Dolphin se acaba de cerrar.
fn run_configure(shared: &SharedState, layout: &Layout, after_close: bool) {
    let _serial = CONFIGURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Solo para los e2e en el propio equipo: escribir aunque el emulador esté abierto
    let assume_closed = std::env::var_os("PEPOMOTE_ASSUME_EMULATOR_CLOSED").is_some();
    let (running, exe_dir) = if assume_closed { (false, None) } else { running_exe() };
    learn_dir(shared, exe_dir);
    let (ok, msg) = if running {
        // Dolphin sobreescribe su configuración al salir: se escribe en
        // cuanto se cierre (vigilante), sin que nadie tenga que pulsar nada
        shared.lock().unwrap().dolphin_pending = true;
        let text = if layout.iter().any(|(_, n)| n.is_some()) { tr!("dolphin.open_nunchuk") } else { tr!("dolphin.open") };
        (false, text.to_owned())
    } else {
        shared.lock().unwrap().dolphin_pending = false;
        let cfg_dir = shared.lock().unwrap().config.dolphin_dir.clone();
        match configure(&cfg_dir, layout) {
            Ok(details) => {
                let prefix = if after_close { tr!("dolphin.configured_after_close") } else { tr!("dolphin.configured") };
                (true, format!("{prefix} {details}"))
            }
            Err(e) => (false, tr!("dolphin.error", e)),
        }
    };
    shared.lock().unwrap().dolphin_cfg_status = Some(CfgStatus { ok, text: msg.clone() });
    // Los móviles lo ven también (banner): hasta ahora un Nunchuk que
    // entraba con Dolphin abierto fallaba en silencio
    crate::net::notify_all(&msg);
}

/// Disparo automático (conexión/desconexión/cambio a modo Dolphin).
pub fn maybe_auto_configure(shared: &SharedState) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let (auto, mode, layout) = {
            let s = shared.lock().unwrap();
            (s.config.auto_dolphin, s.mode, crate::state::player_layout(&s.players))
        };
        // al menos un mando: un Nunchuk solo no tiene a quién acompañar
        if auto && mode == Mode::Dolphin && !layout.is_empty() {
            run_configure(&shared, &layout, false);
        }
    });
}

/// Botón manual de la ventana.
pub fn configure_now(shared: &SharedState) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let mut layout = crate::state::player_layout(&shared.lock().unwrap().players);
        if layout.is_empty() {
            layout.push((0, None));
        }
        run_configure(&shared, &layout, false);
    });
}

/// Botón «Detectar» de Ajustes: busca Dolphin y guarda su carpeta.
pub fn detect_now(shared: &SharedState) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let (_, dir) = running_exe();
        let found = dir.into_iter().chain(find_exe_dirs()).next();
        let mut s = shared.lock().unwrap();
        match found {
            Some(d) => {
                s.config.dolphin_dir = d.to_string_lossy().to_string();
                s.config.save();
                let dirs = config_dirs_with(&s.config.dolphin_dir);
                s.dolphin_cfg_status = Some(CfgStatus::ok(tr!("dolphin.found", d.display(), describe_dirs(&dirs))));
            }
            None => {
                s.dolphin_cfg_status = Some(CfgStatus::warn(tr!("dolphin.not_found_manual").to_owned()));
            }
        }
    });
}

/// Lo pendiente (Dolphin estaba abierto) se aplica cuando ya está cerrado:
/// lo llama el vigilante de `auto_mode` al ver a Dolphin cerrado. Así nadie
/// tiene que acordarse de pulsar «Configurar».
pub fn apply_pending(shared: &SharedState) {
    let (auto, mode, layout) = {
        let s = shared.lock().unwrap();
        (s.config.auto_dolphin, s.mode, crate::state::player_layout(&s.players))
    };
    if auto && mode == Mode::Dolphin && !layout.is_empty() {
        run_configure(shared, &layout, true);
    } else {
        shared.lock().unwrap().dolphin_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// n mandos sin Nunchuk en los slots 0..n.
    fn wm(n: usize) -> Vec<(u8, Option<u8>)> {
        (0..n as u8).map(|s| (s, None)).collect()
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pepomote-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn env_empty() -> Env {
        Env {
            exe_dirs: vec![],
            reg_local_user_config: false,
            reg_user_config_path: None,
            documents_legacy: None,
            appdata: None,
            userpath_env: None,
            legacy_home: None,
            xdg_config: None,
            flatpak: None,
        }
    }

    #[test]
    fn carpeta_de_usuario_como_la_elige_dolphin() {
        let base = tmp_dir("dirs");
        let exe = base.join("DolphinPortable");
        std::fs::create_dir_all(&exe).unwrap();
        // Windows moderno sin nada especial: AppData (aunque no exista aún)
        let mut env = env_empty();
        env.appdata = Some(base.join("AppData").join("Dolphin Emulator"));
        let dirs = resolve_user_dirs(&env);
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].config, base.join("AppData").join("Dolphin Emulator").join("Config"));
        // instalación antigua: Documentos manda sobre AppData
        env.documents_legacy = Some(base.join("Documents").join("Dolphin Emulator"));
        let dirs = resolve_user_dirs(&env);
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].config.starts_with(base.join("Documents")), "{dirs:?}");
        // portable.txt junto al exe: esa es la que usa ESE Dolphin, y va primero
        std::fs::write(exe.join("portable.txt"), "").unwrap();
        env.exe_dirs = vec![exe.clone()];
        let dirs = resolve_user_dirs(&env);
        assert_eq!(dirs[0].config, exe.join("User").join("Config"));
        assert!(dirs[0].why.starts_with("portable"));
        assert_eq!(dirs.len(), 2, "y además la de Documentos: {dirs:?}");
        // registro: UserConfigPath a una carpeta cualquiera
        std::fs::remove_file(exe.join("portable.txt")).unwrap();
        env.reg_user_config_path = Some(base.join("MiDolphin"));
        let dirs = resolve_user_dirs(&env);
        assert_eq!(dirs[0].config, base.join("MiDolphin").join("Config"));
        // registro: LocalUserConfig → junto al exe
        env.reg_user_config_path = None;
        env.reg_local_user_config = true;
        let dirs = resolve_user_dirs(&env);
        assert_eq!(dirs[0].config, exe.join("User").join("Config"));
        // Linux: ~/.dolphin-emu antiguo manda sobre XDG; Flatpak aparte; la variable de entorno primero
        let mut lx = env_empty();
        lx.xdg_config = Some(base.join(".config").join("dolphin-emu"));
        assert_eq!(resolve_user_dirs(&lx)[0].config, base.join(".config").join("dolphin-emu"));
        lx.legacy_home = Some(base.join(".dolphin-emu"));
        assert_eq!(resolve_user_dirs(&lx)[0].config, base.join(".dolphin-emu").join("Config"));
        lx.flatpak = Some(base.join("flatpak-cfg"));
        lx.userpath_env = Some(base.join("userpath"));
        let dirs = resolve_user_dirs(&lx);
        assert_eq!(dirs[0].config, base.join("userpath").join("Config"));
        assert!(dirs.iter().any(|d| d.why == "Flatpak"));
        assert_eq!(dirs.len(), 3, "{dirs:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn wiimotes_escribe_slots_y_preserva_lo_ajeno() {
        let dir = tmp_dir("wiimote");
        std::fs::write(
            dir.join("WiimoteNew.ini"),
            "[Wiimote1]\nDevice = DInput/0/Keyboard Mouse\nSource = 0\n[Wiimote3]\nDevice = Real/Cosa\nSource = 2\n[BalanceBoard]\nSource = 0\n",
        )
        .unwrap();

        write_wiimotes(&dir, &wm(2)).unwrap();
        let out = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();

        assert!(out.contains("[Wiimote1]"));
        assert!(out.contains("Device = DSUClient/0/PepoMote"));
        assert!(out.contains("[Wiimote2]"));
        assert!(out.contains("Device = DSUClient/1/PepoMote"));
        // slots sin móvil: Source = 0 (ningún mando de más en pantalla),
        // conservando las demás líneas; lo ajeno, intacto
        assert!(out.contains("[Wiimote3]
Device = Real/Cosa
Source = 0
"), "{out}");
        assert!(out.contains("[Wiimote4]
Source = 0
"), "{out}");
        assert!(out.contains("[BalanceBoard]
Source = 0
"));
        // backup creado
        assert!(dir.join("WiimoteNew.ini.pepomote.bak").exists());

        // idempotente
        write_wiimotes(&dir, &wm(2)).unwrap();
        let out2 = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();
        assert_eq!(out, out2);
    }

    #[test]
    fn backup_es_el_original_y_no_se_pisa() {
        let dir = tmp_dir("bak");
        let original = "[Wiimote1]\nDevice = Bluetooth/0/Wii Remote\nSource = 2\n";
        std::fs::write(dir.join("WiimoteNew.ini"), original).unwrap();
        let bak = dir.join("WiimoteNew.ini.pepomote.bak");

        write_wiimotes(&dir, &wm(1)).unwrap();
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), original);

        // segunda pasada con más jugadores: el backup sigue siendo el ORIGINAL
        write_wiimotes(&dir, &wm(3)).unwrap();
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), original);
        let out = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();
        assert!(out.contains("Device = DSUClient/2/PepoMote"));
    }

    #[test]
    fn sin_cambios_no_se_reescribe() {
        let dir = tmp_dir("nochange");
        write_wiimotes(&dir, &wm(2)).unwrap();
        let path = dir.join("WiimoteNew.ini");
        let m1 = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        write_wiimotes(&dir, &wm(2)).unwrap();
        let m2 = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(m1, m2, "un INI idéntico no debe tocar el disco");
    }

    #[test]
    fn un_mando_por_movil_y_los_demas_apagados() {
        let dir = tmp_dir("uno-por-movil");
        // dos móviles: Wiimote1 y 2 emulados, 3 y 4 en Ninguno
        write_wiimotes(&dir, &wm(2)).unwrap();
        let out = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();
        assert!(out.contains("[Wiimote1]
Device = DSUClient/0/PepoMote
Source = 1
"));
        assert!(out.contains("[Wiimote2]
Device = DSUClient/1/PepoMote
Source = 1
"));
        assert!(out.contains("[Wiimote3]
Source = 0
"), "{out}");
        assert!(out.contains("[Wiimote4]
Source = 0
"), "{out}");
        // se va el segundo móvil: su mando se apaga (el mapeo queda para la próxima)
        write_wiimotes(&dir, &wm(1)).unwrap();
        let out = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();
        assert!(out.contains("[Wiimote1]
Device = DSUClient/0/PepoMote
Source = 1
"));
        assert!(out.contains("[Wiimote2]
Device = DSUClient/1/PepoMote
Source = 0
"), "{out}");
        assert_eq!(out.matches("Source = 1").count(), 1);
        // vuelve: se reactiva
        write_wiimotes(&dir, &wm(2)).unwrap();
        let out = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();
        assert_eq!(out.matches("Source = 1").count(), 2);
    }

    #[test]
    fn nunchuk_del_jugador_lee_del_pad_del_otro_movil() {
        let dir = tmp_dir("nunchuk");
        // J1 = mando en slot 0 con Nunchuk en slot 3
        write_wiimotes(&dir, &[(0, Some(3))]).unwrap();
        let out = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();
        assert!(out.contains("[Wiimote1]
Device = DSUClient/0/PepoMote
Source = 1
"), "{out}");
        assert!(out.contains("Extension = Nunchuk
"), "{out}");
        assert!(out.contains("Nunchuk/Buttons/C = `DSUClient/3/PepoMote:Cross`
"));
        assert!(out.contains("Nunchuk/Stick/Up = `DSUClient/3/PepoMote:Left Y+`
"));
        assert!(out.contains("Nunchuk/IMUAccelerometer/Forward = `DSUClient/3/PepoMote:Accel Forward`
"));
        assert!(!out.contains("Extension = None"));
        assert!(out.contains("[Wiimote2]
Source = 0
") && out.contains("[Wiimote4]
Source = 0
"), "{out}");
        // se va el Nunchuk: el mando vuelve a Extension = None sin restos
        write_wiimotes(&dir, &[(0, None)]).unwrap();
        let out = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();
        assert!(out.contains("Extension = None
"));
        assert!(!out.contains("Nunchuk/"), "{out}");
        // dos jugadores con Nunchuk cada uno: slots 0/3 y 1/2
        write_wiimotes(&dir, &[(0, Some(3)), (1, Some(2))]).unwrap();
        let out = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();
        assert!(out.contains("[Wiimote2]
Device = DSUClient/1/PepoMote
Source = 1
"), "{out}");
        assert!(out.contains("Nunchuk/Buttons/Z = `DSUClient/2/PepoMote:Circle`
"));
        assert_eq!(out.matches("Extension = Nunchuk").count(), 2);
    }

    #[test]
    fn adaptador_emulado_se_fuerza_y_lo_demas_se_conserva() {
        let dir = tmp_dir("bt");
        std::fs::write(
            dir.join("Dolphin.ini"),
            "[Core]
WiimoteContinuousScanning = False
[BluetoothPassthrough]
Enabled = True
VID = -1
PID = -1
LinkKeys = 
[Interface]
Language = 0
",
        )
        .unwrap();
        ensure_emulated_adapter(&dir).unwrap();
        let out = std::fs::read_to_string(dir.join("Dolphin.ini")).unwrap();
        assert!(out.contains("[BluetoothPassthrough]
Enabled = False
VID = -1
PID = -1
"), "{out}");
        assert!(!out.contains("Enabled = True"));
        assert!(out.contains("[Core]
WiimoteContinuousScanning = False
"));
        assert!(out.contains("[Interface]
Language = 0
"));
        assert!(dir.join("Dolphin.ini.pepomote.bak").exists());
        // idempotente
        ensure_emulated_adapter(&dir).unwrap();
        assert_eq!(out, std::fs::read_to_string(dir.join("Dolphin.ini")).unwrap());
    }

    #[test]
    fn adaptador_emulado_sin_seccion_ni_archivo() {
        let dir = tmp_dir("bt-nuevo");
        ensure_emulated_adapter(&dir).unwrap();
        let out = std::fs::read_to_string(dir.join("Dolphin.ini")).unwrap();
        assert_eq!(out, "[BluetoothPassthrough]
Enabled = False
");
        // sección presente pero sin la clave: se añade sin tocar el resto
        std::fs::write(dir.join("Dolphin.ini"), "[BluetoothPassthrough]
VID = 1234
").unwrap();
        ensure_emulated_adapter(&dir).unwrap();
        let out = std::fs::read_to_string(dir.join("Dolphin.ini")).unwrap();
        assert_eq!(out, "[BluetoothPassthrough]
Enabled = False
VID = 1234
");
    }

    #[test]
    fn dsu_server_agrega_sin_duplicar_ni_borrar() {
        let dir = tmp_dir("dsu");
        std::fs::write(
            dir.join("DSUClient.ini"),
            "[Server]\nEnabled = False\nIPAddress = 127.0.0.1\nPort = 26760\nEntries = Otro:1.2.3.4:26760;\n",
        )
        .unwrap();

        ensure_dsu_server(&dir).unwrap();
        let out = std::fs::read_to_string(dir.join("DSUClient.ini")).unwrap();
        assert!(out.contains("Enabled = True"));
        assert!(!out.contains("Enabled = False"));
        assert!(out.contains("Otro:1.2.3.4:26760"));
        assert!(out.contains("PepoMote:127.0.0.1:26760"));
        // las claves ajenas de [Server] sobreviven
        assert!(out.contains("IPAddress = 127.0.0.1"));
        assert!(out.contains("Port = 26760"));

        ensure_dsu_server(&dir).unwrap();
        let out2 = std::fs::read_to_string(dir.join("DSUClient.ini")).unwrap();
        assert_eq!(out2.matches("PepoMote:127.0.0.1:26760").count(), 1);
        assert_eq!(out2.matches("Entries").count(), 1);
        assert_eq!(out, out2);
    }
}
