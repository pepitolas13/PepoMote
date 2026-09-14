//! Firewalls de Linux que bloquean al móvil: detección sin privilegios y
//! oferta de reparación (el trabajo con root lo hace `fixes`).
//!
//! Muchas distros (CachyOS, Fedora, openSUSE…) traen un firewall activado con
//! política de entrada DROP: el receptor arranca perfecto pero ningún paquete
//! del móvil llega. Se detectan los dos habituales (ufw y firewalld) y solo se
//! dice «bloqueado» con pruebas: si las reglas de ufw no se pueden leer (van
//! 0640 root, lo normal) el estado es «no puedo comprobar», y una reparación
//! con éxito o un móvil que entra lo dan por abierto (`firewall_opened_port`
//! en settings.json). `packaging/linux/install.sh` abre estos puertos él
//! mismo; esto cubre a quien ejecuta el AppImage a pelo.

#[cfg(target_os = "linux")]
use crate::state::{LockTolerant, SharedState};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FwKind {
    Ufw,
    Firewalld,
}

impl FwKind {
    pub fn name(self) -> &'static str {
        match self {
            FwKind::Ufw => "ufw",
            FwKind::Firewalld => "firewalld",
        }
    }
}

/// Estado del firewall respecto al puerto del móvil.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FwStatus {
    /// Ni ufw ni firewalld activos.
    NoFirewall,
    /// Activo y con el puerto permitido (TCP y UDP).
    Open(FwKind),
    /// Activo y se ha visto que el puerto NO está permitido.
    Blocked(FwKind),
    /// Activo, pero sus reglas no se pueden consultar sin root.
    Unknown(FwKind),
}

/// Lo que ve la ventana: qué firewall y si el bloqueo está probado.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FirewallIssue {
    pub kind: FwKind,
    pub certain: bool,
}

/// ufw: `/etc/ufw/ufw.conf` dice si está activado; `user.rules` (si se puede
/// leer) lleva las reglas allow, una línea `-A ufw-user-input -p tcp --dport
/// 26761 -j ACCEPT` por protocolo.
#[cfg(any(target_os = "linux", test))]
pub fn ufw_status(conf: &str, rules: Option<&str>, port: u16) -> FwStatus {
    let enabled = conf.lines().any(|l| l.trim().eq_ignore_ascii_case("ENABLED=yes"));
    if !enabled {
        return FwStatus::NoFirewall;
    }
    let Some(rules) = rules else {
        return FwStatus::Unknown(FwKind::Ufw);
    };
    let port = port.to_string();
    let allows = |proto: &str| {
        rules.lines().any(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            let has = |flag: &str, value: &str| t.windows(2).any(|w| w[0] == flag && w[1] == value);
            has("-p", proto) && has("--dport", &port) && has("-j", "ACCEPT")
        })
    };
    if allows("tcp") && allows("udp") {
        FwStatus::Open(FwKind::Ufw)
    } else {
        FwStatus::Blocked(FwKind::Ufw)
    }
}

/// firewalld: `firewall-cmd --query-port` contesta 0 (sí) o 1 (no); otro
/// código (sin permiso para consultar, D-Bus caído) = no se sabe.
#[cfg(any(target_os = "linux", test))]
pub fn firewalld_status(tcp: Option<bool>, udp: Option<bool>) -> FwStatus {
    match (tcp, udp) {
        (Some(true), Some(true)) => FwStatus::Open(FwKind::Firewalld),
        (Some(false), _) | (_, Some(false)) => FwStatus::Blocked(FwKind::Firewalld),
        _ => FwStatus::Unknown(FwKind::Firewalld),
    }
}

/// Qué enseñar (si algo). `opened_port` = puerto que consta abierto en
/// settings (reparación con éxito o un móvil que ya entró). Un bloqueo VISTO
/// manda sobre settings (alguien pudo cerrar el puerto después).
#[cfg(any(target_os = "linux", test))]
pub fn issue_for(status: &FwStatus, opened_port: Option<u16>, port: u16) -> Option<FirewallIssue> {
    match status {
        FwStatus::NoFirewall | FwStatus::Open(_) => None,
        FwStatus::Blocked(k) => Some(FirewallIssue { kind: *k, certain: true }),
        FwStatus::Unknown(k) => {
            if opened_port == Some(port) {
                None
            } else {
                Some(FirewallIssue { kind: *k, certain: false })
            }
        }
    }
}

