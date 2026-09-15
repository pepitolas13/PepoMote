//! Configuración de Eden para los mandos Switch de PepoMote.
mod keyboard;
mod mapping;
mod paths;
use crate::state::{switch_layout, CfgStatus, Config, LockTolerant, Mode, SharedState};
use crate::state::{SwitchPad, SwitchPlayer};
use crate::{ini, tr};
pub use keyboard::focus_keyboard;
use mapping::*;
pub use paths::running_exe;
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
    path.with_extension("ini.pepomote.bak")
}

fn write_config(path: &Path, layout: &[SwitchPlayer], port: u16) -> Result<(), String> {
    let original = read_config(path)?;
    let guid = dsu_guid(DSU_IP);
    if server_list(controls(&ini::parse_ini(&original)), port).len() > 8 {
        return Err(tr!("eden.servers_full").to_owned());
    }
    let new = apply_layout(&original, layout, &guid, port);
    if new == original {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let bak = backup_path(path);
    let current = ini::parse_ini(&original);
    let any_ours = (0..MAX_PLAYERS).any(|i| player_is_ours(controls(&current), i, &guid, port));
    if !bak.exists() || !any_ours {
        // Una sesión nueva tras restaurar o reconfigurar manualmente Eden.
        ini::atomic_write(&bak, &original)?;
    } else {
        // Un jugador nuevo puede haber cambiado su mando desde el primer
        // respaldo. Guardar sus claves justo antes de tomar el control.
        let saved_text = read_config(&bak)?;
        let mut saved = ini::parse_ini(&saved_text);
        let mut body = controls(&saved).to_vec();
        for pl in layout {
            if !player_is_ours(controls(&current), pl.index, &guid, port) {
                copy_keys(&mut body, controls(&current), &managed_keys(pl.index));
            }
        }
        ini::set_section(&mut saved, "Controls", body);
        let updated = ini::serialize_ini(&saved);
        if updated != saved_text {
            ini::atomic_write(&bak, &updated)?;
        }
    }
    ini::atomic_write(path, &new)
}

fn restore_config(path: &Path, port: u16) -> Result<bool, String> {
    let bak = backup_path(path);
    if !bak.is_file() {
        return Ok(false);
    }
    let current = read_config(path)?;
    let backup = read_config(&bak)?;
    let new = restore_keys(&current, &backup, &dsu_guid(DSU_IP), port);
    if current == new {
        return Ok(false);
    }
    ini::atomic_write(path, &new)?;
    Ok(true)
}

fn describe(layout: &[SwitchPlayer]) -> String {
    layout
        .iter()
        .map(|p| {
            let kind = tr!("eden.kind_pro");
            tr!("eden.player_kind", p.index + 1, kind)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn configure(cfg: &Config, layout: &[SwitchPlayer]) -> Result<String, String> {
    let files = paths::config_files_with(cfg);
    if files.is_empty() {
        return Err(tr!("eden.not_found").to_owned());
    }
    for f in &files {
        write_config(&f.path, layout, crate::dsu::port())?;
    }
    let where_ = files
        .iter()
        .map(|f| format!("{} ({})", f.path.display(), f.why))
        .collect::<Vec<_>>()
        .join(" · ");
    Ok(format!("{} · {where_}", describe(layout)))
}

fn has_selected_config(path: &Path) -> bool {
    (path.file_name().is_some_and(|n| n == "qt-config.ini") && path.is_file())
        || path.join("qt-config.ini").is_file()
}

pub(crate) fn learn_dir(shared: &SharedState, dir: Option<PathBuf>) {
    let Some(dir) = dir else { return };
    let mut s = shared.lock_tolerant();
    // Respetar la carpeta elegida explícitamente (puede ser otro perfil).
    if s.config.eden_dir.trim().is_empty()
        || (cfg!(target_os = "linux")
            && dir.ends_with("user/config")
            && !has_selected_config(Path::new(&s.config.eden_dir)))
    {
        s.config.eden_dir = dir.to_string_lossy().into_owned();
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
    crate::log_line!("Eden: {text}");
    shared.lock_tolerant().eden_cfg_status = Some(CfgStatus { ok, text });
    crate::net::notify_all(phone);
}

fn run_configure(shared: &SharedState, after_close: bool, manual: bool) {
    let _serial = CONFIGURE_LOCK.lock_tolerant();
    if manual {
        let mut s = shared.lock_tolerant();
        s.eden_restore_pending = false;
        if s.config.eden_restore_pending {
            s.config.eden_restore_pending = false;
            s.config.save();
        }
    }
    // Tomar el reparto DESPUÉS del candado: conexiones rápidas no deben
    // dejar escrito el reparto antiguo de un hilo que llegó tarde.
    let (cfg, mode, mut layout, restoring) = {
        let s = shared.lock_tolerant();
        (
            s.config.clone(),
            s.mode,
            switch_layout(&s.players),
            s.eden_restore_pending,
        )
    };
    if !manual && (!cfg.auto_eden || mode != Mode::Switch || layout.is_empty() || restoring) {
        if after_close {
            shared.lock_tolerant().eden_pending = false;
        }
        return;
    }
    if layout.is_empty() {
        layout.push(SwitchPlayer {
            index: 0,
            kind: SwitchPad::Pro,
            dsu_slot: 0,
        });
    }
    let (running, dir) = observed();
    learn_dir(shared, dir);
    if running {
        {
            let mut s = shared.lock_tolerant();
            s.eden_pending = true;
            s.eden_manual_pending |= manual;
        }
        status(
            shared,
            false,
            tr!("eden.open").to_owned(),
            tr!("eden.phone_open"),
        );
        return;
    }
    {
        let mut s = shared.lock_tolerant();
        s.eden_pending = false;
        s.eden_manual_pending = false;
    }
    match configure(&cfg, &layout) {
        Ok(details) => {
            let prefix = if after_close {
                tr!("eden.configured_after_close")
            } else {
                tr!("eden.configured")
            };
            status(
                shared,
                true,
                format!("{prefix} {details}"),
                tr!("eden.phone_configured"),
            );
        }
        Err(e) => {
            let text = tr!("eden.error", e);
            status(shared, false, text.clone(), &text);
        }
    }
}

pub fn maybe_auto_configure(shared: &SharedState) {
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("eden-configure", move || {
        run_configure(&shared, false, false)
    });
}

pub fn configure_now(shared: &SharedState) {
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("eden-configure", move || {
        run_configure(&shared, false, true)
    });
}

pub fn apply_pending(shared: &SharedState) {
    let (restore, manual) = {
        let s = shared.lock_tolerant();
        (s.eden_restore_pending, s.eden_manual_pending)
    };
    if restore {
        run_restore(shared);
    } else {
        run_configure(shared, true, manual);
    }
}

fn run_restore(shared: &SharedState) {
    let _serial = CONFIGURE_LOCK.lock_tolerant();
    if !shared.lock_tolerant().eden_restore_pending {
        return;
    }
    if observed().0 {
        {
            let mut s = shared.lock_tolerant();
            s.eden_pending = true;
            s.eden_restore_pending = true;
            s.eden_manual_pending = false;
        }
        status(
            shared,
            false,
            tr!("eden.restore_open").to_owned(),
            tr!("eden.restore_open"),
        );
        return;
    }
    {
        let mut s = shared.lock_tolerant();
        s.eden_pending = false;
        s.eden_restore_pending = false;
    }
    let cfg = shared.lock_tolerant().config.clone();
    let result = (|| {
        let files = paths::config_files_with(&cfg);
        if files.is_empty() {
            return Err(tr!("eden.not_found").to_owned());
        }
        let mut restored = false;
        for f in files {
            restored |= restore_config(&f.path, crate::dsu::port())?;
        }
        Ok(restored)
    })();
    match result {
        Ok(restored) => {
            {
                let mut s = shared.lock_tolerant();
                s.config.eden_restore_pending = false;
                s.config.save();
            }
            let text = if restored {
                tr!("eden.restored")
            } else {
                tr!("eden.nothing_restore")
            };
            status(shared, true, text.to_owned(), text);
        }
        Err(e) => {
            let text = tr!("eden.error", e);
            status(shared, false, text.clone(), &text);
        }
    }
}

pub fn restore_now(shared: &SharedState) {
    // La restauración debe permanecer: reconectar un móvil no puede volver
    // a tomar los mandos que el usuario acaba de recuperar.
    {
        let mut s = shared.lock_tolerant();
        s.config.auto_eden = false;
        s.config.eden_restore_pending = true;
        s.config.save();
        s.eden_restore_pending = true;
    }
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("eden-restore", move || run_restore(&shared));
}

pub fn detect_now(shared: &SharedState) {
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("eden-detect", move || {
        let (_, dir) = running_exe();
        let found = dir.into_iter().chain(paths::find_exe_dirs()).next();
        let mut s = shared.lock_tolerant();
        if let Some(d) = found {
            s.config.eden_dir = d.to_string_lossy().into_owned();
            s.config.save();
            s.eden_cfg_status = Some(CfgStatus::ok(tr!("eden.found", d.display())));
        } else {
            let files = paths::config_files_with(&s.config);
            if let Some(f) = files.first() {
                s.eden_cfg_status = Some(CfgStatus::ok(tr!("eden.found", f.path.display())));
            } else {
                s.eden_cfg_status = Some(CfgStatus::warn(tr!("eden.not_found").to_owned()));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    const GUID: &str = "0000000000000000000000007f000001";
    fn pro(index: u8, slot: u8) -> SwitchPlayer {
        SwitchPlayer {
            index,
            kind: SwitchPad::Pro,
            dsu_slot: slot,
        }
    }
    #[test]
    fn pro_layout_writes_effective_qt_keys_and_preserves_other_sections() {
        let src="[Audio]\r\nvolume=73\r\n\r\n[Controls]\r\nplayer_0_button_a=old\r\nplayer_0_button_a\\default=true\r\nother=keep\r\n[Renderer]\r\nbackend=1\r\n";
        let got = apply_layout(src, &[pro(0, 0)], GUID, 26760);
        assert!(got.contains("player_0_button_a\\default=false\r\n"));
        assert!(got.contains("player_0_button_a=\"button:8192,engine:cemuhookudp,guid:0000000000000000000000007f000001,pad:0,port:26760\"\r\n"));
        assert!(got.starts_with("[Audio]\r\nvolume=73\r\n\r\n"));
        assert!(got.ends_with("[Renderer]\r\nbackend=1\r\n"));
        assert!(got.contains("other=keep\r\n"));
        assert_eq!(apply_layout(&got, &[pro(0, 0)], GUID, 26760), got);
    }
    fn val(text: &str, key: &str) -> String {
        ini::qt_effective(controls(&ini::parse_ini(text)), key, "missing").to_owned()
    }
    fn temp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "pepomote-eden-{tag}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn guid_is_ipv4_in_low_32_bits() {
        assert_eq!(dsu_guid(DSU_IP), GUID);
        assert_eq!(
            dsu_guid(std::net::Ipv4Addr::new(192, 168, 1, 7)),
            "000000000000000000000000c0a80107"
        );
    }
    #[test]
    fn pro_players_use_their_own_slot_for_every_control() {
        let text = apply_layout("", &[pro(0, 2), pro(1, 0)], GUID, 26760);
        for (player, slot) in [(0, 2), (1, 0)] {
            assert_eq!(val(&text, &format!("player_{player}_type")), "0");
            for key in ["button_a", "button_b", "button_x", "button_y", "button_dleft",
                "button_l", "button_r", "button_zl", "button_zr", "button_plus",
                "button_minus", "button_home", "button_screenshot", "button_lstick",
                "button_rstick", "lstick", "rstick", "motionleft", "motionright"] {
                let value = val(&text, &format!("player_{player}_{key}"));
                assert!(value.contains(&format!("pad:{slot},")), "{key}: {value}");
            }
            assert!(val(&text, &format!("player_{player}_lstick")).contains("axis_x:0,axis_y:1"));
            assert!(val(&text, &format!("player_{player}_rstick")).contains("axis_x:2,axis_y:3"));
        }
    }

    #[test]
    fn pro_setup_replaces_old_joycon_type_and_clears_side_buttons() {
        for old_type in [1, 2, 3] {
            let original = format!("[Controls]\nplayer_0_type={old_type}\nplayer_0_type\\default=false\nplayer_0_button_slleft=old\nplayer_0_button_srright=old\n");
            let text = apply_layout(&original, &[pro(0, 1)], GUID, 26760);
            assert_eq!(val(&text, "player_0_type"), "0");
            for name in ["slleft", "srleft", "slright", "srright"] {
                assert_eq!(val(&text, &format!("player_0_button_{name}")), "[empty]");
            }
            assert!(val(&text, "player_0_lstick").contains("axis_x:0,axis_y:1"));
            assert!(val(&text, "player_0_rstick").contains("axis_x:2,axis_y:3"));
            assert!(!text.contains("invert_x:-") && !text.contains("invert_y:-"));
        }
    }

    #[test]
    fn foreign_udp_servers_and_players_are_preserved_and_departed_player_disabled() {
        let original="[Controls]\nudp_input_servers\\default=false\nudp_input_servers=10.0.0.4:25000\nplayer_7_type\\default=false\nplayer_7_type=1\nplayer_7_button_a=keyboard\n";
        let both = apply_layout(original, &[pro(0, 2), pro(1, 0)], GUID, 26760);
        assert_eq!(
            val(&both, "udp_input_servers"),
            "10.0.0.4:25000,127.0.0.1:26760"
        );
        let one = apply_layout(&both, &[pro(0, 2)], GUID, 26760);
        assert_eq!(val(&one, "player_1_connected"), "false");
        assert_eq!(val(&one, "player_7_type"), "1");
        assert!(one.contains("player_7_button_a=keyboard\n"));
    }
    #[test]
    fn qt_resave_with_sorted_keys_is_idempotent() {
        let text = apply_layout("", &[pro(0, 0)], GUID, 26760);
        let mut parsed = ini::parse_ini(&text);
        for (_, body) in &mut parsed.sections {
            body.sort();
        }
        parsed.crlf = true;
        let saved = ini::serialize_ini(&parsed);
        assert_eq!(apply_layout(&saved, &[pro(0, 0)], GUID, 26760), saved);
    }
    #[test]
    fn restore_changes_only_owned_players_and_global_keys() {
        let original="[Controls]\nplayer_0_button_a=keyboard\nplayer_0_type=0\nplayer_7_type=1\nudp_input_servers\\default=false\nudp_input_servers=10.0.0.5:25000\n[Audio]\nvolume=75\n";
        let ours = apply_layout(original, &[pro(0, 0)], GUID, 26760)
            .replace("volume=75", "volume=88")
            .replace("player_7_type=1", "player_7_type=3");
        let restored = restore_keys(&ours, original, GUID, 26760);
        assert!(restored.contains("player_0_button_a=keyboard\n"));
        assert!(!restored.contains("player_0_button_a\\default"));
        assert!(!restored.contains("player_0_motion"));
        assert!(restored.contains("volume=88"));
        assert!(restored.contains("player_7_type=3"));
        assert_eq!(val(&restored, "udp_input_servers"), "10.0.0.5:25000");
    }
    #[test]
    fn creates_file_backs_up_once_and_captures_new_players_at_takeover() {
        let dir = temp("backup");
        let file = dir.join("qt-config.ini");
        write_config(&file, &[pro(0, 0)], 26760).unwrap();
        assert_eq!(read_config(&backup_path(&file)).unwrap(), "");
        let text = read_config(&file).unwrap();
        let new = text.replace(
            "[Controls]",
            "[Controls]\nplayer_1_button_a=New keyboard\nplayer_6_type=2",
        );
        ini::atomic_write(&file, &new).unwrap();
        write_config(&file, &[pro(0, 0), pro(1, 1)], 26760).unwrap();
        let before = read_config(&backup_path(&file)).unwrap();
        assert!(before.contains("player_1_button_a=New keyboard"));
        assert!(!before.contains("engine:cemuhookudp"));
        write_config(&file, &[pro(0, 0), pro(1, 1)], 26760).unwrap();
        assert_eq!(read_config(&backup_path(&file)).unwrap(), before);
        assert!(restore_config(&file, 26760).unwrap());
        let restored = read_config(&file).unwrap();
        assert!(restored.contains("player_1_button_a=New keyboard"));
        assert!(restored.contains("player_6_type=2"));
        assert!(!restored.contains("engine:cemuhookudp"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn paths_portable_roaming_xdg_and_no_installation() {
        let dir = temp("paths");
        let exe = dir.join("Eden");
        std::fs::create_dir_all(exe.join("user")).unwrap();
        let mut env = paths::Env {
            exe_dirs: vec![exe.clone()],
            appdata: Some(dir.join("roaming")),
            ..Default::default()
        };
        let files = paths::resolve_config_files(&env);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, exe.join("user/config/qt-config.ini"));
        env.exe_dirs.push(dir.join("Eden2"));
        assert_eq!(paths::resolve_config_files(&env).len(), 2);
        env.exe_dirs.clear();
        assert!(paths::resolve_config_files(&env).is_empty());
        env.appdata = None;
        env.xdg_config_home = Some(dir.join("xdg"));
        env.exe_dirs.push(dir.join("AppImage"));
        assert_eq!(
            paths::resolve_config_files(&env)[0].path,
            dir.join("xdg/eden/qt-config.ini")
        );
        env.xdg_config_home = None;
        env.home = Some(dir.clone());
        assert_eq!(
            paths::resolve_config_files(&env)[0].path,
            dir.join(".config/eden/qt-config.ini")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn eden_uses_global_pad_indices_across_dsu_servers() {
        let before="[Controls]\nudp_input_servers\\default=false\nudp_input_servers=10.0.0.5:25000\nplayer_7_motionleft=engine:cemuhookudp,guid:remote,pad:0,port:25000\n";
        let after = apply_layout(before, &[pro(0, 2)], GUID, 26760);
        assert_eq!(
            val(&after, "udp_input_servers"),
            "10.0.0.5:25000,127.0.0.1:26760"
        );
        assert!(val(&after, "player_0_motionleft").contains("pad:6"));
        assert!(
            after.contains("player_7_motionleft=engine:cemuhookudp,guid:remote,pad:0,port:25000")
        );
    }
    #[test]
    fn restoring_preserves_servers_added_after_takeover_without_renumbering() {
        let before="[Controls]\nudp_input_servers\\default=false\nudp_input_servers=10.0.0.5:25000\nenable_udp_controller\\default=false\nenable_udp_controller=false\n";
        let after = apply_layout(before, &[pro(0, 0)], GUID, 26760);
        let mut parsed = ini::parse_ini(&after);
        let mut body = controls(&parsed).to_vec();
        ini::set_qt_key(
            &mut body,
            "udp_input_servers",
            "\"10.0.0.5:25000,127.0.0.1:26760,10.0.0.9:28000\"",
        );
        ini::set_section(&mut parsed, "Controls", body);
        let restored = restore_keys(&ini::serialize_ini(&parsed), before, GUID, 26760);
        assert_eq!(
            val(&restored, "udp_input_servers"),
            "10.0.0.5:25000,127.0.0.1:26760,10.0.0.9:28000"
        );
        assert_eq!(val(&restored, "enable_udp_controller"), "true");
        assert!(!restored.contains("player_0_motionleft"));
    }
    #[test]
    fn restoring_does_not_undo_a_later_udp_toggle_change() {
        let before =
            "[Controls]\nenable_udp_controller\\default=false\nenable_udp_controller=true\n";
        let after = apply_layout(before, &[pro(0, 0)], GUID, 26760)
            .replace("enable_udp_controller=true", "enable_udp_controller=false");
        assert_eq!(
            val(
                &restore_keys(&after, before, GUID, 26760),
                "enable_udp_controller"
            ),
            "false"
        );
    }
    #[test]
    fn explicit_config_file_and_folder_are_both_recognized() {
        let dir = temp("selected");
        let file = dir.join("qt-config.ini");
        std::fs::write(&file, "[Controls]\n").unwrap();
        assert!(has_selected_config(&dir));
        assert!(has_selected_config(&file));
        assert!(!has_selected_config(&dir.join("eden")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn pending_restore_survives_settings_roundtrip() {
        let c: Config = serde_json::from_str(
            r#"{"sens_deg":40.0,"abs_mode":true,"auto_eden":false,"eden_restore_pending":true}"#,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(c).unwrap()["eden_restore_pending"],
            true
        );
    }
}
