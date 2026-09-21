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

use crate::net::MAX_PLAYERS;
use crate::state::{LockTolerant, Mode, Role, SharedState};
use crate::tr;
use std::net::UdpSocket;
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

/// Cuánto esperar antes de volver a intentar crear un mando tras `failures`
/// fallos seguidos: 5, 10, 20, 40 s y de ahí un minuto. Un driver que falla
/// al crear cada 5 s enchufaba y desenchufaba un mando cada 5 s, y cada
/// vaivén hacía a Dolphin releer todos sus mandos (el DSU incluido).
fn retry_after(failures: u32) -> Duration {
    let veces = 1u32 << failures.min(4);
    (PROBE_EVERY * veces).min(RETRY_MAX)
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
}

impl Status {
    /// Valor de `ok.rumble` (PROTOCOL.md §3): el móvil enseña una línea por
    /// cada uno; los fallos de Linux van como «denied» y los de Windows como
    /// «driver» (la ventana del receptor da el detalle).
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ready => "ready",
            Status::NeedsDriver => "driver",
            Status::UinputDenied | Status::UinputMissing => "denied",
            Status::Unsupported => "unsupported",
            #[cfg(windows)]
            Status::Failed => "driver",
            #[cfg(not(windows))]
            Status::Failed => "denied",
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

/// Mandos virtuales: qué jugadores lo quieren y cuáles existen.
struct Pads {
    backend: Option<platform::Backend>,
    backend_err: Option<Status>,
    probed_at: Option<Instant>,
    /// Fallos seguidos al crear un mando: espacia los reintentos
    /// ([`retry_after`]); a cero en cuanto uno sale bien.
    create_failures: u32,
    wanted: [bool; MAX_PLAYERS],
    pads: [Option<platform::Pad>; MAX_PLAYERS],
}

pub struct Hub {
    track: Mutex<Track>,
    socket: Mutex<Option<UdpSocket>>,
    pads: Mutex<Pads>,
    shared: SharedState,
}

static HUB: OnceLock<Arc<Hub>> = OnceLock::new();

fn hub() -> Option<Arc<Hub>> {
    HUB.get().cloned()
}

/// Arranca el hilo de reenvío (y de reintento del driver). Una vez.
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
        }),
        shared,
    });
    if HUB.set(hub.clone()).is_err() {
        return;
    }
    let _ = crate::threads::spawn_guarded(
        "pmp-rumble",
        crate::threads::OnPanic::Restart { after: Duration::from_secs(5), max: 10 },
        move || {
            let mut last_probe = Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(20));
                let now = Instant::now();
                // El sondeo del driver (SetupAPI en Windows) vive AQUÍ: la
                // ventana y el canal de control solo leen el resultado
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
                // Un mando que se quiere y no existe (driver recién instalado,
                // permiso recién dado): se vuelve a intentar sin reiniciar. Si
                // nace ya con identidad, los perfiles de los emuladores se
                // escribieron sin él: se reescriben con su motor.
                if now.duration_since(last_probe) >= PROBE_EVERY {
                    last_probe = now;
                    let missing = {
                        let p = hub.pads.lock_tolerant();
                        p.wanted.iter().zip(p.pads.iter()).any(|(w, p)| *w && p.is_none())
                    };
                    if missing && hub.reconcile() {
                        crate::dolphin::maybe_auto_configure(&hub.shared);
                        crate::cemu::maybe_auto_configure(&hub.shared);
                    }
                }
            }
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

    /// Sondea el driver (o /dev/uinput) si toca: al arrancar, y cada
    /// [`PROBE_EVERY`] mientras no haya backend (más si crear falla seguido,
    /// [`retry_after`]). Solo lo llama el hilo del hub; los demás leen.
    fn probe_if_due(&self) {
        let mut p = self.pads.lock_tolerant();
        if p.backend.is_some() {
            return;
        }
        let due = p.probed_at.is_none_or(|t| t.elapsed() >= retry_after(p.create_failures));
        if !due {
            return;
        }
        p.probed_at = Some(Instant::now());
        match platform::Backend::probe() {
            Ok(b) => {
                p.backend = Some(b);
                p.backend_err = None;
            }
            Err(st) => p.backend_err = Some(st),
        }
    }

    /// Crea o quita mandos virtuales hasta cuadrar con `wanted`. `true` si
    /// ha nacido un mando que ya sabe quién es (en Windows, su hueco de
    /// XInput; en Linux, siempre): los perfiles de los emuladores que se
    /// escribieron sin él tienen que reescribirse con su motor. Quien llama
    /// decide si eso toca ya (el hub) o lo hace su propio paso siguiente
    /// (`sync`, desde la autoconfiguración).
    fn reconcile(&self) -> bool {
        let mut nacido_con_identidad = false;
        {
            let mut p = self.pads.lock_tolerant();
            let any_wanted = p.wanted.iter().any(|w| *w);
            if any_wanted && p.backend.is_none() {
                let due = p.probed_at.is_none_or(|t| t.elapsed() >= retry_after(p.create_failures));
                if due {
                    p.probed_at = Some(Instant::now());
                    match platform::Backend::probe() {
                        Ok(b) => {
                            p.backend = Some(b);
                            p.backend_err = None;
                        }
                        Err(st) => p.backend_err = Some(st),
                    }
                }
            }
            for slot in 0..MAX_PLAYERS {
                if p.wanted[slot] {
                    if p.pads[slot].is_some() {
                        continue;
                    }
                    let Some(backend) = p.backend.as_ref() else { continue };
                    match backend.create(slot as u8) {
                        Ok(pad) => {
                            crate::log_line!("Vibración: mando virtual del jugador {} creado ({})", slot + 1, pad.describe());
                            // Con hueco de XInput ya conocido (Linux: siempre)
                            // el perfil puede llevar su motor; si aún no,
                            // `settle_indices` reescribe al averiguarlo
                            if !pad.pending_index() {
                                nacido_con_identidad = true;
                            }
                            p.pads[slot] = Some(pad);
                            p.create_failures = 0;
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
                        }
                    }
                } else if p.pads[slot].take().is_some() {
                    crate::log_line!("Vibración: mando virtual del jugador {} retirado", slot + 1);
                }
            }
        }
        for slot in 0..MAX_PLAYERS {
            if !self.pads.lock_tolerant().wanted[slot] {
                set(slot as u8, Source::Pad, 0, 0);
            }
        }
        nacido_con_identidad
    }
}

