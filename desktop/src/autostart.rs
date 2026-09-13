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

/// macOS: LaunchAgent (~/Library/LaunchAgents/dev.pepotech.pepomote.plist)
/// que lanza el binario del .app con --minimized al iniciar sesión. Sale en
/// Ajustes → General → Ítems de inicio (como «Permitir en segundo plano»).
#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;

    const LABEL: &str = "dev.pepotech.pepomote";

    fn plist_path() -> Option<PathBuf> {
        directories::BaseDirs::new().map(|b| b.home_dir().join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
    }

    /// El binario dentro del .app (si corremos desde un bundle) o el actual.
    fn exe_path() -> Option<String> {
        crate::macos::bundle_path()
            .map(|b| b.join("Contents/MacOS/PepoMote").display().to_string())
            .or_else(|| std::env::current_exe().ok().map(|p| p.display().to_string()))
    }

    pub fn is_enabled() -> bool {
        plist_path().map(|p| p.exists()).unwrap_or(false)
    }

    pub fn set_enabled(on: bool) -> Result<(), String> {
        let path = plist_path().ok_or("sin carpeta de usuario")?;
        if on {
            let exe = exe_path().ok_or("no sé mi propia ruta")?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, super::plist_xml(&exe)).map_err(|e| e.to_string())
        } else {
            // mejor esfuerzo: que launchd lo olvide ya, no solo en el próximo inicio
            let _ = std::process::Command::new("launchctl")
                .args(["bootout", &format!("gui/{}/{LABEL}", unsafe { libc::getuid() })])
                .output();
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            }
            Ok(())
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    pub fn is_enabled() -> bool {
        false
    }
    pub fn set_enabled(_on: bool) -> Result<(), String> {
        Err("plataforma sin soporte".into())
    }
}

/// Contenido del LaunchAgent de macOS (puro: se prueba en cualquier SO).
#[cfg(any(target_os = "macos", test))]
pub fn plist_xml(exe: &str) -> String {
    let exe = exe.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \t<key>Label</key>\n\t<string>dev.pepotech.pepomote</string>\n\
         \t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>{exe}</string>\n\t\t<string>--minimized</string>\n\t</array>\n\
         \t<key>RunAtLoad</key>\n\t<true/>\n\
         \t<key>KeepAlive</key>\n\t<false/>\n\
         \t<key>LimitLoadToSessionType</key>\n\t<string>Aqua</string>\n\
         </dict>\n</plist>\n"
    )
}

pub use imp::{is_enabled, set_enabled};

#[cfg(test)]
mod tests {
    #[test]
    fn el_plist_lanza_minimizado_y_escapa_xml() {
        let p = super::plist_xml("/Applications/PepoMote.app/Contents/MacOS/PepoMote");
        assert!(p.starts_with("<?xml"));
        assert!(p.contains("<string>dev.pepotech.pepomote</string>"));
        assert!(p.contains("<string>/Applications/PepoMote.app/Contents/MacOS/PepoMote</string>\n\t\t<string>--minimized</string>"));
        assert!(p.contains("<key>RunAtLoad</key>\n\t<true/>"));
        let p = super::plist_xml("/Apps/A&B <x>/PepoMote");
        assert!(p.contains("<string>/Apps/A&amp;B &lt;x&gt;/PepoMote</string>"));
    }
}
