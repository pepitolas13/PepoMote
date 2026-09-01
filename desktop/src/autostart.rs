//! Autoarranque opcional con el sistema.
//! Windows: HKCU\...\Run (todo Windows, sin admin), arrancando --minimized.
//! Linux: XDG autostart (~/.config/autostart/PepoMote.desktop) — el estándar
//! freedesktop que respetan todos los escritorios (GNOME, KDE, XFCE, MATE,
//! Cinnamon, LXQt...). El propio archivo/clave ES el estado: nada duplicado.

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "PepoMote";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn reg(args: &[&str]) -> std::io::Result<std::process::ExitStatus> {
        Command::new("reg")
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .status()
    }

    pub fn is_enabled() -> bool {
        reg(&["query", RUN_KEY, "/v", VALUE])
            .map(|s| s.success())
            .unwrap_or(false)
    }

    pub fn set_enabled(on: bool) -> Result<(), String> {
        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let cmd = format!("\"{}\" --minimized", exe.display());
            let ok = reg(&["add", RUN_KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &cmd, "/f"])
                .map(|s| s.success())
                .unwrap_or(false);
            if ok { Ok(()) } else { Err("no pude escribir en el registro".into()) }
        } else {
            let _ = reg(&["delete", RUN_KEY, "/v", VALUE, "/f"]);
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::path::PathBuf;

    fn desktop_path() -> Option<PathBuf> {
        directories::BaseDirs::new()
            .map(|b| b.config_dir().join("autostart").join("PepoMote.desktop"))
    }

    /// El AppImage expone su propia ruta en $APPIMAGE; fuera de AppImage,
    /// el binario actual.
    fn exe_path() -> Option<String> {
        std::env::var("APPIMAGE")
            .ok()
            .or_else(|| std::env::current_exe().ok().map(|p| p.display().to_string()))
    }

    pub fn is_enabled() -> bool {
        desktop_path().map(|p| p.exists()).unwrap_or(false)
    }

    pub fn set_enabled(on: bool) -> Result<(), String> {
        let path = desktop_path().ok_or("sin directorio de config")?;
        if on {
            let exe = exe_path().ok_or("no sé mi propia ruta")?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=PepoMote\n\
                 Comment=Tu móvil, mando de tu PC\nExec=\"{exe}\"\n\
                 Terminal=false\nX-GNOME-Autostart-enabled=true\n"
            );
            std::fs::write(&path, content).map_err(|e| e.to_string())
        } else {
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            }
            Ok(())
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn is_enabled() -> bool {
        false
    }
    pub fn set_enabled(_on: bool) -> Result<(), String> {
        Err("plataforma sin soporte".into())
    }
}

pub use imp::{is_enabled, set_enabled};
