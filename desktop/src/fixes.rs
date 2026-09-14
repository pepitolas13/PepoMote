//! Reparación con privilegios (Linux): instala los permisos de uinput y abre
//! el firewall con UN diálogo de contraseña del sistema (pkexec), como el
//! UAC de Windows. Sin terminal, sin scripts, sin cerrar sesión.
//!
//! Solo hace falta uinput donde el cursor va por él (GNOME, KDE, X11): en los
//! compositores wlroots el puntero virtual de Wayland no necesita nada. Las
//! dos secciones del script (uinput y firewall) son independientes y cada una
//! informa por stdout de cómo acabó; la ventana lo enseña («Listo: …»). La
//! primera vez se ofrece sola desde la ventana (tras leer la explicación);
//! si el usuario cancela o todo fue bien, no se insiste (`fix_attempted`);
//! si no hubo forma de autenticar, el siguiente arranque lo vuelve a
//! ofrecer. Sin agente de polkit (gestores de ventanas «a pelo»), la ventana
//! enseña los comandos manuales con un botón para copiarlos.

use crate::firewall::FwKind;
#[cfg(target_os = "linux")]
use crate::state::{LockTolerant, SharedState};
use crate::tr;
use std::time::{Duration, Instant};

/// Comando de una línea para quien no tiene pkexec ni diálogo de
/// contraseña: carga el módulo, lo deja cargado en cada arranque, instala
/// la regla udev y da acceso al usuario actual sin cerrar sesión. Con `;` a
/// propósito: si modprobe falla, el resto se aplica igual y udevadm/setfacl
/// dicen por qué.
pub const UINPUT_MANUAL_CMD: &str = r#"sudo sh -c 'modprobe uinput; printf "uinput\n" > /etc/modules-load.d/pepomote.conf; printf "KERNEL==\"uinput\", SUBSYSTEM==\"misc\", TAG+=\"uaccess\", OPTIONS+=\"static_node=uinput\"\n" > /etc/udev/rules.d/99-pepomote.rules; udevadm control --reload-rules; udevadm trigger /dev/uinput; setfacl -m "u:${SUDO_UID:-$(id -u)}:rw" /dev/uinput'"#;

/// Comando manual para abrir el puerto del móvil (y mDNS) en el firewall.
pub fn firewall_manual_cmd(kind: FwKind, port: u16) -> String {
    match kind {
        FwKind::Ufw => format!("sudo ufw allow {port}/tcp && sudo ufw allow {port}/udp && sudo ufw allow 5353/udp"),
        FwKind::Firewalld => format!(
            "sudo firewall-cmd --permanent --add-port={port}/tcp --add-port={port}/udp --add-service=mdns && sudo firewall-cmd --reload"
        ),
    }
}

/// Segundos que la tarjeta de reparación está a la vista antes de abrir el
/// diálogo de contraseña por sí sola (así siempre se lee qué va a pedir).
pub const AUTO_FIX_DELAY: Duration = Duration::from_secs(3);

pub fn auto_fire_due(seen: Instant, now: Instant) -> bool {
    now.saturating_duration_since(seen) >= AUTO_FIX_DELAY
}

/// Código de salida del script: bits por sección fallida.
const RC_UINPUT: i32 = 4;
const RC_FIREWALL: i32 = 8;