/// Para el log y `--diag`.
#[cfg(any(target_os = "linux", test))]
pub fn describe(status: &FwStatus) -> String {
    match status {
        FwStatus::NoFirewall => "sin ufw ni firewalld activos".to_owned(),
        FwStatus::Open(k) => format!("{} activo y el puerto del móvil permitido", k.name()),
        FwStatus::Blocked(k) => format!("{} activo y el puerto del móvil NO permitido", k.name()),
        FwStatus::Unknown(k) => format!("{} activo; sus reglas no se pueden leer sin root", k.name()),
    }
}

/// Evalúa el estado ahora (archivos y `firewall-cmd`, sin privilegios).
#[cfg(target_os = "linux")]
pub fn check(port: u16) -> FwStatus {
    if let Ok(conf) = std::fs::read_to_string("/etc/ufw/ufw.conf") {
        let rules = std::fs::read_to_string("/etc/ufw/user.rules").ok();
        let st = ufw_status(&conf, rules.as_deref(), port);
        if st != FwStatus::NoFirewall {
            return st;
        }
    }
    if std::path::Path::new("/run/firewalld").is_dir() {
        let query = |proto: &str| {
            std::process::Command::new("firewall-cmd")
                .arg(format!("--query-port={port}/{proto}"))
                .output()
                .ok()
                .and_then(|o| match o.status.code() {
                    Some(0) => Some(true),
                    Some(1) => Some(false),
                    _ => None,
                })
        };
        return firewalld_status(query("tcp"), query("udp"));
    }
    FwStatus::NoFirewall
}

