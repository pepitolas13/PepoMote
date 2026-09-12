//! Auto-reparación con privilegios (Linux): abre el firewall e instala los
//! permisos de uinput con UN diálogo de contraseña del sistema (pkexec),
//! como el UAC de Windows. Sin terminal, sin scripts, sin cerrar sesión.
//!
//! Solo hace falta donde el cursor va por uinput (GNOME, KDE, X11): en los
//! compositores wlroots el puntero virtual de Wayland no necesita nada.
//! Se dispara sola la primera vez que se detecta un bloqueo (flag persistido
//! en settings.json) y queda un botón «Reparar» en la ventana. Si la sesión
//! no tiene agente de polkit (gestores de ventanas «a pelo»), pkexec no
//! puede pedir la contraseña: entonces la ventana enseña el comando manual
//! ([`UINPUT_MANUAL_CMD`]) con un botón para copiarlo.

#[cfg(target_os = "linux")]
use crate::state::SharedState;
#[cfg(target_os = "linux")]
use std::process::Command;
use crate::tr;

/// Comando de una línea para quien no tiene pkexec ni diálogo de
/// contraseña: carga el módulo, lo deja cargado en cada arranque, instala
/// la regla udev y da acceso al usuario actual sin cerrar sesión. Con `;` a
/// propósito: si modprobe falla, el resto se aplica igual y udevadm/setfacl
/// dicen por qué.
pub const UINPUT_MANUAL_CMD: &str = r#"sudo sh -c 'modprobe uinput; printf "uinput\n" > /etc/modules-load.d/pepomote.conf; printf "KERNEL==\"uinput\", SUBSYSTEM==\"misc\", TAG+=\"uaccess\", OPTIONS+=\"static_node=uinput\"\n" > /etc/udev/rules.d/99-pepomote.rules; udevadm control --reload-rules; udevadm trigger /dev/uinput; setfacl -m "u:${SUDO_UID:-$(id -u)}:rw" /dev/uinput'"#;

/// Script POSIX que se ejecuta como root. Idempotente: cada paso comprueba
/// o sobreescribe, ejecutarlo dos veces no hace daño.
fn script(port: u16, user: &str) -> String {
    format!(
        r#"set -e
# --- uinput: cursor/teclado virtuales ---
modprobe uinput 2>/dev/null || true
[ -e /dev/uinput ] || {{ echo "no existe /dev/uinput tras modprobe: este kernel no trae uinput" >&2; exit 3; }}
mkdir -p /etc/udev/rules.d /etc/modules-load.d
printf 'KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"\n' > /etc/udev/rules.d/99-pepomote.rules
printf 'uinput\n' > /etc/modules-load.d/pepomote.conf
udevadm control --reload-rules 2>/dev/null || true
udevadm trigger /dev/uinput 2>/dev/null || true
# ACL directa al usuario que pidio la reparacion (pkexec exporta PKEXEC_UID)
# para no tener que cerrar sesion; la regla uaccess cubre las siguientes
u="${{PKEXEC_UID:-}}"; [ -n "$u" ] || u="$(id -u '{user}' 2>/dev/null || true)"
[ -n "$u" ] && setfacl -m "u:$u:rw" /dev/uinput 2>/dev/null || true
# --- firewall: puerto del movil ({port} TCP+UDP) y mDNS ---
if [ -f /etc/ufw/ufw.conf ] && grep -q '^ENABLED=yes' /etc/ufw/ufw.conf && command -v ufw >/dev/null 2>&1; then
    ufw allow {port}/tcp comment PepoMote >/dev/null
    ufw allow {port}/udp comment PepoMote >/dev/null
    ufw allow 5353/udp comment 'mDNS PepoMote' >/dev/null 2>&1 || true
elif [ -d /run/firewalld ] && command -v firewall-cmd >/dev/null 2>&1; then
    firewall-cmd --permanent --add-port={port}/tcp --add-port={port}/udp >/dev/null
    firewall-cmd --permanent --add-service=mdns >/dev/null 2>&1 || true
    firewall-cmd --reload >/dev/null
fi
"#
    )
}

/// Cómo acabó pkexec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixOutcome {
    Done,
    /// El usuario cerró el diálogo: sin drama, el botón sigue ahí.
    Cancelled,
    /// No hubo forma de autenticar: sesión sin agente de polkit (nadie
    /// puede pedir la contraseña) o contraseña incorrecta.
    NoAuth(String),
    /// El script falló (primera línea de stderr o el código).
    Failed(String),
}

/// pkexec: 0 bien; 126 diálogo cerrado; 127 no autorizado (sin agente de
/// polkit, contraseña mal); otro código = fallo del propio script.
pub fn classify(status: Option<i32>, stderr: &str) -> FixOutcome {
    let low = stderr.to_lowercase();
    let first = stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_owned();
    match status {
        Some(0) => FixOutcome::Done,
        Some(126) => FixOutcome::Cancelled,
        Some(127) if low.contains("dismissed") => FixOutcome::Cancelled,
        Some(127) if low.contains("authority") || low.contains("connect") => {
            FixOutcome::NoAuth(tr!("fix.polkit_silent", first))
        }
        Some(127) => FixOutcome::NoAuth(tr!("fix.no_auth").to_owned()),
        Some(n) => FixOutcome::Failed(if first.is_empty() { tr!("fix.exit_code", n) } else { first }),
        None => FixOutcome::Failed(tr!("fix.signal").to_owned()),
    }
}

