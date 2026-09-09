//! Detección de firewalls Linux que bloquean al móvil (solo aviso en la UI).
//!
//! Muchas distros (CachyOS, Fedora, openSUSE…) traen un firewall activado con
//! política de entrada DROP: el receptor arranca perfecto pero ningún paquete
//! del móvil llega. Aquí se detectan los dos habituales (ufw y firewalld) sin
//! privilegios y se le da al usuario el comando exacto para abrir el puerto.
//! `packaging/linux/install.sh` abre estos puertos él mismo; esto cubre a
//! quien ejecuta el AppImage a pelo.

use crate::state::SharedState;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Vigila el firewall en un hilo aparte: cada pocos segundos re-evalúa, así
/// el aviso aparece al arrancar y desaparece solo cuando el usuario abre el
/// puerto (o se conecta un móvil, que es la prueba definitiva).
///
/// La primera vez que se detecta un bloqueo (firewall o uinput) lanza la
/// auto-reparación de `fixes` — un diálogo de contraseña del sistema — y lo
/// apunta en settings.json para no insistir en cada arranque si se canceló.
pub fn watch(shared: SharedState, port: u16) {
    let _ = std::thread::Builder::new()
        .name("pmp-firewall".into())
        .spawn(move || {
            let autofix = std::env::var_os("PEPOMOTE_NO_AUTOFIX").is_none()
                && crate::fixes::pkexec_available();
            // Que la ventana exista antes de que pueda saltar el diálogo
            std::thread::sleep(Duration::from_millis(1500));
            loop {
                let hint = {
                    let connected = shared.lock().unwrap().player_count() > 0;
                    if connected {
                        None // hay un móvil dentro: el firewall no está bloqueando
                    } else {
                        check(port)
                    }
                };
                let auto_fix = {
                    let mut s = shared.lock().unwrap();
                    s.firewall_hint = hint;
                    let broken = s.firewall_hint.is_some() || s.uinput_denied;
                    if autofix && broken && !s.config.fix_attempted {
                        s.config.fix_attempted = true;
                        let cfg = s.config.clone();
                        drop(s);
                        cfg.save();
                        true
                    } else {
                        false
                    }
                };
                if auto_fix {
                    crate::fixes::fix_all(shared.clone(), port);
                }
                std::thread::sleep(Duration::from_secs(10));
            }
        });
}

/// None = sin firewall o puerto abierto. Some(aviso) = probablemente bloqueado.
fn check(port: u16) -> Option<String> {
    if let Some(hint) = check_ufw(port) {
        return Some(hint);
    }
    if let Some(hint) = check_firewalld(port) {
        return Some(hint);
    }
    None
}

/// ufw (CachyOS, Ubuntu…): `/etc/ufw/ufw.conf` dice si está activado y
/// `user.rules` (legible en las distros habituales) lleva las reglas allow.
fn check_ufw(port: u16) -> Option<String> {
    let conf = std::fs::read_to_string("/etc/ufw/ufw.conf").ok()?;
    let enabled = conf
        .lines()
        .any(|l| l.trim().eq_ignore_ascii_case("ENABLED=yes"));
    if !enabled {
        return None;
    }
    // Si las reglas son legibles y ya permiten el puerto, no hay nada que avisar
    if let Ok(rules) = std::fs::read_to_string("/etc/ufw/user.rules") {
        if rules.contains(&format!("--dport {port} ")) || rules.contains(&format!("--dport {port}\n")) {
            return None;
        }
    }
    Some(format!(
        "El firewall (ufw) está bloqueando al móvil. Ábrelo con:\n\
         sudo ufw allow {port}/tcp && sudo ufw allow {port}/udp"
    ))
}

/// firewalld (Fedora, openSUSE…): si está corriendo, `firewall-cmd
/// --query-port` funciona sin root para consultar.
fn check_firewalld(port: u16) -> Option<String> {
    if !Path::new("/run/firewalld").is_dir() {
        return None;
    }
    let open = |proto: &str| {
        Command::new("firewall-cmd")
            .arg(format!("--query-port={port}/{proto}"))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if open("tcp") && open("udp") {
        return None;
    }
    Some(format!(
        "El firewall (firewalld) está bloqueando al móvil. Ábrelo con:\n\
         sudo firewall-cmd --permanent --add-port={port}/tcp --add-port={port}/udp && sudo firewall-cmd --reload"
    ))
}
