//! Auto-reparación con privilegios (Linux): abre el firewall e instala los
//! permisos de uinput con UN diálogo de contraseña del sistema (pkexec),
//! como el UAC de Windows. Sin terminal, sin scripts, sin cerrar sesión.
//!
//! Se dispara solo la primera vez que se detecta un bloqueo (flag persistido
//! en settings.json) y queda un botón "Reparar" en la ventana por si el
//! usuario canceló el diálogo o algo cambió después.

use crate::state::SharedState;
use std::process::Command;

/// Script POSIX que se ejecuta como root. Idempotente: cada paso comprueba
/// o sobreescribe, ejecutarlo dos veces no hace daño.
fn script(port: u16, user: &str) -> String {
    format!(
        r#"set -e
# --- uinput: cursor/teclado virtuales ---
modprobe uinput 2>/dev/null || true
mkdir -p /etc/udev/rules.d /etc/modules-load.d
printf 'KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"\n' > /etc/udev/rules.d/99-pepomote.rules
printf 'uinput\n' > /etc/modules-load.d/pepomote.conf
udevadm control --reload-rules 2>/dev/null || true
udevadm trigger /dev/uinput 2>/dev/null || true
# ACL directa para no tener que cerrar sesion (la regla uaccess cubre las siguientes)
setfacl -m 'u:{user}:rw' /dev/uinput 2>/dev/null || true
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

pub fn pkexec_available() -> bool {
    Command::new("pkexec")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Lanza la reparación en su hilo (pkexec bloquea hasta que el usuario
/// responde al diálogo). El resultado se refleja en el estado compartido.
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
                Ok(o) if o.status.success() => {
                    // Los vigilantes (firewall cada 10 s, uinput cada 2 s)
                    // confirmarán; esto limpia la UI al instante.
                    s.firewall_hint = None;
                    s.last_error = None;
                }
                Ok(o) if o.status.code() == Some(126) || o.status.code() == Some(127) => {
                    // Diálogo cancelado: sin drama, el botón sigue ahí
                }
                Ok(o) => {
                    let err = String::from_utf8_lossy(&o.stderr);
                    s.last_error = Some(format!("Reparación fallida: {}", err.trim()));
                }
                Err(e) => {
                    s.last_error = Some(format!("No pude lanzar pkexec: {e}"));
                }
            }
        });
}
