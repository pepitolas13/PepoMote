//! Vibración de los juegos (1.12). El protocolo DSU no transporta
//! vibración: ni el cliente de Dolphin ni el de Cemu la mandan nunca. Así
//! que el receptor crea un MANDO VIRTUAL por jugador (Windows: Xbox 360 por
//! el driver ViGEmBus; Linux: gamepad uinput con force feedback), lo liga en
//! el perfil del emulador (`Rumble/Motor` en Dolphin, un `<controller>` más
//! en Cemu; RetroArch lo coge solo por el puerto del jugador) y lo que el
//! juego le manda a ese mando viaja al móvil por UDP (`TYPE_RUMBLE`,
//! PROTOCOL.md §4.5). Además se atiende la extensión «rumble» no oficial del
//! DSU (protocol/DSU.md) para los clientes que la hablen. macOS no tiene
//! mando virtual sin driver: ahí solo queda la extensión.
//!
//! El envío es por ESTADO: cada cambio sale al momento y se repite cada
//! 100 ms mientras haya vibración (y medio segundo más tras parar, por si se
//! pierde el datagrama de parada); el móvil se para solo si no le llega
//! nada en `TTL_MS`. Un mando de Wii solo tiene un motor: el móvil vibra con
//! el mayor de los dos que manda un mando XInput.
//!
//! Con el driver solo habla el hilo del hub, y ni él espera: cada llamada
//! (sondear el bus, enchufar un mando, desenchufarlo) corre en un hilo de un
//! solo uso ([`Pending`]) que el hub mira en cada tic. La ventana, el `hello`
//! del móvil, la autoconfiguración y `--diag` leen una foto ([`View`]) que el
//! hub publica: no cogen el candado de los mandos. En la 1.12 el sondeo del
//! driver iba en el hilo de la ventana, en su primer fotograma, y un
//! ViGEmBus que no contestaba dejaba la ventana en negro y al móvil sin `ok`.

use crate::net::MAX_PLAYERS;
use crate::state::{LockTolerant, Mode, Role, SharedState};
use crate::tr;
use std::net::UdpSocket;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;
/// El instalador del driver, dentro del exe (Windows).
#[cfg(windows)]
pub mod vigem_setup;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(any(windows, target_os = "linux")))]
mod none;
#[cfg(not(any(windows, target_os = "linux")))]
use none as platform;

/// Tiempo que el móvil espera un refresco antes de pararse solo.
pub const TTL_MS: u16 = 400;
/// Cadencia de reenvío mientras hay vibración.
pub const REFRESH: Duration = Duration::from_millis(100);
/// Tras ponerse a cero se sigue reenviando este tiempo.
pub const ZERO_TAIL: Duration = Duration::from_millis(500);
/// Extensión DSU: sin paquetes de vibración en este tiempo el slot vuelve a
/// cero (la spec recomienda el timeout de cliente, unos 5 s).
pub const DSU_TIMEOUT: Duration = Duration::from_secs(5);
/// Con el driver o /dev/uinput ausentes se vuelve a probar cada tanto: al
/// instalarlo no hace falta reiniciar nada.
const PROBE_EVERY: Duration = Duration::from_secs(5);
/// Tope del reintento tras fallos seguidos al crear un mando virtual.
const RETRY_MAX: Duration = Duration::from_secs(60);
/// Cadencia del tic del hub (refresco de la vibración y sondeo de las
/// llamadas al driver en marcha).
const TICK: Duration = Duration::from_millis(20);
/// Un sondeo del driver que tarde más de esto se apunta en el log y la
/// ventana lo enseña como «no contesta». Lo normal (SetupAPI + un IOCTL) son
/// menos de 50 ms.
const PROBE_SLOW: Duration = Duration::from_secs(3);
/// Lo mismo para crear un mando (`plugin` + `wait_ready`; lo normal, menos
/// de 200 ms).
const CREATE_SLOW: Duration = Duration::from_secs(5);
/// Tope de espera donde no hay bucle del hub que sondee: `--diag`,
/// `--install-driver` y la comprobación tras instalar.
const ONE_SHOT_LIMIT: Duration = Duration::from_secs(5);

/// Cuánto esperar antes de volver a intentar crear un mando tras `failures`
/// fallos seguidos: 5, 10, 20, 40 s y de ahí un minuto. Un driver que falla
/// al crear cada 5 s enchufaba y desenchufaba un mando cada 5 s, y cada
/// vaivén hacía a Dolphin releer todos sus mandos (el DSU incluido).
fn retry_after(failures: u32) -> Duration {
    let veces = 1u32 << failures.min(4);
    (PROBE_EVERY * veces).min(RETRY_MAX)
}

/// Solo pruebas: con `PEPOMOTE_FAKE_DRIVER_HANG` puesto, la llamada al driver
/// se cuelga para siempre, como un ViGEmBus con un IRP atascado. Con ello se
/// reprodujo la ventana negra de la 1.12 (el sondeo del driver bloqueaba el
/// primer fotograma) y se comprueba que ya no puede pasar
/// (`desktop/e2e/smoke_gui.sh … hang`).
pub(crate) fn fake_hang_if_requested(what: &str) {
    if std::env::var_os("PEPOMOTE_FAKE_DRIVER_HANG").is_some() {
        crate::log_line!("PEPOMOTE_FAKE_DRIVER_HANG: {what} se cuelga a propósito");
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}

/// Nombre del mando virtual del jugador del slot (Dolphin en Linux lo ve
/// como `evdev/0/<nombre>`; en la lista de mandos de cualquier programa).
pub fn pad_name(slot: u8) -> String {
    format!("PepoMote Wiimote {}", slot + 1)
}

/// Identidad USB del mando virtual (bus virtual, ids neutros y fijos: así
/// el GUID que calcula SDL es siempre el mismo y se puede escribir en el
/// perfil de Cemu sin preguntarle a SDL).
pub const PAD_VENDOR: u16 = 0x5045; // "PE"
pub const PAD_PRODUCT: u16 = 0x504D; // "PM"
pub const PAD_VERSION: u16 = 0x0001;

/// De dónde viene una orden de vibración.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// El mando virtual (ViGEm / uinput).
    Pad,
    /// La extensión «rumble» del DSU.
    Dsu,
}

/// Qué puede hacer este receptor (`ok.rumble` y la ventana).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ready,
    /// Windows: falta el driver ViGEmBus.
    NeedsDriver,
    /// Linux: /dev/uinput sin permiso.
    UinputDenied,
    /// Linux: no existe /dev/uinput.
    UinputMissing,
    /// macOS: no hay mando virtual.
    Unsupported,
    /// El mando virtual falló por otra cosa.
    Failed,
    /// El primer sondeo del driver aún no ha contestado (los primeros
    /// milisegundos tras arrancar).
    Checking,
    /// El driver lleva más de [`PROBE_SLOW`] sin contestar al sondeo: un
    /// ViGEmBus a medio instalar o con una petición atascada. Es lo que en la
    /// 1.12 dejaba la ventana en negro; ahora solo se cuenta.
    Unresponsive,
}

impl Status {
    /// Valor de `ok.rumble` (PROTOCOL.md §3): el móvil enseña una línea por
    /// cada uno; los fallos de Linux van como «denied» y los de Windows como
    /// «driver» (la ventana del receptor da el detalle). «Comprobando» y «no
    /// contesta» van como un fallo: el protocolo no tiene valor para ellos y
    /// no merecen una versión nueva de las apps.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ready => "ready",
            Status::NeedsDriver => "driver",
            Status::UinputDenied | Status::UinputMissing => "denied",
            Status::Unsupported => "unsupported",
            #[cfg(windows)]
            Status::Failed | Status::Checking | Status::Unresponsive => "driver",
            #[cfg(target_os = "linux")]
            Status::Failed | Status::Checking | Status::Unresponsive => "denied",
            #[cfg(not(any(windows, target_os = "linux")))]
            Status::Failed | Status::Checking | Status::Unresponsive => "unsupported",
        }
    }
}

