//! Escuchar los motores del mando virtual sin perder ningún aviso (Windows).
//!
//! El driver ViGEmBus avisa de cada escritura del juego en los motores
//! completando una petición que el receptor le deja esperando
//! (`IOCTL_XUSB_REQUEST_NOTIFICATION`). El crate `vigem-client` deja UNA sola
//! y la repite después de entregar el aviso, y eso pierde el último «apaga»:
//!
//! - Dolphin escribe cada cambio del motor en DOS llamadas seguidas (el perfil
//!   liga `Motor L | Motor R` y su expresión pone primero uno y luego el
//!   otro): apagar es `{0,FF}` y, microsegundos después, `{0,0}`.
//! - ViGEmBus anterior a 1.17.333 (el 1.16 que traen BetterJoy o x360ce)
//!   TIRA el aviso si no hay petición esperando: el receptor se quedaba con
//!   `{0,FF}` y mandaba «vibra» al móvil para siempre desde la primera
//!   vibración (nefarius/ViGEmBus#68).
//! - Desde 1.17.333 el driver guarda el aviso en una cola, pero entre «no hay
//!   petición» y «lo encolo» hay un hueco: si la petición nueva llega justo
//!   ahí, el aviso se queda en la cola hasta la siguiente vibración.
//!
//! Con [`IN_FLIGHT`] peticiones esperando siempre hay una para cada escritura,
//! con cualquier versión del driver. Es lo que hacía el SDK de ViGEm en la
//! 1.16 y lo que recuperó DS4Windows 2.2.7 para cerrar ese mismo fallo. El
//! driver completa las peticiones en el orden en que le llegaron (una cola
//! manual por mando), así que recorrerlas en anillo entrega los avisos en
//! orden. Vive aparte y sin nada de Windows para probar el orden y el modelo
//! del driver en los tres sistemas.

/// Peticiones de aviso esperando en el driver por mando. DS4Windows usa 6 y
/// dice que el mínimo son 4; Dolphin manda dos por cambio.
pub(crate) const IN_FLIGHT: usize = 8;

/// Una petición de aviso, vista desde el anillo.
pub(crate) trait Request {
    /// La deja esperando en el driver. `Err` = el driver la rechazó en el acto:
    /// esa petición no se completará nunca y esperarla colgaría el hilo.
    fn request(&mut self) -> Result<(), String>;
    /// Espera a que el driver la complete: `(motor grande, motor pequeño)`.
    fn wait(&mut self) -> Result<(u8, u8), Ending>;
}

/// Por qué terminó el escuchador.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Ending {
    /// El mando se desenchufó: el driver aborta todas las peticiones.
    Unplugged,
    /// Cualquier otra cosa (el detalle va al log).
    Failed(String),
}