/// Script POSIX que se ejecuta como root. Dos secciones independientes que
/// informan por stdout (`uinput: …`, `firewall: …`); ninguna aborta la otra.
/// Idempotente: ejecutarlo dos veces no hace daño. Salida: 0 todo bien, 4
/// falló uinput, 8 falló el firewall, 12 las dos (126/127 son de pkexec).
fn script(port: u16, user: &str) -> String {
    format!(
        r#"rc=0
err="$(mktemp 2>/dev/null || echo /dev/null)"
# --- uinput: cursor/teclado virtuales ---
modprobe uinput 2>/dev/null || true
if [ -e /dev/uinput ]; then
    mkdir -p /etc/udev/rules.d /etc/modules-load.d
    printf 'KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"\n' > /etc/udev/rules.d/99-pepomote.rules
    printf 'uinput\n' > /etc/modules-load.d/pepomote.conf
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger /dev/uinput 2>/dev/null || true
    # ACL directa al usuario que pidio la reparacion (pkexec exporta PKEXEC_UID)
    # para no tener que cerrar sesion; la regla uaccess cubre las siguientes
    u="${{PKEXEC_UID:-}}"; [ -n "$u" ] || u="$(id -u '{user}' 2>/dev/null || true)"
    if [ -n "$u" ] && setfacl -m "u:$u:rw" /dev/uinput 2>"$err"; then
        echo "uinput: ok"
    else
        echo "uinput: error $(head -c 200 "$err" 2>/dev/null | tr '\n' ' ')"; rc=$((rc | {RC_UINPUT}))
    fi
else
    echo "uinput: no-module"; rc=$((rc | {RC_UINPUT}))
fi
# --- firewall: puerto del movil ({port} TCP+UDP) y mDNS ---
if [ -f /etc/ufw/ufw.conf ] && grep -q '^ENABLED=yes' /etc/ufw/ufw.conf && command -v ufw >/dev/null 2>&1; then
    if ufw allow {port}/tcp comment PepoMote >/dev/null 2>"$err" && ufw allow {port}/udp comment PepoMote >/dev/null 2>"$err"; then
        ufw allow 5353/udp comment 'mDNS PepoMote' >/dev/null 2>&1 || true
        echo "firewall: ufw ok"
    else
        echo "firewall: error $(head -c 200 "$err" 2>/dev/null | tr '\n' ' ')"; rc=$((rc | {RC_FIREWALL}))
    fi
elif [ -d /run/firewalld ] && command -v firewall-cmd >/dev/null 2>&1; then
    if firewall-cmd --permanent --add-port={port}/tcp --add-port={port}/udp >/dev/null 2>"$err"; then
        firewall-cmd --permanent --add-service=mdns >/dev/null 2>&1 || true
        firewall-cmd --reload >/dev/null 2>&1 || true
        echo "firewall: firewalld ok"
    else
        echo "firewall: error $(head -c 200 "$err" 2>/dev/null | tr '\n' ' ')"; rc=$((rc | {RC_FIREWALL}))
    fi
else
    echo "firewall: none"
fi
[ "$err" = /dev/null ] || rm -f "$err"
exit $rc
"#
    )
}

/// Cómo acabó una sección del script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section {
    Applied,
    /// No hacía falta (sin firewall activo).
    NotNeeded,
    /// uinput: este kernel no trae el módulo.
    NoModule,
    Failed(String),
    /// El script no llegó a informar (matado, o pkexec no lo ejecutó).
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixReport {
    pub uinput: Section,
    pub firewall: Section,
    pub firewall_kind: Option<FwKind>,
}

/// Lee las líneas `uinput: …` y `firewall: …` del stdout del script.
pub fn parse_report(stdout: &str) -> FixReport {
    let mut r = FixReport { uinput: Section::Unknown, firewall: Section::Unknown, firewall_kind: None };
    let failed = |v: &str| Section::Failed(v.strip_prefix("error").map(str::trim).unwrap_or(v).to_owned());
    for l in stdout.lines().map(str::trim) {
        if let Some(v) = l.strip_prefix("uinput:") {
            r.uinput = match v.trim() {
                "ok" => Section::Applied,
                "no-module" => Section::NoModule,
                other => failed(other),
            };
        } else if let Some(v) = l.strip_prefix("firewall:") {
            r.firewall = match v.trim() {
                "ufw ok" => {
                    r.firewall_kind = Some(FwKind::Ufw);
                    Section::Applied
                }
                "firewalld ok" => {
                    r.firewall_kind = Some(FwKind::Firewalld);
                    Section::Applied
                }
                "none" => Section::NotNeeded,
                other => failed(other),
            };
        }
    }
    r
}

/// Cómo acabó pkexec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixOutcome {
    /// Todo lo que había que hacer, hecho.
    Done(FixReport),
    /// Alguna sección falló; el informe dice cuál.
    Partial(FixReport),
    /// El usuario cerró el diálogo: sin drama, el botón sigue ahí.
    Cancelled,
    /// No hubo forma de autenticar: sesión sin agente de polkit (nadie
    /// puede pedir la contraseña) o contraseña incorrecta.
    NoAuth(String),
    /// pkexec o el script fallaron de otra forma (primera línea de stderr o
    /// el código).
    Failed(String),
}