/// Un datagrama a mandar al móvil del slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Out {
    pub slot: u8,
    pub seq: u32,
    pub strong: u8,
    pub weak: u8,
    pub ttl_ms: u16,
}

#[derive(Clone, Copy, Default, Debug)]
struct SlotState {
    pad: (u8, u8),
    dsu: (u8, u8),
    dsu_at: Option<Instant>,
    /// Estado combinado vigente (lo último que se mandó o se va a mandar).
    strong: u8,
    weak: u8,
    /// Crece con CADA datagrama: el móvil ignora los repetidos, y un
    /// refresco tiene que contar para alargar su caducidad.
    seq: u32,
    changed_at: Option<Instant>,
    last_sent: Option<Instant>,
}

/// La parte pura del envío: qué mandar y cuándo (se prueba sin red).
pub struct Track {
    slots: [SlotState; MAX_PLAYERS],
}

impl Default for Track {
    fn default() -> Self {
        Self::new()
    }
}

impl Track {
    pub fn new() -> Self {
        Track { slots: [SlotState::default(); MAX_PLAYERS] }
    }

    fn combined(s: &SlotState) -> (u8, u8) {
        (s.pad.0.max(s.dsu.0), s.pad.1.max(s.dsu.1))
    }

    fn emit(s: &mut SlotState, slot: usize, now: Instant) -> Out {
        s.seq = s.seq.wrapping_add(1);
        s.last_sent = Some(now);
        Out {
            slot: slot as u8,
            seq: s.seq,
            strong: s.strong,
            weak: s.weak,
            ttl_ms: if s.strong == 0 && s.weak == 0 { 0 } else { TTL_MS },
        }
    }

    /// Recalcula el estado combinado; si cambió, sale un datagrama ya.
    fn recompute(s: &mut SlotState, slot: usize, now: Instant) -> Option<Out> {
        let (strong, weak) = Self::combined(s);
        if (strong, weak) == (s.strong, s.weak) {
            return None;
        }
        s.strong = strong;
        s.weak = weak;
        s.changed_at = Some(now);
        Some(Self::emit(s, slot, now))
    }

    /// Nueva orden de una fuente para el slot.
    pub fn set(&mut self, slot: usize, source: Source, strong: u8, weak: u8, now: Instant) -> Option<Out> {
        let s = self.slots.get_mut(slot)?;
        match source {
            Source::Pad => s.pad = (strong, weak),
            Source::Dsu => {
                s.dsu = (strong, weak);
                s.dsu_at = Some(now);
            }
        }
        Self::recompute(s, slot, now)
    }

    /// Lo que toca reenviar ahora: refrescos de la vibración viva, la cola
    /// de ceros tras parar y los slots DSU caducados.
    pub fn due(&mut self, now: Instant) -> Vec<Out> {
        let mut out = Vec::new();
        for (slot, s) in self.slots.iter_mut().enumerate() {
            if s.dsu != (0, 0) && s.dsu_at.is_some_and(|t| now.duration_since(t) >= DSU_TIMEOUT) {
                s.dsu = (0, 0);
                if let Some(o) = Self::recompute(s, slot, now) {
                    out.push(o);
                    continue;
                }
            }
            let fresh = s.last_sent.is_some_and(|t| now.duration_since(t) < REFRESH);
            if fresh {
                continue;
            }
            let alive = s.strong != 0 || s.weak != 0;
            let tail = s.changed_at.is_some_and(|t| now.duration_since(t) < ZERO_TAIL);
            if alive || tail {
                out.push(Self::emit(s, slot, now));
            }
        }
        out
    }

    /// Estado combinado vigente del slot (para la ventana).
    pub fn level(&self, slot: usize) -> (u8, u8) {
        self.slots.get(slot).map(|s| (s.strong, s.weak)).unwrap_or((0, 0))
    }
}

/// Una llamada al driver en marcha en su propio hilo. El hub la mira con
/// [`Pending::poll`] en cada tic, sin esperar nunca: si el driver no
/// contesta (un ViGEmBus a medio instalar, una petición atascada en el
/// núcleo), lo único que se queda colgado es ese hilo; ni la ventana ni los
/// móviles notan nada más que el aviso. [`Pending::wait`] es para donde no
/// hay bucle que sondee.
struct Pending<T> {
    /// Qué es («el sondeo del driver», «la creación del mando del jugador 2»).
    what: String,
    since: Instant,
    slow_after: Duration,
    /// Ya se apuntó en el log que no contesta.
    warned: bool,
    rx: Receiver<T>,
}

impl<T: Send + 'static> Pending<T> {
    fn start(thread: &'static str, what: String, slow_after: Duration, f: impl FnOnce() -> T + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let _ = crate::threads::spawn_once(thread, move || {
            let _ = tx.send(f());
        });
        Pending { what, since: Instant::now(), slow_after, warned: false, rx }
    }

    /// Lleva más de `slow_after` sin contestar.
    fn slow(&self) -> bool {
        self.since.elapsed() >= self.slow_after
    }

    /// `None` = sigue; `Some(Err(()))` = el hilo murió sin contestar (un
    /// pánico, o no se pudo crear); `Some(Ok(v))` = terminó. Al pasar
    /// `slow_after` sin respuesta se apunta una vez en el log.
    fn poll(&mut self) -> Option<Result<T, ()>> {
        match self.rx.try_recv() {
            Ok(v) => Some(Ok(v)),
            Err(TryRecvError::Disconnected) => Some(Err(())),
            Err(TryRecvError::Empty) => {
                if !self.warned && self.slow() {
                    self.warned = true;
                    crate::log_line!(
                        "Mando virtual: {} no contesta a los {} s; se deja en segundo plano",
                        self.what,
                        self.slow_after.as_secs()
                    );
                }
                None
            }
        }
    }

    /// Espera con tope. Solo para `--diag`, `--install-driver` y la
    /// comprobación tras instalar: sin bucle del hub que sondee. Al agotarlo,
    /// `None`; el hilo queda huérfano y, si algún día contesta, se tira.
    fn wait(self, limit: Duration) -> Option<T> {
        match self.rx.recv_timeout(limit) {
            Ok(v) => Some(v),
            Err(RecvTimeoutError::Timeout) => {
                crate::log_line!(
                    "Mando virtual: {} no contesta a los {} s; se abandona (el hilo queda en segundo plano)",
                    self.what,
                    limit.as_secs()
                );
                None
            }
            Err(RecvTimeoutError::Disconnected) => {
                crate::log_line!("Mando virtual: el hilo de {} murió sin contestar", self.what);
                None
            }
        }
    }

    fn in_flight(&self) -> InFlight {
        InFlight { what: self.what.clone(), since: self.since, slow_after: self.slow_after }
    }
}

/// Lo que la ventana y `--diag` saben de la llamada al driver en marcha.
#[derive(Clone, Debug)]
struct InFlight {
    what: String,
    since: Instant,
    slow_after: Duration,
}

impl InFlight {
    fn slow(&self) -> bool {
        self.since.elapsed() >= self.slow_after
    }
}

