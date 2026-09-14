//! Hilos vigilados. El cuerpo va dentro de `catch_unwind`: un pánico se
//! queda en ese hilo (el hook de `log.rs` ya lo apunta con archivo:línea) y,
//! si es un vigilante periódico, el hilo se relanza tras una espera en vez de
//! desaparecer en silencio. Junto con el candado tolerante de `state.rs`, es
//! lo que garantiza que ningún hilo secundario pueda tumbar la ventana.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::thread::JoinHandle;
use std::time::Duration;

/// Qué hacer cuando el cuerpo entra en pánico.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnPanic {
    /// El hilo muere y no vuelve (hilos que poseen sockets o recursos únicos,
    /// o trabajos de una sola vez).
    Stop,
    /// Se relanza tras `after`, como mucho `max` veces (vigilantes periódicos:
    /// lo que necesiten lo vuelven a crear ellos al empezar).
    Restart { after: Duration, max: u32 },
}

/// Como `thread::Builder::spawn`, pero con el cuerpo en `catch_unwind` y la
/// política `on_panic`. `body` es `Fn` para poder llamarlo otra vez: lo que
/// necesite (Arc, sockets) lo captura por referencia o lo clona dentro.
pub fn spawn_guarded<F>(name: &'static str, on_panic: OnPanic, body: F) -> std::io::Result<JoinHandle<()>>
where
    F: Fn() + Send + 'static,
{
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || run_guarded(name, on_panic, body, std::thread::sleep, |m| crate::log_line!("{m}")))
}

/// Trabajo de una sola vez con nombre (para que el hook de pánico no diga
/// «hilo «?»»): un pánico queda en el log y el hilo termina.
pub fn spawn_once<F>(name: &'static str, body: F) -> std::io::Result<JoinHandle<()>>
where
    F: FnOnce() + Send + 'static,
{
    std::thread::Builder::new().name(name.into()).spawn(move || {
        if catch_unwind(AssertUnwindSafe(body)).is_err() {
            crate::log_line!("Hilo «{name}»: terminado por un pánico (detalles arriba)");
        }
    })
}

/// Bucle del hilo, separado para probarlo sin dormir ni escribir en el log.
fn run_guarded<F, S, L>(name: &str, on_panic: OnPanic, body: F, sleep: S, log: L)
where
    F: Fn(),
    S: Fn(Duration),
    L: Fn(String),
{
    let mut restarts = 0u32;
    loop {
        if catch_unwind(AssertUnwindSafe(&body)).is_ok() {
            return; // terminó por las buenas
        }
        match on_panic {
            OnPanic::Stop => {
                log(format!("Hilo «{name}»: terminado por un pánico (detalles arriba)"));
                return;
            }
            OnPanic::Restart { after, max } => {
                if restarts >= max {
                    log(format!("Hilo «{name}»: {max} pánicos seguidos, se abandona"));
                    return;
                }
                restarts += 1;
                log(format!(
                    "Hilo «{name}»: pánico, se relanza en {:.0} s ({restarts}/{max})",
                    after.as_secs_f32()
                ));
                sleep(after);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// Cuerpo que entra en pánico las `fails` primeras veces y luego termina.
    fn flaky(fails: u32) -> (Arc<AtomicU32>, impl Fn()) {
        let n = Arc::new(AtomicU32::new(0));
        let body = {
            let n = n.clone();
            move || {
                if n.fetch_add(1, Ordering::SeqCst) < fails {
                    panic!("boom");
                }
            }
        };
        (n, body)
    }

    #[test]
    fn relanza_tras_cada_panico_y_para_cuando_termina_bien() {
        let _quiet = crate::log::quiet_panics();
        let (n, body) = flaky(2);
        let slept = Cell::new(0u32);
        let lines = RefCell::new(Vec::new());
        run_guarded(
            "t",
            OnPanic::Restart { after: Duration::from_millis(7), max: 5 },
            body,
            |d| {
                assert_eq!(d, Duration::from_millis(7));
                slept.set(slept.get() + 1);
            },
            |m| lines.borrow_mut().push(m),
        );
        assert_eq!(n.load(Ordering::SeqCst), 3, "dos pánicos + la buena");
        assert_eq!(slept.get(), 2);
        assert_eq!(lines.borrow().len(), 2);
        assert!(lines.borrow()[0].contains("(1/5)"));
    }

    #[test]
    fn con_tope_se_abandona_y_con_stop_no_se_relanza() {
        let _quiet = crate::log::quiet_panics();
        let (n, body) = flaky(99);
        let lines = RefCell::new(Vec::new());
        run_guarded("t", OnPanic::Restart { after: Duration::ZERO, max: 2 }, body, |_| {}, |m| {
            lines.borrow_mut().push(m)
        });
        assert_eq!(n.load(Ordering::SeqCst), 3, "la primera + dos relanzamientos");
        assert!(lines.borrow().last().unwrap().contains("se abandona"));

        let (n, body) = flaky(99);
        run_guarded("t", OnPanic::Stop, body, |_| panic!("no debe dormir"), |_| {});
        assert_eq!(n.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn spawn_once_sobrevive_al_panico_y_lleva_nombre() {
        let _quiet = crate::log::quiet_panics();
        let seen = Arc::new(std::sync::Mutex::new(String::new()));
        let h = {
            let seen = seen.clone();
            spawn_once("prueba-once", move || {
                *seen.lock().unwrap() = std::thread::current().name().unwrap_or("?").to_owned();
                panic!("boom");
            })
            .unwrap()
        };
        assert!(h.join().is_ok(), "el pánico queda dentro del hilo");
        assert_eq!(*seen.lock().unwrap(), "prueba-once");
    }
}