/// ¿Existe ya el mando virtual del jugador del slot? Sin él no se escribe
/// ningún motor en los perfiles de los emuladores.
pub fn pad_exists(slot: u8) -> bool {
    hub().is_some_and(|h| h.pads.lock_tolerant().pads.get(slot as usize).is_some_and(|p| p.is_some()))
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
        // candado, que la ventana y el canal de control también lo cogen
        let after = platform::xinput_connected();
        let mut reconfigure = false;
        {
            let mut p = self.pads.lock_tolerant();
            // Los huecos que ya tienen dueño, para no dárselos a dos mandos:
            // los de una misma pasada de `reconcile` comparten foto previa.
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
            crate::dolphin::maybe_auto_configure(&self.shared);
            crate::cemu::maybe_auto_configure(&self.shared);
        }
    }

    /// Deja todos los mandos virtuales en reposo, sin destruirlos.
    fn release_pads(&self) {
        let mut p = self.pads.lock_tolerant();
        for hueco in p.pads.iter_mut() {
            if let Some(pad) = hueco.as_mut() {
                let _ = pad.apply(&crate::pad::PadState::default());
            }
        }
    }
}

/// Escribe el estado del móvil en el mando virtual del jugador (modo mando
/// universal). Si el mando deja de aceptarlo, se tira para que el sondeo de
/// `reconcile` lo vuelva a crear, que es el camino de recuperación de siempre.
/// `true` = el estado llegó al mando virtual. `false` = no hay mando (todavía
/// sin driver, o se acaba de tirar): quien llama tiene que OLVIDAR lo enviado,
/// porque el `Feed` solo manda lo que cambia y el mando nuevo nace en reposo;
/// si no, un botón que sigue pulsado no volvería a salir nunca.
pub fn push_pad(slot: u8, s: &crate::pad::PadState) -> bool {
    let Some(h) = hub() else { return false };
    let fallo = {
        let mut p = h.pads.lock_tolerant();
        let Some(hueco) = p.pads.get_mut(slot as usize) else { return false };
        let Some(pad) = hueco.as_mut() else { return false };
        match pad.apply(s) {
            Ok(()) => return true,
            Err(e) => {
                *hueco = None;
                e
            }
        }
    };
    crate::log_line!("Mando universal: el jugador {} no acepta el estado ({fallo}); se recrea", slot + 1);
    // El mando ya no existe: sin esto su última vibración se seguiría
    // reenviando cada 100 ms y el móvil no pararía nunca.
    set(slot, Source::Pad, 0, 0);
    false
}