/// La llamada al driver en marcha: una como mucho.
enum Job {
    Probe(Pending<Result<platform::Backend, Status>>),
    Create { slot: usize, pending: Pending<Result<platform::Pad, String>> },
}

impl Job {
    fn in_flight(&self) -> InFlight {
        match self {
            Job::Probe(p) => p.in_flight(),
            Job::Create { pending, .. } => pending.in_flight(),
        }
    }
}

/// Lo que una llamada terminada trae de vuelta.
enum Done {
    Probe(Result<Result<platform::Backend, Status>, ()>),
    Create(usize, Result<Result<platform::Pad, String>, ()>),
}

/// Mandos virtuales: qué jugadores lo quieren y cuáles existen. Lo tocan el
/// hub y `push_pad` (telemetría); la ventana y el canal de control leen
/// [`View`], nunca esto: `push_pad` escribe en el mando con un IOCTL, y con
/// este candado cogido nadie más puede esperar detrás.
struct Pads {
    /// En `Arc`: la creación de un mando se lleva un clon a su hilo.
    backend: Option<Arc<platform::Backend>>,
    backend_err: Option<Status>,
    probed_at: Option<Instant>,
    /// Fallos seguidos al crear un mando: espacia los reintentos
    /// ([`retry_after`]); a cero en cuanto uno sale bien.
    create_failures: u32,
    wanted: [bool; MAX_PLAYERS],
    pads: [Option<platform::Pad>; MAX_PLAYERS],
    /// La llamada al driver en marcha, si la hay.
    job: Option<Job>,
    /// Tras un `sync` con cambios se crea lo que falte sin esperar la
    /// cadencia; se apaga al no faltar nada o al fallar una creación.
    creating: bool,
    last_create_done: Option<Instant>,
    /// Windows: con el primer resultado del sondeo se decide, una vez, si
    /// instalar el driver embebido.
    startup_decided: bool,
}

/// Lo que pide el hilo de control con [`sync`]; el hub lo recoge en su tic.
#[derive(Default)]
struct Wants {
    wanted: [bool; MAX_PLAYERS],
    dirty: bool,
    /// Dejar los mandos en reposo (al salir del mando universal).
    release: bool,
}

#[derive(Clone, Debug)]
struct PadView {
    index: Option<u32>,
    describe: String,
}

/// Foto que el hub publica en cada tic para la ventana, el `hello` del
/// móvil, la autoconfiguración y `--diag`: ninguno de ellos coge [`Pads`].
#[derive(Clone, Debug)]
struct View {
    status: Status,
    in_flight: Option<InFlight>,
    pads: [Option<PadView>; MAX_PLAYERS],
}

pub struct Hub {
    track: Mutex<Track>,
    socket: Mutex<Option<UdpSocket>>,
    pads: Mutex<Pads>,
    wants: Mutex<Wants>,
    view: Mutex<View>,
    shared: SharedState,
}

static HUB: OnceLock<Arc<Hub>> = OnceLock::new();

fn hub() -> Option<Arc<Hub>> {
    HUB.get().cloned()
}

/// Arranca el hilo de reenvío (y del driver). Una vez.
pub fn start(shared: SharedState) {
    let hub = Arc::new(Hub {
        track: Mutex::new(Track::new()),
        socket: Mutex::new(None),
        pads: Mutex::new(Pads {
            backend: None,
            backend_err: None,
            probed_at: None,
            create_failures: 0,
            wanted: [false; MAX_PLAYERS],
            pads: [None, None, None, None],
            job: None,
            creating: false,
            last_create_done: None,
            startup_decided: false,
        }),
        wants: Mutex::new(Wants::default()),
        view: Mutex::new(View { status: Status::Checking, in_flight: None, pads: [None, None, None, None] }),
        shared,
    });
    if HUB.set(hub.clone()).is_err() {
        return;
    }
    let _ = crate::threads::spawn_guarded(
        "pmp-rumble",
        crate::threads::OnPanic::Restart { after: Duration::from_secs(5), max: 10 },
        move || loop {
            // Nada de esto espera al driver: cada llamada va en su hilo
            // (`Pending`) y aquí solo se mira si ya contestó. El tic también
            // refresca la vibración cada 100 ms, y el móvil se para a los
            // 400 ms sin refresco: el hub no puede quedarse parado ni 3 s.
            let now = Instant::now();
            hub.poll_job();
            let (dirty, release) = hub.take_wants();
            hub.probe_if_due();
            let outs = hub.track.lock_tolerant().due(now);
            for o in outs {
                hub.send(o);
            }
            // Los mandos recién creados, a la espera de que el sistema
            // les asigne su hueco de XInput
            let pendientes = {
                let p = hub.pads.lock_tolerant();
                p.pads.iter().flatten().any(|pad| pad.pending_index())
            };
            if pendientes {
                hub.settle_indices();
            }
            hub.reconcile(dirty, release);
            hub.publish();
            std::thread::sleep(TICK);
        },
    );
}

/// El socket UDP de telemetría (el mismo por el que van los PING al móvil):
/// lo registra el hilo de telemetría al nacer.
pub fn set_socket(socket: UdpSocket) {
    if let Some(h) = hub() {
        *h.socket.lock_tolerant() = Some(socket);
    }
}

/// Orden de vibración para el slot desde una fuente (hilos de los mandos
/// virtuales y del servidor DSU).
pub fn set(slot: u8, source: Source, strong: u8, weak: u8) {
    let Some(h) = hub() else { return };
    let out = h.track.lock_tolerant().set(slot as usize, source, strong, weak, Instant::now());
    if let Some(o) = out {
        h.send(o);
    }
}

/// Extensión DSU: un motor, intensidad 0..255.
pub fn set_from_dsu(slot: u8, intensity: u8) {
    set(slot, Source::Dsu, intensity, intensity);
}

/// Los mandos retirados se destruyen en un hilo aparte: en Windows el `Drop`
/// desenchufa con un IOCTL que espera sin límite, y eso no puede correr ni
/// con el candado de los mandos cogido ni en el hilo del hub.
fn reap_pads(pads: Vec<platform::Pad>) {
    if pads.is_empty() {
        return;
    }
    let _ = crate::threads::spawn_once("pmp-rumble-drop", move || drop(pads));
}

/// El estado que se publica: con backend, listo; si el sondeo lleva más de
/// [`PROBE_SLOW`] sin contestar, «no contesta» (pisa el resultado anterior:
/// un re-sondeo colgado tras instalar dice más que el «falta el driver» de
/// antes); si no, el último resultado, o «comprobando» antes del primero.
fn view_status(backend: bool, last: Option<Status>, probe_slow: bool) -> Status {
    if backend {
        Status::Ready
    } else if probe_slow {
        Status::Unresponsive
    } else {
        last.unwrap_or(Status::Checking)
    }
}

impl Hub {
    fn send(&self, o: Out) {
        let Some(sessions) = crate::net::sessions() else { return };
        let targets: Vec<(u32, std::net::SocketAddr)> = sessions
            .lock_tolerant()
            .values()
            .filter(|s| s.slot == o.slot)
            .filter_map(|s| s.phone_udp.map(|a| (s.id, a)))
            .collect();
        if targets.is_empty() {
            return;
        }
        let sock = self.socket.lock_tolerant();
        let Some(sock) = sock.as_ref() else { return };
        for (id, addr) in targets {
            let pkt = pmp::build_rumble(&pmp::RumblePacket {
                session_id: id,
                seq: o.seq,
                strong: o.strong,
                weak: o.weak,
                ttl_ms: o.ttl_ms,
            });
            let _ = sock.send_to(&pkt, addr);
        }
    }