/// Lanza todas las peticiones y entrega los avisos en orden hasta que el mando
/// se desenchufe o el driver falle. Cada petición completada vuelve a la cola
/// ANTES de entregar su aviso: mientras se entrega, las demás siguen
/// esperando. Las peticiones las lanza SIEMPRE el hilo que llama: Windows
/// cancela la E/S de un hilo cuando ese hilo termina.
pub(crate) fn serve<R: Request>(ring: &mut [R], mut deliver: impl FnMut(u8, u8)) -> Ending {
    if ring.is_empty() {
        return Ending::Failed("sin peticiones".to_owned());
    }
    for r in ring.iter_mut() {
        if let Err(e) = r.request() {
            return Ending::Failed(e);
        }
    }
    let mut i = 0;
    loop {
        let (grande, pequeno) = match ring[i].wait() {
            Ok(v) => v,
            Err(end) => return end,
        };
        let again = ring[i].request();
        deliver(grande, pequeno);
        if let Err(e) = again {
            return Ending::Failed(e);
        }
        i = (i + 1) % ring.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// Qué hace el driver con una escritura que no encuentra petición.
    #[derive(Clone, Copy, PartialEq)]
    enum Bus {
        /// ViGEmBus hasta 1.17.306: la tira.
        V116,
        /// ViGEmBus 1.17.333 en adelante: la encola y la entrega, en orden,
        /// a las siguientes peticiones.
        V117,
    }

    /// Modelo del driver: las peticiones esperan en una cola FIFO y cada
    /// escritura completa la más vieja con el estado nuevo.
    struct Driver {
        bus: Bus,
        pending: VecDeque<usize>,
        done: Vec<Option<(u8, u8)>>,
        queued: VecDeque<(u8, u8)>,
        /// Ráfagas de escrituras: cada una llega ENTERA antes de que el
        /// escuchador vuelva a correr (lo peor que puede pasar).
        script: VecDeque<Vec<(u8, u8)>>,
        /// Peticiones que se rechazan en el acto a partir de esta.
        reject_from: Option<usize>,
        requests: usize,
    }

    impl Driver {
        fn new(bus: Bus, n: usize, script: Vec<Vec<(u8, u8)>>) -> Rc<RefCell<Driver>> {
            Rc::new(RefCell::new(Driver {
                bus,
                pending: VecDeque::new(),
                done: vec![None; n],
                queued: VecDeque::new(),
                script: script.into(),
                reject_from: None,
                requests: 0,
            }))
        }

        fn write(&mut self, v: (u8, u8)) {
            if let Some(i) = self.pending.pop_front() {
                self.done[i] = Some(v);
            } else if self.bus == Bus::V117 {
                self.queued.push_back(v);
            }
        }

        /// Peticiones que el escuchador tiene «en el driver» (esperando o ya
        /// completadas sin recoger).
        fn outstanding(&self) -> usize {
            self.pending.len() + self.done.iter().filter(|d| d.is_some()).count()
        }
    }

    struct Fake {
        i: usize,
        d: Rc<RefCell<Driver>>,
    }

    impl Request for Fake {
        fn request(&mut self) -> Result<(), String> {
            let mut d = self.d.borrow_mut();
            d.requests += 1;
            if d.reject_from.is_some_and(|n| d.requests >= n) {
                return Err("rechazada".to_owned());
            }
            match d.queued.pop_front() {
                Some(v) => d.done[self.i] = Some(v),
                None => d.pending.push_back(self.i),
            }
            Ok(())
        }

        fn wait(&mut self) -> Result<(u8, u8), Ending> {
            let mut d = self.d.borrow_mut();
            loop {
                if let Some(v) = d.done[self.i].take() {
                    return Ok(v);
                }
                // Nada más que escribir: el mando se desenchufa
                let Some(burst) = d.script.pop_front() else { return Err(Ending::Unplugged) };
                for v in burst {
                    d.write(v);
                }
            }
        }
    }

    fn ring(d: &Rc<RefCell<Driver>>, n: usize) -> Vec<Fake> {
        (0..n).map(|i| Fake { i, d: d.clone() }).collect()
    }

    /// Lo que escribe Dolphin: cada cambio son dos llamadas seguidas, primero
    /// el motor L y luego el R.
    fn dolphin(pulsos: usize) -> Vec<Vec<(u8, u8)>> {
        let mut out = Vec::new();
        for _ in 0..pulsos {
            out.push(vec![(255, 0), (255, 255)]);
            out.push(vec![(0, 255), (0, 0)]);
        }
        out
    }

    fn run(bus: Bus, n: usize, script: Vec<Vec<(u8, u8)>>) -> Vec<(u8, u8)> {
        let d = Driver::new(bus, n, script);
        let mut r = ring(&d, n);
        let mut got = Vec::new();
        assert_eq!(serve(&mut r, |a, b| got.push((a, b))), Ending::Unplugged);
        got
    }

    /// El fallo tal cual: con una sola petición y un ViGEmBus 1.16, el
    /// segundo aviso de cada cambio se pierde y lo último que queda es
    /// `{0,FF}`: el móvil vibraría para siempre.
    #[test]
    fn con_una_sola_peticion_un_vigembus_viejo_se_queda_vibrando() {
        let got = run(Bus::V116, 1, dolphin(3));
        assert_eq!(got.last(), Some(&(0, 255)), "el «apaga» completo nunca llega: {got:?}");
    }

    /// Lo mismo con el anillo: llegan todos, en orden, y lo último es el cero.
    #[test]
    fn con_el_anillo_llegan_todos_los_avisos_y_en_orden() {
        let esperado: Vec<(u8, u8)> = dolphin(3).into_iter().flatten().collect();
        for bus in [Bus::V116, Bus::V117] {
            let got = run(bus, IN_FLIGHT, dolphin(3));
            assert_eq!(got, esperado, "todos y en orden");
        }
        // Con el driver nuevo una sola petición también bastaba sin carrera:
        // el anillo no cambia nada ahí
        assert_eq!(run(Bus::V117, 1, dolphin(3)), esperado);
    }

    /// Una ráfaga más larga que el anillo tampoco se pierde con el driver
    /// nuevo (los encola), y con el viejo llega hasta donde hay peticiones.
    #[test]
    fn una_rafaga_de_muchas_escrituras() {
        let rafaga: Vec<(u8, u8)> = (1..=20u8).map(|v| (v, v)).collect();
        let got = run(Bus::V117, IN_FLIGHT, vec![rafaga.clone()]);
        assert_eq!(got, rafaga);
        let got = run(Bus::V116, IN_FLIGHT, vec![rafaga[..IN_FLIGHT].to_vec()]);
        assert_eq!(got, rafaga[..IN_FLIGHT].to_vec());
    }

    /// Mientras se entrega un aviso, todas las peticiones están otra vez en
    /// el driver: la que acaba de completarse vuelve a la cola ANTES de
    /// entregar.
    #[test]
    fn al_entregar_el_anillo_esta_lleno() {
        let d = Driver::new(Bus::V116, IN_FLIGHT, dolphin(2));
        let mut r = ring(&d, IN_FLIGHT);
        let mut vistos = 0;
        let end = serve(&mut r, |_, _| {
            assert_eq!(d.borrow().outstanding(), IN_FLIGHT, "ninguna petición fuera del driver");
            vistos += 1;
        });
        assert_eq!(end, Ending::Unplugged);
        assert_eq!(vistos, 8);
    }

    /// Una petición que el driver rechaza en el acto no se completa nunca:
    /// esperarla colgaría el hilo. El escuchador termina (y lo ya recibido
    /// se entrega).
    #[test]
    fn una_peticion_rechazada_termina_sin_colgarse() {
        let d = Driver::new(Bus::V116, IN_FLIGHT, dolphin(2));
        d.borrow_mut().reject_from = Some(3);
        let mut r = ring(&d, IN_FLIGHT);
        assert_eq!(serve(&mut r, |_, _| {}), Ending::Failed("rechazada".to_owned()), "ya al lanzarlas");

        let d = Driver::new(Bus::V116, 2, dolphin(2));
        d.borrow_mut().reject_from = Some(3);
        let mut r = ring(&d, 2);
        let mut got = Vec::new();
        let end = serve(&mut r, |a, b| got.push((a, b)));
        assert_eq!(end, Ending::Failed("rechazada".to_owned()), "al repetirla");
        assert_eq!(got, vec![(255, 0)], "lo que llegó antes del rechazo se entrega");
    }

    #[test]
    fn sin_peticiones_no_hay_escuchador() {
        let mut r: Vec<Fake> = Vec::new();
        assert!(matches!(serve(&mut r, |_, _| {}), Ending::Failed(_)));
    }
}