#[cfg(target_os = "linux")]
pub fn pkexec_available() -> bool {
    Command::new("pkexec")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Lanza la reparación en su hilo (pkexec bloquea hasta que el usuario
/// responde al diálogo). El resultado se refleja en el estado compartido.
#[cfg(target_os = "linux")]
pub fn fix_all(shared: SharedState, port: u16) {
    {
        let mut s = shared.lock().unwrap();
        if s.fixing {
            return; // ya hay un diálogo abierto
        }
        s.fixing = true;
    }
    let _ = std::thread::Builder::new()
        .name("pmp-fix".into())
        .spawn(move || {
            let user = std::env::var("USER").unwrap_or_default();
            let out = Command::new("pkexec")
                .arg("/bin/sh")
                .arg("-c")
                .arg(script(port, &user))
                .output();
            let mut s = shared.lock().unwrap();
            s.fixing = false;
            match out {
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let outcome = classify(o.status.code(), &stderr);
                    crate::log_line!(
                        "Reparación (pkexec): código {:?} · {}",
                        o.status.code(),
                        stderr.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("(sin salida)")
                    );
                    match outcome {
                        FixOutcome::Done => {
                            // Los vigilantes (firewall cada 10 s, uinput cada
                            // 2 s) confirmarán; esto limpia la UI al instante.
                            s.firewall_hint = None;
                            s.last_error = None;
                            s.injection_error = false;
                            s.fix_failed = None;
                        }
                        FixOutcome::Cancelled => {}
                        FixOutcome::NoAuth(why) => s.fix_failed = Some(why),
                        FixOutcome::Failed(why) => s.fix_failed = Some(tr!("fix.failed", why)),
                    }
                }
                Err(e) => {
                    crate::log_line!("Reparación (pkexec): no se pudo lanzar: {e}");
                    s.fix_failed = Some(tr!("fix.no_pkexec_launch", e));
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_script_usa_pkexec_uid_y_comprueba_el_modulo() {
        let s = script(26761, "dan");
        assert!(s.contains("${PKEXEC_UID:-}"));
        assert!(s.contains("id -u 'dan'"));
        assert!(s.contains("setfacl -m \"u:$u:rw\" /dev/uinput"));
        assert!(s.contains("[ -e /dev/uinput ] || { echo"));
        assert!(s.contains("exit 3;"));
        assert!(s.contains("ufw allow 26761/tcp"));
        assert!(s.contains("--add-port=26761/tcp --add-port=26761/udp"));
        assert!(s.contains("TAG+=\"uaccess\""));
    }

    #[test]
    fn el_comando_manual_esta_bien_entrecomillado() {
        let c = UINPUT_MANUAL_CMD;
        assert_eq!(c.matches('\'').count(), 2, "una sola pareja de comillas simples");
        assert!(c.starts_with("sudo sh -c '"));
        assert!(c.ends_with('\''));
        assert!(!c.contains('\n'));
        for needle in [
            "modprobe uinput",
            "99-pepomote.rules",
            "modules-load.d/pepomote.conf",
            "${SUDO_UID:-$(id -u)}",
            "udevadm trigger /dev/uinput",
            "TAG+=\\\"uaccess\\\"",
        ] {
            assert!(c.contains(needle), "{needle}");
        }
    }

    #[test]
    fn pkexec_126_cancelado_127_sin_auth_otro_fallo() {
        assert_eq!(classify(Some(0), ""), FixOutcome::Done);
        assert_eq!(classify(Some(126), ""), FixOutcome::Cancelled);
        assert_eq!(
            classify(Some(127), "Error executing command as another user: Request dismissed\n"),
            FixOutcome::Cancelled
        );
        match classify(Some(127), "Error executing command as another user: Not authorized\n") {
            FixOutcome::NoAuth(w) => assert!(w.contains("agente de polkit")),
            o => panic!("{o:?}"),
        }
        match classify(Some(127), "Error getting authority: Could not connect: No such file\n") {
            FixOutcome::NoAuth(w) => assert!(w.starts_with("polkit no responde: Error getting authority")),
            o => panic!("{o:?}"),
        }
        assert_eq!(
            classify(Some(3), "\nno existe /dev/uinput tras modprobe: este kernel no trae uinput\n"),
            FixOutcome::Failed("no existe /dev/uinput tras modprobe: este kernel no trae uinput".to_owned())
        );
        assert_eq!(classify(Some(2), ""), FixOutcome::Failed("código 2".to_owned()));
        assert_eq!(classify(None, ""), FixOutcome::Failed("terminado por una señal".to_owned()));
    }
}