    /// Recoge lo que pidió [`sync`]: `(hubo cambios, dejar en reposo)`.
    fn take_wants(&self) -> (bool, bool) {
        let (wanted, dirty, release) = {
            let mut w = self.wants.lock_tolerant();
            let out = (w.wanted, w.dirty, w.release);
            w.dirty = false;
            w.release = false;
            out
        };
        if dirty {
            let mut p = self.pads.lock_tolerant();
            p.wanted = wanted;
            p.creating = true;
        }
        (dirty, release)
    }

    /// Pone en marcha el sondeo del driver (o de /dev/uinput) si toca: al
    /// arrancar, y cada [`PROBE_EVERY`] mientras no haya backend (más si
    /// crear falla seguido, [`retry_after`]). El sondeo corre en su hilo;
    /// [`Hub::poll_job`] recoge el resultado.
    fn probe_if_due(&self) {
        let mut p = self.pads.lock_tolerant();
        if p.backend.is_some() || p.job.is_some() {
            return;
        }
        let due = p.probed_at.is_none_or(|t| t.elapsed() >= retry_after(p.create_failures));
        if !due {
            return;
        }
        p.probed_at = Some(Instant::now());
        p.job = Some(Job::Probe(Pending::start(
            "pmp-rumble-probe",
            "el sondeo del driver".to_owned(),
            PROBE_SLOW,
            platform::Backend::probe,
        )));
    }

    /// Mira si la llamada al driver en marcha ya contestó y aplica el
    /// resultado. Sin esperar: si no ha contestado, vuelve.
    fn poll_job(&self) {
        let mut reconfigure = false;
        let mut startup: Option<Status> = None;
        let mut retired: Vec<platform::Pad> = Vec::new();
        {
            let mut p = self.pads.lock_tolerant();
            let mut first_warning = false;
            let done = match p.job.as_mut() {
                None => return,
                Some(Job::Probe(pending)) => {
                    let was = pending.warned;
                    let r = pending.poll();
                    first_warning = pending.warned && !was;
                    r.map(Done::Probe)
                }
                Some(Job::Create { slot, pending }) => {
                    let slot = *slot;
                    pending.poll().map(|r| Done::Create(slot, r))
                }
            };
            let Some(done) = done else {
                if first_warning && !p.startup_decided {
                    crate::log_line!(
                        "Mando virtual: el sondeo del driver no contesta; no se instala solo (queda el botón de la ventana)"
                    );
                }
                return;
            };
            p.job = None;
            match done {
                Done::Probe(result) => {
                    let st = match result {
                        Ok(Ok(b)) => {
                            p.backend = Some(Arc::new(b));
                            p.backend_err = None;
                            Status::Ready
                        }
                        Ok(Err(st)) => {
                            p.backend_err = Some(st);
                            st
                        }
                        Err(()) => {
                            crate::log_line!("Mando virtual: el hilo del sondeo del driver murió sin contestar");
                            p.backend_err = Some(Status::Failed);
                            Status::Failed
                        }
                    };
                    // `None` aquí = el instalador pidió volver a sondear
                    // mientras este iba en vuelo: se respeta y el tic
                    // siguiente sondea otra vez
                    if p.probed_at.is_some() {
                        p.probed_at = Some(Instant::now());
                    }
                    if !p.startup_decided {
                        p.startup_decided = true;
                        startup = Some(st);
                    }
                }
                Done::Create(slot, result) => {
                    p.last_create_done = Some(Instant::now());
                    match result.unwrap_or_else(|()| Err("el hilo de creación murió sin contestar".to_owned())) {
                        Ok(pad) => {
                            if p.wanted[slot] && p.pads[slot].is_none() {
                                crate::log_line!("Vibración: mando virtual del jugador {} creado ({})", slot + 1, pad.describe());
                                // Con hueco de XInput ya conocido (Linux:
                                // siempre) el perfil puede llevar su motor;
                                // si aún no, `settle_indices` reescribe al
                                // averiguarlo
                                if !pad.pending_index() {
                                    reconfigure = true;
                                }
                                p.pads[slot] = Some(pad);
                                p.create_failures = 0;
                            } else {
                                crate::log_line!(
                                    "Vibración: mando virtual del jugador {} creado cuando ya no se quería; se retira",
                                    slot + 1
                                );
                                retired.push(pad);
                            }
                        }
                        Err(e) => {
                            p.create_failures = p.create_failures.saturating_add(1);
                            crate::log_line!(
                                "Vibración: no se pudo crear el mando virtual del jugador {}: {e} (se reintenta en {} s)",
                                slot + 1,
                                retry_after(p.create_failures).as_secs()
                            );
                            p.backend_err = Some(Status::Failed);
                            // ViGEm caído o /dev/uinput cerrado: se vuelve a sondear
                            p.backend = None;
                            p.probed_at = Some(Instant::now());
                            p.creating = false;
                        }
                    }
                }
            }
        }
        reap_pads(retired);
        if reconfigure {
            // la autoconfiguración lee la foto: que lleve ya el mando nuevo
            self.publish();
            crate::dolphin::maybe_auto_configure(&self.shared);
            crate::cemu::maybe_auto_configure(&self.shared);
        }
        if let Some(st) = startup {
            self.decide_startup_setup(st);
        }
    }

    /// Cuadra los mandos con `wanted`: retira los que ya no se quieren (se
    /// destruyen en otro hilo), pone en marcha la creación del primero que
    /// falte (una a la vez, en su hilo; los perfiles de los emuladores se
    /// reescriben cuando nace con identidad) y, si se pidió, deja los mandos
    /// en reposo. Nada de esto espera al driver.
    fn reconcile(&self, dirty: bool, release: bool) {
        let mut retired: Vec<platform::Pad> = Vec::new();
        let mut zero: Vec<u8> = Vec::new();
        {
            let mut p = self.pads.lock_tolerant();
            for slot in 0..MAX_PLAYERS {
                if p.wanted[slot] {
                    continue;
                }
                if let Some(pad) = p.pads[slot].take() {
                    crate::log_line!("Vibración: mando virtual del jugador {} retirado", slot + 1);
                    retired.push(pad);
                }
            }
            if dirty || !retired.is_empty() {
                zero = (0..MAX_PLAYERS).filter(|s| !p.wanted[*s]).map(|s| s as u8).collect();
            }
            if p.job.is_none() {
                if let Some(backend) = p.backend.clone() {
                    match (0..MAX_PLAYERS).find(|s| p.wanted[*s] && p.pads[*s].is_none()) {
                        None => p.creating = false,
                        Some(slot) => {
                            // Tras un `sync`, ya; un mando que se tiró (`push_pad`)
                            // se recrea a la cadencia de siempre
                            let due = p.creating || p.last_create_done.is_none_or(|t| t.elapsed() >= PROBE_EVERY);
                            if due {
                                let what = format!("la creación del mando del jugador {}", slot + 1);
                                p.job = Some(Job::Create {
                                    slot,
                                    pending: Pending::start("pmp-rumble-create", what, CREATE_SLOW, move || {
                                        backend.create(slot as u8)
                                    }),
                                });
                            }
                        }
                    }
                }
            }
            if release {
                // Salir del mando universal no destruye el mando (los modos
                // de emulador también lo quieren, para la vibración), pero
                // conserva lo último que se le escribió: un botón que se
                // quedó pulsado lo vería el emulador para siempre
                for pad in p.pads.iter_mut().flatten() {
                    let _ = pad.apply(&crate::pad::PadState::default());
                }
            }
        }
        reap_pads(retired);
        for slot in zero {
            set(slot, Source::Pad, 0, 0);
        }
    }

