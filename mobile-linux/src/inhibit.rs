//! Pantalla encendida y sin suspensión mientras el mando está conectado.
//! En Android lo hacen FLAG_KEEP_SCREEN_ON y el servicio en primer plano;
//! en Linux móvil el compositor apaga la pantalla con el tiempo de bloqueo
//! del sistema y después el móvil se suspende, y eso corta la partida por la
//! mitad. winit no expone el protocolo idle-inhibit de Wayland, así que se
//! usa el inhibidor de sesión del escritorio como proceso hijo que vive lo
//! que dura la conexión:
//! - `gnome-session-inhibit` (Phosh, GNOME Mobile): pantalla + suspensión;
//! - `systemd-inhibit` / `elogind-inhibit` (Plasma Mobile, Sxmo…): solo evita
//!   la suspensión; la pantalla la sigue mandando el compositor.
//!
//! El hijo ejecuta `cat` leyendo de una tubería nuestra: si la app muere, la
//! tubería se cierra, `cat` ve EOF y el inhibidor se libera solo. Sin
//! ninguna de las tres herramientas no pasa nada: la app funciona igual.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

pub struct Inhibit {
    child: Child,
    pub tool: &'static str,
}

const CANDIDATES: [(&str, &[&str]); 3] = [
    (
        "gnome-session-inhibit",
        &["--inhibit", "idle:suspend", "--reason", "PepoMote: mando en uso", "cat"],
    ),
    (
        "systemd-inhibit",
        &["--what=idle:sleep", "--who=PepoMote", "--why=mando en uso", "--mode=block", "cat"],
    ),
    (
        "elogind-inhibit",
        &["--what=idle:sleep", "--who=PepoMote", "--why=mando en uso", "--mode=block", "cat"],
    ),
];

/// Lo que se espera a que el inhibidor falle de inmediato (sin gestor de
/// sesión, sin permiso de polkit…) antes de darlo por bueno.
const SETTLE: Duration = Duration::from_millis(150);

impl Inhibit {
    /// Arranca el primer inhibidor que exista y no muera al instante.
    pub fn start() -> Option<Inhibit> {
        for (tool, args) in CANDIDATES {
            let Ok(mut child) = Command::new(tool)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                continue; // no está instalado
            };
            std::thread::sleep(SETTLE);
            match child.try_wait() {
                Ok(None) => return Some(Inhibit { child, tool }),
                _ => {
                    let _ = child.wait(); // murió: sin zombi, y al siguiente
                }
            }
        }
        None
    }

    /// Sigue vivo (el inhibidor se mantiene).
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for Inhibit {
    fn drop(&mut self) {
        // Cerrar nuestra punta de la tubería basta (cat ve EOF y el
        // envoltorio termina); el kill es por si el envoltorio se quedara.
        drop(self.child.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_herramientas_no_estalla() {
        // En el runner de CI/Windows puede o no haber inhibidor: solo se
        // exige que no falle y que, si hay, se libere limpiamente.
        if let Some(mut i) = Inhibit::start() {
            assert!(i.alive());
            assert!(!i.tool.is_empty());
            drop(i);
        }
    }
}