/// Vigila el firewall en un hilo aparte: cada pocos segundos re-evalúa, así
/// el aviso aparece al arrancar y desaparece cuando el puerto se abre (o se
/// conecta un móvil, que es la prueba definitiva y queda en settings).
///
/// La primera vez que hay algo probado que reparar (uinput, o un bloqueo
/// visto) marca `auto_fix_due`: la VENTANA enseña la explicación y, tras unos
/// segundos a la vista, lanza la reparación (`fixes`). Con solo «no puedo
/// comprobar» no hay cuenta atrás: queda el botón.
#[cfg(target_os = "linux")]
pub fn watch(shared: SharedState, port: u16) {
    use std::time::Duration;
    let _ = crate::threads::spawn_guarded(
        "pmp-firewall",
        crate::threads::OnPanic::Restart { after: Duration::from_secs(10), max: 20 },
        move || {
            let autofix = std::env::var_os("PEPOMOTE_NO_AUTOFIX").is_none() && crate::fixes::pkexec_available();
            let mut offered = false;
            // Que la ventana y el primer intento de uinput existan antes
            std::thread::sleep(Duration::from_millis(1500));
            loop {
                let connected = shared.lock_tolerant().player_count() > 0;
                if connected {
                    let mut s = shared.lock_tolerant();
                    s.firewall = None;
                    if s.config.firewall_opened_port != Some(port) {
                        s.config.firewall_opened_port = Some(port);
                        let cfg = s.config.clone();
                        drop(s);
                        cfg.save();
                        crate::log_line!("Firewall: un móvil ha entrado por el puerto {port}: consta como abierto");
                    }
                } else {
                    let status = check(port);
                    let mut s = shared.lock_tolerant();
                    let issue = issue_for(&status, s.config.firewall_opened_port, port);
                    if s.firewall != issue {
                        crate::log_line!("Firewall: {}", describe(&status));
                    }
                    s.firewall = issue;
                    let broken = s.uinput_denied || s.uinput_missing || issue.is_some_and(|i| i.certain);
                    if autofix && broken && !s.config.fix_attempted && !offered {
                        offered = true;
                        s.auto_fix_due = true;
                        crate::log_line!(
                            "Reparación automática: se ofrece en la ventana (uinput denegado: {} · sin módulo: {} · firewall: {:?})",
                            s.uinput_denied,
                            s.uinput_missing,
                            issue
                        );
                    }
                }
                std::thread::sleep(Duration::from_secs(10));
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONF_ON: &str = "# ufw.conf\nENABLED=yes\nLOGLEVEL=low\n";
    const RULES_OPEN: &str = "*filter\n-A ufw-user-input -p tcp --dport 26761 -j ACCEPT\n-A ufw-user-input -p udp --dport 26761 -j ACCEPT\n-A ufw-user-input -p udp --dport 5353 -j ACCEPT\nCOMMIT\n";

    #[test]
    fn ufw_apagado_abierto_bloqueado_o_ilegible() {
        assert_eq!(ufw_status("ENABLED=no\n", Some(RULES_OPEN), 26761), FwStatus::NoFirewall);
        assert_eq!(ufw_status(CONF_ON, Some(RULES_OPEN), 26761), FwStatus::Open(FwKind::Ufw));
        // solo TCP, o un puerto que empieza igual, no vale
        let solo_tcp = "-A ufw-user-input -p tcp --dport 26761 -j ACCEPT\n";
        assert_eq!(ufw_status(CONF_ON, Some(solo_tcp), 26761), FwStatus::Blocked(FwKind::Ufw));
        let otro = "-A ufw-user-input -p tcp --dport 267610 -j ACCEPT\n-A ufw-user-input -p udp --dport 267610 -j ACCEPT\n";
        assert_eq!(ufw_status(CONF_ON, Some(otro), 26761), FwStatus::Blocked(FwKind::Ufw));
        assert_eq!(ufw_status(CONF_ON, Some(""), 26761), FwStatus::Blocked(FwKind::Ufw));
        // reglas ilegibles (0640 root): no se sabe, nunca «bloqueado»
        assert_eq!(ufw_status(CONF_ON, None, 26761), FwStatus::Unknown(FwKind::Ufw));
    }

    #[test]
    fn firewalld_segun_las_dos_consultas() {
        assert_eq!(firewalld_status(Some(true), Some(true)), FwStatus::Open(FwKind::Firewalld));
        assert_eq!(firewalld_status(Some(true), Some(false)), FwStatus::Blocked(FwKind::Firewalld));
        assert_eq!(firewalld_status(None, Some(false)), FwStatus::Blocked(FwKind::Firewalld));
        assert_eq!(firewalld_status(None, None), FwStatus::Unknown(FwKind::Firewalld));
        assert_eq!(firewalld_status(Some(true), None), FwStatus::Unknown(FwKind::Firewalld));
    }

    #[test]
    fn el_aviso_solo_con_pruebas_o_sin_constancia_de_apertura() {
        assert_eq!(issue_for(&FwStatus::NoFirewall, None, 26761), None);
        assert_eq!(issue_for(&FwStatus::Open(FwKind::Ufw), None, 26761), None);
        assert_eq!(
            issue_for(&FwStatus::Blocked(FwKind::Ufw), Some(26761), 26761),
            Some(FirewallIssue { kind: FwKind::Ufw, certain: true })
        );
        assert_eq!(
            issue_for(&FwStatus::Unknown(FwKind::Ufw), None, 26761),
            Some(FirewallIssue { kind: FwKind::Ufw, certain: false })
        );
        assert_eq!(issue_for(&FwStatus::Unknown(FwKind::Ufw), Some(26761), 26761), None);
        // consta abierto OTRO puerto (PEPOMOTE_PORT cambió): no vale
        assert_eq!(
            issue_for(&FwStatus::Unknown(FwKind::Firewalld), Some(26700), 26761),
            Some(FirewallIssue { kind: FwKind::Firewalld, certain: false })
        );
    }

    #[test]
    fn la_descripcion_nombra_el_firewall() {
        assert!(describe(&FwStatus::Unknown(FwKind::Ufw)).starts_with("ufw activo; sus reglas"));
        assert!(describe(&FwStatus::Blocked(FwKind::Firewalld)).contains("NO permitido"));
    }
}