    /// Publica la foto que leen la ventana, el `hello`, la
    /// autoconfiguración y `--diag`.
    fn publish(&self) {
        let view = {
            let p = self.pads.lock_tolerant();
            let in_flight = p.job.as_ref().map(Job::in_flight);
            let probe_slow = matches!(&p.job, Some(Job::Probe(pending)) if pending.slow());
            let status = view_status(p.backend.is_some(), p.backend_err, probe_slow);
            let pads = std::array::from_fn(|i| {
                p.pads[i].as_ref().map(|pad| PadView { index: pad.xinput_index(), describe: pad.describe() })
            });
            View { status, in_flight, pads }
        };
        *self.view.lock_tolerant() = view;
    }

    /// Windows: con el primer resultado del sondeo, si falta el driver se
    /// instala el embebido (una vez por versión del instalador; instalado,
    /// cancelado o fallido, no se vuelve a preguntar solo). `PEPOMOTE_NO_DRIVER_SETUP`
    /// lo apaga (receptores de prueba). Si el sondeo no contesta no se
    /// decide nada: queda el botón de la ventana. Fuera del candado de los
    /// mandos: `run_setup` coge el estado compartido.
    fn decide_startup_setup(&self, st: Status) {
        #[cfg(windows)]
        {
            let tried = self.shared.lock_tolerant().config.vigem_setup_version.clone();
            let skip = std::env::var_os("PEPOMOTE_NO_DRIVER_SETUP").is_some();
            if vigem_setup::should_install(st, tried.as_deref(), skip) {
                crate::log_line!(
                    "Mando virtual: falta el driver; se instala el embebido (ViGEmBus {})",
                    vigem_setup::SETUP_VERSION
                );
                run_setup(self.shared.clone());
            }
        }
        #[cfg(not(windows))]
        let _ = st;
    }
}

/// ¿Existe ya el mando virtual del jugador del slot? Sin él no se escribe
/// ningún motor en los perfiles de los emuladores.
pub fn pad_exists(slot: u8) -> bool {
    hub().is_some_and(|h| h.view.lock_tolerant().pads.get(slot as usize).is_some_and(|p| p.is_some()))
}

/// Qué slots quieren mando virtual: los modos con emulador que vibra
/// (Dolphin, Cemu, RetroArch) y el mando universal, donde además de vibrar
/// es la salida del móvil. Siempre con papel de mando y sin «solo pantalla».
pub fn wanted_slots(mode: Mode, players: &[Option<crate::state::PlayerInfo>]) -> [bool; MAX_PLAYERS] {
    let mut out = [false; MAX_PLAYERS];
    let quiere = matches!(mode, Mode::Dolphin | Mode::Cemu | Mode::RetroArch | Mode::Gamepad);
    for (i, p) in players.iter().take(MAX_PLAYERS).enumerate() {
        out[i] = quiere && p.as_ref().is_some_and(|p| p.role == Role::Wiimote && !p.screen_only);
    }
    out
}

impl Hub {
    /// Un mando virtual recién enchufado todavía no sabe en qué hueco de
    /// XInput ha caído: Windows tarda unas decenas de milisegundos en
    /// enumerarlo. Esperarlo al crearlo dejaría colgado al móvil que acaba de
    /// conectarse, así que se resuelve aquí, en el tic del hub, y en cuanto
    /// se sabe el hueco se reescribe la configuración del emulador, que hasta
    /// ahora no llevaba este mando (sin hueco no se apunta a ningún XInput:
    /// podía ser el mando de verdad de otro jugador).
    fn settle_indices(&self) {
        // La foto de XInput cuesta milisegundos por hueco vacío: fuera del
        // candado, que la telemetría también lo coge
        let after = platform::xinput_connected();
        let mut reconfigure = false;
        {
            let mut p = self.pads.lock_tolerant();
            // Los huecos que ya tienen dueño, para no dárselos a dos mandos:
            // dos mandos creados seguidos pueden compartir foto previa.
            let mut taken = [false; 4];
            for pad in p.pads.iter().flatten() {
                if let Some(i) = pad.xinput_index() {
                    if (i as usize) < taken.len() {
                        taken[i as usize] = true;
                    }
                }
            }
            for slot in 0..MAX_PLAYERS {
                let Some(pad) = p.pads[slot].as_mut() else { continue };
                if !pad.settle(slot as u8, &taken, &after) {
                    continue;
                }
                if let Some(i) = pad.xinput_index() {
                    if (i as usize) < taken.len() {
                        taken[i as usize] = true;
                    }
                }
                reconfigure = true;
            }
        }
        if reconfigure {
            // la autoconfiguración lee la foto: que lleve ya el hueco recién sabido
            self.publish();
            crate::dolphin::maybe_auto_configure(&self.shared);
            crate::cemu::maybe_auto_configure(&self.shared);
        }
    }
}

/// Escribe el estado del móvil en el mando virtual del jugador (modo mando
/// universal). Si el mando deja de aceptarlo, se tira para que el hub lo
/// vuelva a crear, que es el camino de recuperación de siempre.
/// `true` = el estado llegó al mando virtual. `false` = no hay mando (todavía
/// sin driver, o se acaba de tirar): quien llama tiene que OLVIDAR lo enviado,
/// porque el `Feed` solo manda lo que cambia y el mando nuevo nace en reposo;
/// si no, un botón que sigue pulsado no volvería a salir nunca.
pub fn push_pad(slot: u8, s: &crate::pad::PadState) -> bool {
    let Some(h) = hub() else { return false };
    let (fallo, muerto) = {
        let mut p = h.pads.lock_tolerant();
        let Some(hueco) = p.pads.get_mut(slot as usize) else { return false };
        let Some(pad) = hueco.as_mut() else { return false };
        match pad.apply(s) {
            Ok(()) => return true,
            // se destruye fuera del candado y en otro hilo (desenchufar espera al driver)
            Err(e) => (e, hueco.take()),
        }
    };
    reap_pads(muerto.into_iter().collect());
    crate::log_line!("Mando universal: el jugador {} no acepta el estado ({fallo}); se recrea", slot + 1);
    // El mando ya no existe: sin esto su última vibración se seguiría
    // reenviando cada 100 ms y el móvil no pararía nunca.
    set(slot, Source::Pad, 0, 0);
    false
}

/// Cuadra los mandos virtuales con el modo y los jugadores de ahora. Se
/// llama donde se autoconfiguran los emuladores y al irse un móvil. Solo
/// deja la petición: el hub la recoge en su siguiente tic (20 ms) y crea o
/// retira los mandos en sus hilos, así que quien llama nunca espera al
/// driver. Un mando que nazca después de que la autoconfiguración haya
/// escrito los perfiles los hace reescribir (`poll_job`, `settle_indices`).
pub fn sync(shared: &SharedState) {
    let Some(h) = hub() else { return };
    let (wanted, universal) = {
        let s = shared.lock_tolerant();
        (wanted_slots(s.mode, &s.players), s.mode == Mode::Gamepad)
    };
    let mut w = h.wants.lock_tolerant();
    w.wanted = wanted;
    w.dirty = true;
    if !universal {
        w.release = true;
    }
}

/// Lo que puede este receptor ahora mismo (`ok.rumble`, ventana, --diag).
/// Es una lectura de la foto del hub: nunca sondea el driver. Sin hub
/// (tests) o antes del primer resultado, «comprobando».
pub fn status() -> Status {
    hub().map(|h| h.view.lock_tolerant().status).unwrap_or(Status::Checking)
}

