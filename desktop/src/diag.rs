//! `PepoMote --diag`: informe de diagnóstico (sesión, inyección, audio,
//! firewall, puertos y últimas líneas del log) para pegar en un issue.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// `--diag` en los argumentos: imprime el informe, lo guarda como
/// `diagnostico.txt` junto al log y devuelve true (hay que salir).
pub fn run_from_args() -> bool {
    if !std::env::args().any(|a| a == "--diag") {
        return false;
    }
    #[cfg(windows)]
    attach_console();
    let report = report();
    println!("{report}");
    if let Some(dir) = crate::state::config_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("diagnostico.txt"), &report);
    }
    true
}

/// Valor de una variable de entorno, o `(no)`.
pub fn env_or(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "(no)".to_owned())
}

pub fn report() -> String {
    let mut out: Vec<String> = Vec::new();
    out.push(format!("PepoMote {} — diagnóstico", env!("CARGO_PKG_VERSION")));
    out.push(format!(
        "SO: {} {}{}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        os_details()
    ));
    match crate::state::config_dir() {
        Some(dir) => {
            let has = |n: &str| if dir.join(n).exists() { "sí" } else { "no" };
            out.push(format!(
                "Config: {} (existe: {}) · settings.json: {} · token.txt: {}",
                dir.display(),
                if dir.is_dir() { "sí" } else { "no" },
                has("settings.json"),
                has("token.txt")
            ));
        }
        None => out.push("Config: sin carpeta de configuración".to_owned()),
    }
    match crate::log::path() {
        Some(p) => {
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            out.push(format!(
                "Log: {} ({} líneas, {} KB)",
                p.display(),
                text.lines().count(),
                text.len() / 1024
            ));
        }
        None => out.push("Log: (sin carpeta de configuración)".to_owned()),
    }
    out.push(format!(
        "Sesión: XDG_SESSION_TYPE={} · WAYLAND_DISPLAY={} · DISPLAY={} · XDG_CURRENT_DESKTOP={} · XDG_RUNTIME_DIR={}",
        env_or("XDG_SESSION_TYPE"),
        env_or("WAYLAND_DISPLAY"),
        env_or("DISPLAY"),
        env_or("XDG_CURRENT_DESKTOP"),
        env_or("XDG_RUNTIME_DIR")
    ));
    let mut vars: Vec<String> = std::env::vars()
        .filter(|(k, _)| k.starts_with("PEPOMOTE_"))
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    vars.sort();
    out.push(format!(
        "Variables PEPOMOTE_*: {}",
        if vars.is_empty() { "(ninguna)".to_owned() } else { vars.join(" ") }
    ));
    out.push("Inyección:".to_owned());
    for l in crate::input::diag_lines() {
        out.push(format!("  {l}"));
    }
    out.push(format!("Audio: {}", audio_probe()));
    #[cfg(target_os = "linux")]
    out.push(format!(
        "Firewall: {}",
        crate::firewall::check(crate::pairing::port())
            .unwrap_or_else(|| "sin ufw/firewalld bloqueando el puerto".to_owned())
            .replace('\n', " ")
    ));
    out.extend(ports_section(crate::pairing::port(), crate::dsu::port()));
    #[cfg(target_os = "linux")]
    {
        let cfg = crate::state::Config::load();
        out.push(format!(
            "Reparación: pkexec: {} · fix_attempted: {} · sonido desactivado en esta sesión: {}",
            if crate::fixes::pkexec_available() { "sí" } else { "no" },
            cfg.fix_attempted,
            crate::sound::disabled()
        ));
    }
    out.push("Últimas 40 líneas del log:".to_owned());
    for l in crate::log::tail(40) {
        out.push(format!("  {l}"));
    }
    out.join("\n")
}

#[cfg(target_os = "linux")]
fn os_details() -> String {
    let pretty = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_owned()))
        })
        .unwrap_or_else(|| "(sin /etc/os-release)".to_owned());
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|k| k.trim().to_owned())
        .unwrap_or_else(|_| "?".to_owned());
    format!(" · {pretty} · kernel {kernel}")
}

#[cfg(not(target_os = "linux"))]
fn os_details() -> String {
    String::new()
}

/// Abre y cierra la salida de audio por defecto igual que la campanita,
/// capturando el pánico que ciertos drivers provocan en cpal.
fn audio_probe() -> String {
    match catch_unwind(AssertUnwindSafe(crate::sound::probe)) {
        Ok(Ok(())) => "salida por defecto abierta y cerrada sin problema".to_owned(),
        Ok(Err(e)) => format!("error: {e} (el receptor sigue, sin sonido)"),
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "(sin mensaje)".to_owned());
            format!("PÁNICO capturado al abrir el audio: {msg} (el receptor seguiría, sin sonido)")
        }
    }
}

/// Estado de los puertos: libre, o quién lo tiene. Solo mira (un bind de
/// prueba y `ports::owner`); nunca desaloja a nadie.
fn ports_section(port: u16, dsu: u16) -> Vec<String> {
    use crate::ports::{owner, Proto};
    let describe = |proto: Proto, p: u16, what: &str| -> String {
        let (name, free) = match proto {
            Proto::Tcp => ("TCP", std::net::TcpListener::bind(("0.0.0.0", p)).is_ok()),
            Proto::Udp => ("UDP", std::net::UdpSocket::bind(("0.0.0.0", p)).is_ok()),
        };
        if free {
            return format!("{name} {p} ({what}): libre");
        }
        match owner(p, proto) {
            Some(o) => format!(
                "{name} {p} ({what}): ocupado por {} (PID {}){}",
                o.name,
                o.pid,
                if o.name.to_lowercase().contains("pepomote") {
                    " — es el receptor abierto"
                } else {
                    ""
                }
            ),
            None => format!("{name} {p} ({what}): ocupado (dueño desconocido)"),
        }
    };
    vec![
        "Puertos:".to_owned(),
        format!("  {}", describe(Proto::Tcp, port, "móvil")),
        format!("  {}", describe(Proto::Udp, port, "móvil")),
        format!("  {}", describe(Proto::Udp, dsu, "DSU")),
        format!("  {}", describe(Proto::Udp, crate::singleton::port(), "instancia única")),
    ]
}

/// Windows: el exe es una app de ventana (sin consola). Si nos lanzaron
/// desde una terminal, se engancha a ella para que el informe se vea; con
/// la salida ya redirigida (`> diag.txt`) no se toca nada.
#[cfg(windows)]
fn attach_console() {
    use windows::Win32::System::Console::{
        AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE,
    };
    unsafe {
        let has_stdout = GetStdHandle(STD_OUTPUT_HANDLE).is_ok_and(|h| !h.is_invalid() && !h.0.is_null());
        if !has_stdout {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_informe_lleva_version_log_y_secciones() {
        let r = report();
        assert!(r.starts_with(&format!("PepoMote {} — diagnóstico", env!("CARGO_PKG_VERSION"))));
        for section in ["Config:", "Log:", "Sesión:", "Inyección:", "Audio:", "Puertos:", "Últimas 40 líneas del log:"] {
            assert!(r.contains(section), "{section}");
        }
    }

    #[test]
    fn env_or_devuelve_no_si_falta() {
        assert_eq!(env_or("PEPOMOTE_VARIABLE_QUE_NO_EXISTE_XYZ"), "(no)");
    }
}
