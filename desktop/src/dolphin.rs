//! Auto-configuración de Dolphin para multijugador: escribe las secciones
//! [Wiimote1..N] de WiimoteNew.ini (Source=1 + mapeo DSU validado en h3) y
//! registra el servidor en DSUClient.ini. Nunca escribe con Dolphin abierto
//! (su config se sobreescribe al salir) y siempre deja backup .pepomote.bak.

use crate::state::{Mode, SharedState};
use std::path::{Path, PathBuf};

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
Extension = None
Options/Battery = `Battery`
";

const DSU_ENTRY: &str = "PepoMote:127.0.0.1:26760";

/// Directorios de config de Dolphin presentes en esta máquina.
pub fn config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut push = |p: PathBuf| {
        if p.exists() {
            dirs.push(p);
        }
    };
    if let Some(base) = directories::BaseDirs::new() {
        #[cfg(windows)]
        {
            push(base.config_dir().join("Dolphin Emulator").join("Config"));
            if let Some(doc) = directories::UserDirs::new().and_then(|u| u.document_dir().map(|d| d.to_path_buf())) {
                push(doc.join("Dolphin Emulator").join("Config"));
            }
        }
        #[cfg(target_os = "linux")]
        {
            push(base.config_dir().join("dolphin-emu"));
            push(
                base.home_dir()
                    .join(".var/app/org.DolphinEmu.dolphin-emu/config/dolphin-emu"),
            );
        }
    }
    dirs
}

#[cfg(windows)]
pub fn dolphin_running() -> bool {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
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
                if name == "dolphin.exe" {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
        found
    }
}

#[cfg(target_os = "linux")]
pub fn dolphin_running() -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    for e in entries.flatten() {
        if let Ok(comm) = std::fs::read_to_string(e.path().join("comm")) {
            if comm.trim().starts_with("dolphin-emu") {
                return true;
            }
        }
    }
    false
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn dolphin_running() -> bool {
    false
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

/// [Wiimote1..n] con nuestro mapeo; el resto de secciones, intactas.
pub fn write_wiimotes(cfg_dir: &Path, n_players: usize) -> Result<(), String> {
    let path = cfg_dir.join("WiimoteNew.ini");
    let original = std::fs::read_to_string(&path).unwrap_or_default();
    if path.exists() {
        let _ = std::fs::copy(&path, cfg_dir.join("WiimoteNew.ini.pepomote.bak"));
    }
    let mut ini = parse_ini(&original);
    for slot in 0..n_players.min(crate::net::MAX_PLAYERS) {
        let body: Vec<String> = MAPPING
            .replace("{DEV}", &slot.to_string())
            .lines()
            .map(|l| l.to_owned())
            .collect();
        set_section(&mut ini, &format!("Wiimote{}", slot + 1), body);
    }
    std::fs::write(&path, serialize_ini(&ini)).map_err(|e| e.to_string())
}

/// DSUClient.ini: [Server] Enabled=True + nuestra entrada (preservando otras).
pub fn ensure_dsu_server(cfg_dir: &Path) -> Result<(), String> {
    let path = cfg_dir.join("DSUClient.ini");
    let original = std::fs::read_to_string(&path).unwrap_or_default();
    let mut ini = parse_ini(&original);

    let mut entries = String::new();
    if let Some((_, body)) = ini.sections.iter().find(|(n, _)| n == "Server") {
        for l in body {
            if let Some(v) = l.trim().strip_prefix("Entries") {
                entries = v.trim_start_matches(['=', ' ']).trim().to_owned();
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
    set_section(
        &mut ini,
        "Server",
        vec![
            "Enabled = True".to_owned(),
            format!("Entries = {entries}"),
        ],
    );
    std::fs::write(&path, serialize_ini(&ini)).map_err(|e| e.to_string())
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

/// Configura todos los Dolphin encontrados para n_players mandos.
pub fn configure(n_players: usize) -> Result<String, String> {
    let dirs = config_dirs();
    if dirs.is_empty() {
        return Err("No encuentro la configuración de Dolphin en este equipo".into());
    }
    for dir in &dirs {
        ensure_dsu_server(dir)?;
        write_wiimotes(dir, n_players)?;
        write_profiles(dir, n_players);
    }
    Ok(format!(
        "Dolphin configurado para {} mando(s){}",
        n_players,
        if dirs.len() > 1 {
            format!(" en {} instalaciones", dirs.len())
        } else {
            String::new()
        }
    ))
}

fn run_configure(shared: &SharedState, n: usize) {
    let msg = if dolphin_running() {
        "Dolphin está abierto: ciérralo y pulsa Configurar".to_owned()
    } else {
        match configure(n) {
            Ok(m) => m,
            Err(e) => format!("Dolphin: {e}"),
        }
    };
    shared.lock().unwrap().dolphin_cfg_status = Some(msg);
}

/// Disparo automático (conexión/desconexión/cambio a modo Dolphin).
pub fn maybe_auto_configure(shared: &SharedState) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let (auto, mode, n) = {
            let s = shared.lock().unwrap();
            (s.config.auto_dolphin, s.mode, s.player_count())
        };
        if auto && mode == Mode::Dolphin && n >= 1 {
            run_configure(&shared, n);
        }
    });
}

/// Botón manual de la ventana.
pub fn configure_now(shared: &SharedState) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let n = shared.lock().unwrap().player_count().max(1);
        run_configure(&shared, n);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pepomote-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn wiimotes_escribe_slots_y_preserva_lo_ajeno() {
        let dir = tmp_dir("wiimote");
        std::fs::write(
            dir.join("WiimoteNew.ini"),
            "[Wiimote1]\nDevice = DInput/0/Keyboard Mouse\nSource = 0\n[Wiimote3]\nDevice = Real/Cosa\nSource = 2\n[BalanceBoard]\nSource = 0\n",
        )
        .unwrap();

        write_wiimotes(&dir, 2).unwrap();
        let out = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();

        assert!(out.contains("[Wiimote1]"));
        assert!(out.contains("Device = DSUClient/0/PepoMote"));
        assert!(out.contains("[Wiimote2]"));
        assert!(out.contains("Device = DSUClient/1/PepoMote"));
        // lo ajeno, intacto
        assert!(out.contains("[Wiimote3]"));
        assert!(out.contains("Device = Real/Cosa"));
        assert!(out.contains("[BalanceBoard]"));
        // backup creado
        assert!(dir.join("WiimoteNew.ini.pepomote.bak").exists());

        // idempotente
        write_wiimotes(&dir, 2).unwrap();
        let out2 = std::fs::read_to_string(dir.join("WiimoteNew.ini")).unwrap();
        assert_eq!(out, out2);
    }

    #[test]
    fn dsu_server_agrega_sin_duplicar_ni_borrar() {
        let dir = tmp_dir("dsu");
        std::fs::write(
            dir.join("DSUClient.ini"),
            "[Server]\nEnabled = False\nEntries = Otro:1.2.3.4:26760;\n",
        )
        .unwrap();

        ensure_dsu_server(&dir).unwrap();
        let out = std::fs::read_to_string(dir.join("DSUClient.ini")).unwrap();
        assert!(out.contains("Enabled = True"));
        assert!(out.contains("Otro:1.2.3.4:26760"));
        assert!(out.contains("PepoMote:127.0.0.1:26760"));

        ensure_dsu_server(&dir).unwrap();
        let out2 = std::fs::read_to_string(dir.join("DSUClient.ini")).unwrap();
        assert_eq!(out2.matches("PepoMote:127.0.0.1:26760").count(), 1);
    }
}