/// El estado del driver sondeándolo AHORA, con tope de espera. Solo para
/// donde no hay bucle del hub que sondee: `--diag`, `--install-driver` y la
/// comprobación tras instalar. Si el driver no contesta, `Unresponsive`, y el
/// hilo queda en segundo plano.
pub fn bounded_status(limit: Duration) -> Status {
    Pending::start("pmp-rumble-probe", "el sondeo del driver".to_owned(), limit, platform::probe_status)
        .wait(limit)
        .unwrap_or(Status::Unresponsive)
}

/// Para el vigilante del primer fotograma y `--diag`: si hay una llamada al
/// driver que lleva más de su límite sin contestar, cuál y desde cuándo.
pub fn stuck_note() -> Option<String> {
    let h = hub()?;
    let f = h.view.lock_tolerant().in_flight.clone()?;
    f.slow().then(|| {
        format!(
            "el driver del mando virtual no contesta: {} en curso desde hace {} s",
            f.what,
            f.since.elapsed().as_secs()
        )
    })
}

/// Índice XInput que tiene el mando virtual del slot (Windows), si existe.
pub fn xinput_index(slot: u8) -> Option<u32> {
    hub()?.view.lock_tolerant().pads.get(slot as usize)?.as_ref()?.index
}

/// Expresión de `Rumble/Motor` del Mando de Wii emulado de Dolphin para el
/// slot: los dos motores del mando virtual. Sin mando aún (se crea al
/// conectar el móvil, el perfil puede escribirse antes) se supone el índice
/// = slot, que es lo que XInput da sin otros mandos; si luego no coincide,
/// se reescribe. None = este sistema no tiene mando virtual.
pub fn motor_expression(slot: u8) -> Option<String> {
    platform::motor_expression(slot)
}

/// Nodo `<controller>` extra del perfil de Cemu con el mando virtual del
/// slot (`<rumble>1</rumble>`: la intensidad la pone el móvil). None = este
/// sistema no tiene mando virtual.
pub fn cemu_node(slot: u8, player: u8) -> Option<String> {
    platform::cemu_node(slot, player)
}

/// Nodo `<controller>` de Cemu con la api y el uuid dados (la parte común
/// de Windows y Linux): sin botones, con vibración a tope y las zonas
/// muertas por defecto de Cemu.
pub(crate) fn cemu_node_with(api: &str, uuid: &str, player: u8) -> String {
    format!(
        "\t<controller>\n\t\t<api>{api}</api>\n\t\t<uuid>{uuid}</uuid>\n\t\t<display_name>PepoMote J{player} vibración</display_name>\n\t\t<rumble>1</rumble>\n\t\t<axis>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</axis>\n\t\t<rotation>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</rotation>\n\t\t<trigger>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</trigger>\n\t\t<mappings>\n\t\t</mappings>\n\t</controller>\n"
    )
}

/// CRC-16/ARC (polinomio reflejado 0xA001, inicio 0), el `SDL_crc16` que
/// SDL mete en el GUID de un mando a partir de su nombre.
pub fn sdl_crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

/// GUID que SDL 2/3 calcula para un dispositivo evdev: bus, CRC del nombre,
/// vendor, product y version (u16 LE cada uno, con ceros entre medias), en
/// 32 hex. Cemu en Linux guarda el mando como `0_<guid>`.
pub fn sdl_guid(bus: u16, name: &str, vendor: u16, product: u16, version: u16) -> String {
    let mut g = [0u8; 16];
    g[0..2].copy_from_slice(&bus.to_le_bytes());
    g[2..4].copy_from_slice(&sdl_crc16(name.as_bytes()).to_le_bytes());
    g[4..6].copy_from_slice(&vendor.to_le_bytes());
    g[8..10].copy_from_slice(&product.to_le_bytes());
    g[12..14].copy_from_slice(&version.to_le_bytes());
    g.iter().map(|b| format!("{b:02x}")).collect()
}

/// Líneas para la ventana: estado, mandos virtuales que hay y, si una
/// llamada al driver lleva más de la cuenta sin contestar, desde cuándo.
/// Lee la foto del hub: no coge el candado de los mandos ni sondea nada.
pub fn ui_lines() -> (Status, Vec<String>) {
    let Some(h) = hub() else { return (Status::Checking, Vec::new()) };
    let v = h.view.lock_tolerant().clone();
    let mut lines = Vec::new();
    for (slot, pad) in v.pads.iter().enumerate() {
        if let Some(pad) = pad {
            lines.push(tr!("rumble.pad", slot + 1, pad.describe));
        }
    }
    if let Some(f) = v.in_flight.as_ref().filter(|f| f.slow()) {
        lines.push(tr!("rumble.stuck_line", f.since.elapsed().as_secs()));
    }
    (v.status, lines)
}

/// Texto de la ventana para cada estado.
pub fn status_text(st: Status) -> String {
    match st {
        Status::Ready => tr!("rumble.ready"),
        Status::NeedsDriver => tr!("rumble.driver"),
        Status::UinputDenied => tr!("rumble.denied"),
        Status::UinputMissing => tr!("rumble.missing"),
        Status::Unsupported => tr!("rumble.unsupported"),
        Status::Failed => tr!("rumble.failed"),
        Status::Checking => tr!("rumble.checking"),
        Status::Unresponsive => tr!("rumble.stuck"),
    }
    .to_owned()
}

/// Estado de la instalación del driver embebido del mando virtual
/// (Windows; en el resto de sistemas siempre `Idle`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RumbleSetup {
    #[default]
    Idle,
    /// El instalador está corriendo (o esperando el permiso de Windows).
    Installing,
    Installed,
    /// El usuario canceló el permiso: no se vuelve a preguntar solo.
    Declined,
    /// El instalador terminó con este código (0 = ni llegó a lanzarse;
    /// el detalle está en receptor.log). Tampoco se vuelve a intentar solo:
    /// queda el botón de la ventana.
    Failed(u32),
}

/// ¿Se apunta este resultado en los ajustes para no repetir el intento solo
/// con esta versión del instalador? Todos los definitivos, también un fallo:
/// sin esto, un instalador que fallaba (un MSI roto, una directiva de la
/// empresa, un antivirus) sacaba la ventana de permiso de Windows en CADA
/// arranque. Solo «otra instalación en marcha» se deja para el siguiente
/// arranque: se resuelve sola.
pub fn remembers_attempt(state: RumbleSetup) -> bool {
    match state {
        RumbleSetup::Installed | RumbleSetup::Declined => true,
        RumbleSetup::Failed(code) => code != vigem_setup::INSTALL_ALREADY_RUNNING,
        RumbleSetup::Idle | RumbleSetup::Installing => false,
    }
}

/// Botón «Instalar el mando virtual» de la ventana.
pub fn install_driver_now() {
    #[cfg(windows)]
    if let Some(h) = hub() {
        run_setup(h.shared.clone());
    }
}