/// pkexec: 0 bien; 4/8/12 el script informa de qué falló; 126 diálogo
/// cerrado; 127 no autorizado (sin agente de polkit, contraseña mal); otro
/// código = fallo raro.
pub fn outcome(status: Option<i32>, stdout: &str, stderr: &str) -> FixOutcome {
    let low = stderr.to_lowercase();
    let first = stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_owned();
    match status {
        Some(0) => FixOutcome::Done(parse_report(stdout)),
        Some(n) if n & !(RC_UINPUT | RC_FIREWALL) == 0 => FixOutcome::Partial(parse_report(stdout)),
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

/// ¿Se persiste `fix_attempted` (no volver a ofrecerlo solo)? Éxito, éxito
/// parcial o cancelación explícita: sí. Sin forma de autenticar o fallo
/// raro: no, el siguiente arranque lo vuelve a ofrecer.
pub fn attempted_after(outcome: &FixOutcome) -> bool {
    !matches!(outcome, FixOutcome::NoAuth(_) | FixOutcome::Failed(_))
}

fn section_text(name: &str, s: &Section, kind: Option<FwKind>) -> String {
    match (name, s) {
        ("uinput", Section::Applied) => tr!("fix.sec_uinput_ok").to_owned(),
        ("uinput", Section::NoModule) => tr!("fix.sec_uinput_no_module").to_owned(),
        ("firewall", Section::Applied) => tr!("fix.sec_firewall_ok", kind.map(FwKind::name).unwrap_or("?")),
        ("firewall", Section::NotNeeded) => tr!("fix.sec_firewall_none").to_owned(),
        (_, Section::Failed(why)) => tr!("fix.sec_error", name, why),
        _ => tr!("fix.sec_error", name, "?"),
    }
}

/// Texto de la tarjeta «Listo» según el informe.
pub fn report_text(r: &FixReport) -> String {
    match (&r.uinput, &r.firewall) {
        (Section::Applied, Section::Applied) => tr!("fix.done_both").to_owned(),
        (Section::Applied, Section::NotNeeded) => tr!("fix.done_uinput").to_owned(),
        _ => tr!(
            "fix.done_partial",
            format!(
                "{} · {}",
                section_text("uinput", &r.uinput, None),
                section_text("firewall", &r.firewall, r.firewall_kind)
            )
        ),
    }
}

#[cfg(target_os = "linux")]
pub fn pkexec_available() -> bool {
    std::process::Command::new("pkexec")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Lanza la reparación en su hilo (pkexec bloquea hasta que el usuario
/// responde al diálogo). El resultado se refleja en el estado compartido.
#[cfg(target_os = "linux")]
pub fn fix_all(shared: SharedState, port: u16) {
    {
        let mut s = shared.lock_tolerant();
        if s.fixing {
            return; // ya hay un diálogo abierto
        }
        s.fixing = true;
        s.auto_fix_due = false;
    }
    let _ = crate::threads::spawn_once("pmp-fix", move || {
        /// Pase lo que pase en el hilo (pánico incluido), «Aplicando…» se quita.
        struct FixingGuard(SharedState);
        impl Drop for FixingGuard {
            fn drop(&mut self) {
                self.0.lock_tolerant().fixing = false;
            }
        }
        let _guard = FixingGuard(shared.clone());
        let user = std::env::var("USER").unwrap_or_default();
        let out = std::process::Command::new("pkexec")
            .arg("/bin/sh")
            .arg("-c")
            .arg(script(port, &user))
            .output();
        let mut s = shared.lock_tolerant();
        match out {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                for l in stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    crate::log_line!("Reparación: {l}");
                }
                crate::log_line!(
                    "Reparación (pkexec): código {:?} · {}",
                    o.status.code(),
                    stderr.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("(sin errores)")
                );
                let outcome = outcome(o.status.code(), &stdout, &stderr);
                s.config.fix_attempted = attempted_after(&outcome);
                match outcome {
                    FixOutcome::Done(r) | FixOutcome::Partial(r) => {
                        if r.firewall == Section::Applied {
                            s.firewall = None;
                            s.config.firewall_opened_port = Some(port);
                        }
                        if r.uinput == Section::Applied {
                            // telemetría reintenta el inyector cada 2 s y
                            // quita los flags de uinput; esto limpia la UI ya
                            s.last_error = None;
                            s.injection_error = false;
                        }
                        s.fix_failed = None;
                        s.fix_done = Some((report_text(&r), Instant::now()));
                    }
                    FixOutcome::Cancelled => {}
                    FixOutcome::NoAuth(why) => s.fix_failed = Some(why),
                    FixOutcome::Failed(why) => s.fix_failed = Some(tr!("fix.failed", why)),
                }
                let cfg = s.config.clone();
                drop(s);
                cfg.save();
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
    fn el_script_tiene_dos_secciones_independientes_que_informan() {
        let s = script(26761, "dan");
        assert!(!s.contains("set -e"), "una sección no aborta la otra");
        assert!(s.contains("${PKEXEC_UID:-}"));
        assert!(s.contains("id -u 'dan'"));
        assert!(s.contains("setfacl -m \"u:$u:rw\" /dev/uinput"));
        assert!(s.contains("echo \"uinput: ok\""));
        assert!(s.contains("echo \"uinput: no-module\""));
        assert!(s.contains("ufw allow 26761/tcp"));
        assert!(s.contains("--add-port=26761/tcp --add-port=26761/udp"));
        assert!(s.contains("echo \"firewall: ufw ok\""));
        assert!(s.contains("echo \"firewall: firewalld ok\""));
        assert!(s.contains("echo \"firewall: none\""));
        assert!(s.contains("TAG+=\"uaccess\""));
        assert!(s.contains("mktemp"), "stderr a un temporal seguro, no a una ruta fija");
        assert!(s.contains("rc=$((rc | 4))") && s.contains("rc=$((rc | 8))"));
        assert!(s.trim_end().ends_with("exit $rc"));
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
        assert!(firewall_manual_cmd(FwKind::Ufw, 26761).contains("ufw allow 26761/udp"));
        assert!(firewall_manual_cmd(FwKind::Firewalld, 26761).contains("--add-port=26761/udp"));
    }

    #[test]
    fn el_informe_se_lee_del_stdout() {
        let r = parse_report("uinput: ok\nfirewall: ufw ok\n");
        assert_eq!(r, FixReport { uinput: Section::Applied, firewall: Section::Applied, firewall_kind: Some(FwKind::Ufw) });
        let r = parse_report("uinput: no-module\nfirewall: firewalld ok\n");
        assert_eq!(r.uinput, Section::NoModule);
        assert_eq!(r.firewall_kind, Some(FwKind::Firewalld));
        let r = parse_report("uinput: error setfacl: Operation not supported\nfirewall: none\n");
        assert_eq!(r.uinput, Section::Failed("setfacl: Operation not supported".to_owned()));
        assert_eq!(r.firewall, Section::NotNeeded);
        assert_eq!(parse_report("").uinput, Section::Unknown);
    }

    #[test]
    fn el_desenlace_segun_el_codigo_de_pkexec() {
        let ok = "uinput: ok\nfirewall: none\n";
        assert!(matches!(outcome(Some(0), ok, ""), FixOutcome::Done(_)));
        assert!(matches!(outcome(Some(4), "uinput: no-module\nfirewall: ufw ok\n", ""), FixOutcome::Partial(_)));
        assert!(matches!(outcome(Some(8), "", ""), FixOutcome::Partial(_)));
        assert!(matches!(outcome(Some(12), "", ""), FixOutcome::Partial(_)));
        assert_eq!(outcome(Some(126), "", ""), FixOutcome::Cancelled);
        assert_eq!(
            outcome(Some(127), "", "Error executing command as another user: Request dismissed\n"),
            FixOutcome::Cancelled
        );
        match outcome(Some(127), "", "Error executing command as another user: Not authorized\n") {
            FixOutcome::NoAuth(w) => assert!(w.contains("agente de polkit")),
            o => panic!("{o:?}"),
        }
        match outcome(Some(127), "", "Error getting authority: Could not connect: No such file\n") {
            FixOutcome::NoAuth(w) => assert!(w.starts_with("polkit no responde: Error getting authority")),
            o => panic!("{o:?}"),
        }
        assert_eq!(outcome(Some(2), "", ""), FixOutcome::Failed("código 2".to_owned()));
        assert_eq!(outcome(None, "", ""), FixOutcome::Failed("terminado por una señal".to_owned()));
    }

    #[test]
    fn fix_attempted_solo_tras_exito_o_cancelacion() {
        let r = parse_report("uinput: ok\nfirewall: none\n");
        assert!(attempted_after(&FixOutcome::Done(r.clone())));
        assert!(attempted_after(&FixOutcome::Partial(r)));
        assert!(attempted_after(&FixOutcome::Cancelled));
        assert!(!attempted_after(&FixOutcome::NoAuth("x".into())));
        assert!(!attempted_after(&FixOutcome::Failed("x".into())));
    }

    #[test]
    fn el_texto_de_listo() {
        assert_eq!(report_text(&parse_report("uinput: ok\nfirewall: ufw ok\n")), "Listo: cursor y firewall configurados");
        assert_eq!(report_text(&parse_report("uinput: ok\nfirewall: none\n")), "Listo: cursor configurado (no hay firewall activo)");
        let t = report_text(&parse_report("uinput: no-module\nfirewall: firewalld ok\n"));
        assert!(t.starts_with("Aplicado en parte: "), "{t}");
        assert!(t.contains("no trae el módulo uinput") && t.contains("firewall abierto (firewalld)"), "{t}");
    }

    #[test]
    fn la_cuenta_atras_de_la_reparacion_automatica() {
        let t0 = Instant::now();
        assert!(!auto_fire_due(t0, t0));
        assert!(!auto_fire_due(t0, t0 + Duration::from_millis(2900)));
        assert!(auto_fire_due(t0, t0 + AUTO_FIX_DELAY));
        assert!(auto_fire_due(t0, t0 + Duration::from_secs(10)));
    }
}