/// Cuadra los mandos virtuales con el modo y los jugadores de ahora. Se
/// llama donde se autoconfiguran los emuladores y al irse un móvil.
pub fn sync(shared: &SharedState) {
    let Some(h) = hub() else { return };
    let (wanted, universal) = {
        let s = shared.lock_tolerant();
        (wanted_slots(s.mode, &s.players), s.mode == Mode::Gamepad)
    };
    h.pads.lock_tolerant().wanted = wanted;
    // Si nace un mando con identidad no hace falta reconfigurar desde aquí:
    // quien llama (la autoconfiguración) escribe los perfiles justo después
    let _ = h.reconcile();
    // Salir del mando universal no destruye el mando: los modos de emulador
    // también lo quieren, para la vibración. Pero conserva lo último que se
    // le escribió, así que un botón que se quedó pulsado al cambiar de modo
    // lo vería el emulador para siempre.
    if !universal {
        h.release_pads();
    }
}

/// Lo que puede este receptor ahora mismo (`ok.rumble`, ventana, --diag).
/// Solo lee lo que el hub ya sondeó (`probe_if_due`): la ventana lo pide a
/// cada fotograma y el canal de control en cada `hello`, y ninguno de los
/// dos tiene que pagar un sondeo del driver. Antes del primer sondeo del
/// hub (los primeros milisegundos, o sin hub: tests, `--diag`) se sondea
/// aquí una vez.
pub fn status() -> Status {
    let Some(h) = hub() else { return platform::static_status() };
    let p = h.pads.lock_tolerant();
    if p.backend.is_some() {
        return Status::Ready;
    }
    match p.backend_err {
        Some(st) => st,
        None => platform::static_status(),
    }
}

/// Índice XInput que tiene el mando virtual del slot (Windows), si existe.
pub fn xinput_index(slot: u8) -> Option<u32> {
    let h = hub()?;
    let p = h.pads.lock_tolerant();
    p.pads.get(slot as usize)?.as_ref()?.xinput_index()
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

/// Líneas para la ventana: estado y mandos virtuales que hay.
pub fn ui_lines() -> (Status, Vec<String>) {
    let st = status();
    let mut lines = Vec::new();
    if let Some(h) = hub() {
        let p = h.pads.lock_tolerant();
        for (slot, pad) in p.pads.iter().enumerate() {
            if let Some(pad) = pad {
                lines.push(tr!("rumble.pad", slot + 1, pad.describe()));
            }
        }
    }
    (st, lines)
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
    /// el detalle está en receptor.log).
    Failed(u32),
}

/// Windows: si falta el driver del mando virtual, instalarlo ahora con el
/// instalador que viaja dentro del exe (una vez por versión del instalador;
/// instalado o cancelado, no se vuelve a preguntar solo).
/// `PEPOMOTE_NO_DRIVER_SETUP` lo apaga (receptores de prueba).
pub fn ensure_driver_on_startup(shared: SharedState) {
    #[cfg(windows)]
    {
        let tried = shared.lock_tolerant().config.vigem_setup_version.clone();
        let skip = std::env::var_os("PEPOMOTE_NO_DRIVER_SETUP").is_some();
        if !vigem_setup::should_install(platform::static_status(), tried.as_deref(), skip) {
            return;
        }
        crate::log_line!(
            "Mando virtual: falta el driver; se instala el embebido (ViGEmBus {})",
            vigem_setup::SETUP_VERSION
        );
        run_setup(shared);
    }
    #[cfg(not(windows))]
    drop(shared);
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
        if matches!(result, Ok(vigem_setup::Outcome::Installed)) && platform::static_status() != Status::Ready {
            crate::log_line!("Mando virtual: el instalador terminó pero el driver no responde; segundo pase");
            result = vigem_setup::install();
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
            if matches!(state, RumbleSetup::Installed | RumbleSetup::Declined) {
                s.config.vigem_setup_version = Some(vigem_setup::SETUP_VERSION.to_owned());
                s.config.save();
            }
        }
        if state == RumbleSetup::Installed {
            if let Some(h) = hub() {
                // que el hub lo vea ya, sin esperar su cadencia
                h.pads.lock_tolerant().probed_at = None;
            }
        }
    });
}

/// Para `--diag`.
pub fn diag_lines() -> Vec<String> {
    let (st, pads) = ui_lines();
    let mut out = vec![format!("Vibración de los juegos: {} ({:?})", st.as_str(), st)];
    out.extend(pads.into_iter().map(|l| format!("  {l}")));
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
    }
}