/// `PepoMote.exe --install-driver` (la CI): instala y sale.
pub fn install_driver_from_args() -> bool {
    #[cfg(windows)]
    {
        vigem_setup::run_from_args()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// El instalador en su hilo: el estado va a `Shared::rumble_setup` (la
/// ventana lo cuenta) y, al terminar bien, el hub vuelve a sondear el driver
/// en su siguiente tic, sin reiniciar nada.
#[cfg(windows)]
fn run_setup(shared: SharedState) {
    {
        let mut s = shared.lock_tolerant();
        if s.rumble_setup == RumbleSetup::Installing {
            return;
        }
        s.rumble_setup = RumbleSetup::Installing;
    }
    let _ = crate::threads::spawn_once("vigem-setup", move || {
        let mut result = vigem_setup::install();
        // Sobre una versión anterior del driver el instalador dice «hecho» y
        // el bus sigue siendo el viejo: hace falta un segundo pase (así lo
        // documenta la propia release). Solo si el primero no dejó driver.
        if matches!(result, Ok(vigem_setup::Outcome::Installed)) {
            match bounded_status(ONE_SHOT_LIMIT) {
                Status::Ready => {}
                Status::Unresponsive => {
                    crate::log_line!("Mando virtual: el instalador terminó pero el driver no contesta; sin segundo pase");
                }
                _ => {
                    crate::log_line!("Mando virtual: el instalador terminó pero el driver no responde; segundo pase");
                    result = vigem_setup::install();
                }
            }
        }
        let state = match &result {
            Ok(vigem_setup::Outcome::Installed) => RumbleSetup::Installed,
            Ok(vigem_setup::Outcome::Declined) => RumbleSetup::Declined,
            Ok(vigem_setup::Outcome::Failed(c)) => RumbleSetup::Failed(*c),
            Err(_) => RumbleSetup::Failed(0),
        };
        match &result {
            Ok(o) => crate::log_line!("Mando virtual: instalador terminado: {o:?}"),
            Err(e) => crate::log_line!("Mando virtual: no se pudo lanzar el instalador: {e}"),
        }
        {
            let mut s = shared.lock_tolerant();
            s.rumble_setup = state;
            // Instalado, cancelado o fallido: con esta versión del instalador
            // no se vuelve a intentar solo (queda el botón). Ver `remembers_attempt`.
            if remembers_attempt(state) {
                s.config.vigem_setup_version = Some(vigem_setup::SETUP_VERSION.to_owned());
                s.config.save();
            }
        }
        if state == RumbleSetup::Installed {
            if let Some(h) = hub() {
                // que el hub lo vea ya, sin esperar su cadencia (y si hay un
                // sondeo viejo en vuelo, `poll_job` respeta este `None`)
                h.pads.lock_tolerant().probed_at = None;
            }
        }
    });
}

/// Para `--diag`.
pub fn diag_lines() -> Vec<String> {
    // `--diag` corre antes de arrancar el hub: se sondea aquí, con tope, que
    // el informe que se le pide a quien tiene el problema no puede colgarse
    let (st, pads) = if hub().is_some() { ui_lines() } else { (bounded_status(ONE_SHOT_LIMIT), Vec::new()) };
    let mut out = vec![format!("Vibración de los juegos: {} ({:?})", st.as_str(), st)];
    out.extend(pads.into_iter().map(|l| format!("  {l}")));
    if let Some(n) = stuck_note() {
        out.push(format!("  {n}"));
    }
    #[cfg(windows)]
    {
        let tried = crate::state::Config::load().vigem_setup_version;
        let setup = hub().map(|h| h.shared.lock_tolerant().rumble_setup).unwrap_or_default();
        out.push(format!(
            "Instalador del mando virtual embebido: ViGEmBus {} · intentado en esta versión: {} · en esta sesión: {:?}",
            vigem_setup::SETUP_VERSION,
            match tried.as_deref() {
                Some(v) if v == vigem_setup::SETUP_VERSION => "sí",
                Some(_) => "con otra versión",
                None => "no",
            },
            setup
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn un_cambio_sale_al_momento_y_se_refresca_con_seq_nuevo() {
        let mut tr = Track::new();
        let t = t0();
        let o = tr.set(0, Source::Pad, 255, 128, t).expect("cambio");
        assert_eq!((o.slot, o.seq, o.strong, o.weak, o.ttl_ms), (0, 1, 255, 128, TTL_MS));
        // mismo estado: nada
        assert!(tr.set(0, Source::Pad, 255, 128, t).is_none());
        // antes de 100 ms no se refresca; después sí, con seq nuevo
        assert!(tr.due(t + Duration::from_millis(50)).is_empty());
        let r = tr.due(t + Duration::from_millis(101));
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].seq, r[0].strong, r[0].ttl_ms), (2, 255, TTL_MS));
    }

    #[test]
    fn al_parar_sale_un_cero_y_una_cola_de_medio_segundo() {
        let mut tr = Track::new();
        let t = t0();
        tr.set(0, Source::Pad, 200, 0, t);
        let o = tr.set(0, Source::Pad, 0, 0, t + Duration::from_millis(10)).expect("parada");
        assert_eq!((o.strong, o.weak, o.ttl_ms), (0, 0, 0));
        let cola = tr.due(t + Duration::from_millis(210));
        assert_eq!(cola.len(), 1, "cola de ceros");
        assert_eq!(cola[0].strong, 0);
        assert!(tr.due(t + Duration::from_millis(700)).is_empty(), "la cola se acaba");
    }

    #[test]
    fn las_fuentes_se_combinan_por_el_maximo() {
        let mut tr = Track::new();
        let t = t0();
        tr.set(1, Source::Dsu, 100, 100, t);
        let o = tr.set(1, Source::Pad, 50, 200, t).expect("cambia el débil");
        assert_eq!((o.strong, o.weak), (100, 200));
        assert!(tr.set(1, Source::Pad, 0, 200, t).is_none(), "el fuerte lo sigue dando el DSU");
        let o = tr.set(1, Source::Dsu, 0, 0, t).expect("cambia el fuerte");
        assert_eq!((o.strong, o.weak), (0, 200));
    }

    #[test]
    fn el_dsu_caduca_a_los_cinco_segundos() {
        let mut tr = Track::new();
        let t = t0();
        tr.set(2, Source::Dsu, 255, 255, t);
        let r = tr.due(t + Duration::from_secs(4));
        assert!(r.iter().all(|o| o.strong == 255), "sigue vivo: refresco");
        let r = tr.due(t + Duration::from_secs(5));
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].strong, r[0].weak, r[0].ttl_ms), (0, 0, 0), "caducado");
    }

    #[test]
    fn los_reintentos_de_crear_se_espacian_hasta_un_minuto() {
        let s = |n| retry_after(n).as_secs();
        assert_eq!([s(0), s(1), s(2), s(3), s(4), s(5), s(40)], [5, 10, 20, 40, 60, 60, 60]);
    }

    /// Sin mando virtual (en los tests no hay hub) no se escribe ningún
    /// motor en Dolphin ni en Cemu, en ninguna plataforma: la línea que
    /// apuntaba a un `XInput/0` inexistente era la única diferencia que
    /// Dolphin veía en un PC sin driver.
    #[test]
    fn sin_mando_virtual_no_hay_expresion_de_motor() {
        assert!(!pad_exists(0));
        assert!(motor_expression(0).is_none());
        assert!(cemu_node(0, 1).is_none());
    }

    #[test]
    fn slots_fuera_de_rango_se_ignoran() {
        let mut tr = Track::new();
        assert!(tr.set(9, Source::Pad, 255, 255, t0()).is_none());
        assert_eq!(tr.level(9), (0, 0));
    }

    #[test]
    fn quieren_mando_virtual_los_mandos_de_los_modos_con_emulador() {
        use crate::state::PlayerInfo;
        let mut players: [Option<PlayerInfo>; MAX_PLAYERS] = [None, None, None, None];
        let p = PlayerInfo {
            name: "m".into(),
            model: String::new(),
            battery_pct: 0,
            rtt_ms: None,
            role: Role::Wiimote,
            pad_wii: false,
            own_nunchuk: false,
            screen_only: false,
            switch_pad: Default::default(),
            retro_pad: Default::default(),
            retro_layout: None,
            tilt: false,
        };
        players[0] = Some(p.clone());
        let mut n = p.clone();
        n.role = Role::Nunchuk;
        players[3] = Some(n);
        let mut so = p.clone();
        so.screen_only = true;
        players[1] = Some(so);
        assert_eq!(wanted_slots(Mode::Dolphin, &players), [true, false, false, false]);
        assert_eq!(wanted_slots(Mode::Cemu, &players), [true, false, false, false]);
        assert_eq!(wanted_slots(Mode::RetroArch, &players), [true, false, false, false]);
        assert_eq!(wanted_slots(Mode::Pointer, &players), [false; 4]);
        assert_eq!(wanted_slots(Mode::Switch, &players), [false; 4]);
    }

    #[test]
    fn crc16_arc_y_guid_de_sdl() {
        assert_eq!(sdl_crc16(b"123456789"), 0xBB3D, "valor de comprobación de CRC-16/ARC");
        let g = sdl_guid(0x06, "PepoMote Wiimote 1", PAD_VENDOR, PAD_PRODUCT, PAD_VERSION);
        assert_eq!(g.len(), 32);
        assert!(g.starts_with("0600"), "bus virtual");
        assert_eq!(&g[8..16], "45500000", "vendor PE y ceros");
        assert_eq!(&g[16..24], "4d500000", "product PM y ceros");
        assert_eq!(&g[24..32], "01000000", "version 1 y ceros");
        let crc = sdl_crc16(b"PepoMote Wiimote 1").to_le_bytes();
        assert_eq!(&g[4..8], format!("{:02x}{:02x}", crc[0], crc[1]));
    }

    #[test]
    fn el_nodo_de_cemu_lleva_vibracion_a_tope_y_sin_botones() {
        let n = cemu_node_with("XInput", "2", 3);
        assert!(n.contains("<api>XInput</api>"));
        assert!(n.contains("<uuid>2</uuid>"));
        assert!(n.contains("<display_name>PepoMote J3 vibración</display_name>"));
        assert!(n.contains("<rumble>1</rumble>"));
        assert!(!n.contains("<entry>"));
        assert!(!n.contains("<motion>"));
    }

    #[test]
    fn el_estado_tiene_nombre_de_protocolo() {
        assert_eq!(Status::Ready.as_str(), "ready");
        assert_eq!(Status::NeedsDriver.as_str(), "driver");
        assert_eq!(Status::UinputDenied.as_str(), "denied");
        assert_eq!(Status::UinputMissing.as_str(), "denied");
        assert_eq!(Status::Unsupported.as_str(), "unsupported");
        // sin valor nuevo en el protocolo: los dos van como un fallo
        assert_eq!(Status::Checking.as_str(), Status::Failed.as_str());
        assert_eq!(Status::Unresponsive.as_str(), Status::Failed.as_str());
        assert_ne!(Status::Checking.as_str(), "ready");
    }

    /// Una llamada al driver que no contesta no para a quien la mira: `poll`
    /// vuelve al instante y, pasado el límite, avisa una sola vez. Así se
    /// comporta el hub con un ViGEmBus colgado (la ventana negra de la 1.12).
    #[test]
    fn una_llamada_en_marcha_no_bloquea_y_avisa_una_vez() {
        let mut p = Pending::start("prueba-pending", "una llamada de prueba".to_owned(), Duration::from_millis(10), || {
            std::thread::sleep(Duration::from_secs(60));
            1u8
        });
        let t = Instant::now();
        assert_eq!(p.poll(), None);
        assert!(t.elapsed() < Duration::from_secs(1), "poll no espera");
        assert!(!p.warned, "aún dentro del límite");
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(p.poll(), None);
        assert!(p.warned && p.slow(), "pasado el límite se avisa");
        assert_eq!(p.poll(), None, "y se sigue sin esperar");
        assert!(p.in_flight().slow());
    }

    #[test]
    fn la_espera_con_tope_se_rinde_y_deja_el_hilo_atras() {
        let p = Pending::start("prueba-pending", "una llamada de prueba".to_owned(), Duration::from_millis(10), || {
            std::thread::sleep(Duration::from_secs(60));
            1u8
        });
        let t = Instant::now();
        assert_eq!(p.wait(Duration::from_millis(50)), None);
        assert!(t.elapsed() < Duration::from_secs(2), "el tope manda");
    }

    #[test]
    fn una_llamada_que_contesta_llega_por_poll_y_por_wait() {
        let mut p = Pending::start("prueba-pending", "una llamada de prueba".to_owned(), Duration::from_secs(5), || 7u8);
        let mut got = None;
        for _ in 0..400 {
            if let Some(r) = p.poll() {
                got = Some(r);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got, Some(Ok(7)));
        assert!(!p.warned);
        let p = Pending::start("prueba-pending", "una llamada de prueba".to_owned(), Duration::from_secs(5), || 8u8);
        assert_eq!(p.wait(Duration::from_secs(5)), Some(8));
    }

    #[test]
    fn un_hilo_que_muere_sin_contestar_se_nota() {
        crate::log::quiet_panics();
        let mut p = Pending::start("prueba-pending", "una llamada de prueba".to_owned(), Duration::from_secs(5), || -> u8 {
            panic!("boom")
        });
        let mut got = None;
        for _ in 0..400 {
            if let Some(r) = p.poll() {
                got = Some(r);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got, Some(Err(())));
    }

    /// Lo que ven la ventana y el `hello` según lo que sabe el hub.
    #[test]
    fn el_estado_publicado_antes_y_durante_un_sondeo_colgado() {
        assert_eq!(view_status(true, None, false), Status::Ready);
        assert_eq!(view_status(true, Some(Status::Failed), true), Status::Ready, "con backend, lo demás no importa");
        assert_eq!(view_status(false, None, false), Status::Checking, "antes del primer resultado");
        assert_eq!(view_status(false, None, true), Status::Unresponsive, "el sondeo no contesta");
        assert_eq!(
            view_status(false, Some(Status::NeedsDriver), true),
            Status::Unresponsive,
            "un re-sondeo colgado pisa el resultado viejo"
        );
        assert_eq!(view_status(false, Some(Status::NeedsDriver), false), Status::NeedsDriver);
        assert_eq!(view_status(false, Some(Status::UinputDenied), false), Status::UinputDenied);
    }

    /// Sin hub (tests, `--diag` antes de arrancarlo) nada sondea el driver
    /// por su cuenta: el estado es «comprobando» y no hay mandos.
    #[test]
    fn sin_hub_nadie_sondea_el_driver() {
        assert_eq!(status(), Status::Checking);
        assert_eq!(ui_lines(), (Status::Checking, Vec::new()));
        assert!(stuck_note().is_none());
        assert!(xinput_index(0).is_none());
    }

    /// El intento del instalador se recuerda también si falla: si no, la
    /// ventana de permiso de Windows salía en cada arranque mientras fallara.
    #[test]
    fn el_intento_del_instalador_se_recuerda_tambien_si_falla() {
        assert!(remembers_attempt(RumbleSetup::Installed));
        assert!(remembers_attempt(RumbleSetup::Declined));
        assert!(remembers_attempt(RumbleSetup::Failed(1603)), "un fallo no puede sacar el permiso en cada arranque");
        assert!(remembers_attempt(RumbleSetup::Failed(0)), "ni siquiera si no se pudo lanzar");
        assert!(!remembers_attempt(RumbleSetup::Failed(1618)), "otra instalación en marcha: se resuelve sola");
        assert!(!remembers_attempt(RumbleSetup::Idle));
        assert!(!remembers_attempt(RumbleSetup::Installing));
    }
}
