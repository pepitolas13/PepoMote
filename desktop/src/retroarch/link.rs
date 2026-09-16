//! El enlace vivo con RetroArch: qué se manda, cuándo y a qué ritmo.
//!
//! RetroArch lee UN datagrama del mando en red por jugador y por sondeo de
//! entrada (uno por fotograma emulado). Mandar más rápido acumula cola y
//! retrasa todo; más lento deja fotogramas vacíos, y en Windows un
//! fotograma vacío puede poner el mando a cero (`errno` no es `EAGAIN`
//! tras un `recvfrom` sin datos). Así que aquí no hay temporizador: el
//! reloj lo da el propio RetroArch. Siempre hay UNA sonda (`VERSION` o
//! `GET_STATUS`) en camino por la interfaz de comandos; RetroArch la
//! contesta dentro del mismo sondeo en que lee el mando, así que cada
//! respuesta = un fotograma consumido = hueco para un datagrama por
//! jugador. Nunca hay más de un datagrama en espera por jugador, sea cual
//! sea la velocidad (50 Hz PAL, 60 Hz, avance rápido).
//!
//! Con el hueco de cada fotograma, por jugador, en este orden: el flanco de
//! botón más antiguo pendiente (una pulsación breve son dos fotogramas y no
//! se pierde), el eje que más se ha alejado de lo último enviado, y si no hay
//! nada nuevo, un refresco rotatorio de los controles que no están a cero
//! (si Windows vació el mando, se recompone solo en pocos fotogramas).
//!
//! Sin respuestas (RetroArch cerrado, o su interfaz de comandos apagada) se
//! sondea cada 2 s; si RetroArch está abierto pero mudo, se marca al ritmo
//! de 60 Hz con solo cambios y un refresco lento (modo degradado, avisado).
use super::mapping::{self, RetroPadKind};
use super::protocol::{self, Activity, Control, PadState, Reply, HOTKEYS};
use crate::net::codec::InputPacket;
use crate::net::MAX_PLAYERS;
use crate::state::{LockTolerant, SharedState};
use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Tope de flancos de botón en cola por jugador: por encima (aporreo más
/// rápido que los fotogramas) se compacta a un flanco por botón.
const MAX_EVENTS: usize = 64;

/// Un jugador (slot del móvil = jugador de RetroArch, puerto base + slot).
#[derive(Debug, Default)]
struct User {
    /// Hay un móvil en este slot en modo RetroArch.
    present: bool,
    /// Lo último que ha pedido el móvil.
    desired: PadState,
    /// Lo que creemos que tiene RetroArch (lo último enviado desde la
    /// última resincronización).
    known: PadState,
    /// Flancos de botón (id, pulsado) aún no enviados, en orden.
    events: VecDeque<(u8, bool)>,
    /// Ejes cuyo valor pedido difiere de lo enviado.
    dirty_axes: u8,
    /// Dónde va el refresco rotatorio.
    cursor: u8,
}

impl User {
    fn queue_button(&mut self, id: u8, down: bool) {
        // Dos flancos iguales seguidos del mismo botón no aportan nada
        if self.events.iter().rev().find(|(b, _)| *b == id).is_some_and(|(_, d)| *d == down) {
            return;
        }
        self.events.push_back((id, down));
        if self.events.len() > MAX_EVENTS {
            // Compactar: un flanco por botón, el último pedido
            let desired = self.desired;
            let known = self.known;
            self.events.clear();
            for id in 0..16 {
                if desired.button(id) != known.button(id) {
                    self.events.push_back((id, desired.button(id)));
                }
            }
        }
    }

    fn push(&mut self, state: PadState) {
        let changed = state.buttons ^ self.desired.buttons;
        for id in 0..16u8 {
            if changed & (1 << id) != 0 {
                let down = state.buttons & (1 << id) != 0;
                self.desired.buttons = (self.desired.buttons & !(1 << id)) | ((down as u16) << id);
                self.queue_button(id, down);
            }
        }
        for a in 0..4 {
            self.desired.axes[a] = state.axes[a];
            if state.axes[a] != self.known.axes[a] {
                self.dirty_axes |= 1 << a;
            } else {
                self.dirty_axes &= !(1 << a);
            }
        }
    }

    /// RetroArch acaba de arrancar (o se perdió de vista): parte de cero.
    fn resync(&mut self) {
        self.known = PadState::default();
        self.events.clear();
        for id in 0..16u8 {
            if self.desired.button(id) {
                self.events.push_back((id, true));
            }
        }
        self.dirty_axes = 0;
        for a in 0..4 {
            if self.desired.axes[a] != 0 {
                self.dirty_axes |= 1 << a;
            }
        }
    }

    /// Hay algo que mandar o que mantener vivo.
    fn active(&self) -> bool {
        self.present || !self.events.is_empty() || self.dirty_axes != 0 || !self.known.is_zero()
    }

    /// El datagrama de este fotograma (y lo que RetroArch tendrá tras leerlo).
    fn next(&mut self) -> Option<(Control, i16)> {
        if let Some((id, down)) = self.events.pop_front() {
            self.known.buttons = (self.known.buttons & !(1 << id)) | ((down as u16) << id);
            return Some((Control::Button(id), down as i16));
        }
        if self.dirty_axes != 0 {
            let a = (0..4u8)
                .filter(|a| self.dirty_axes & (1 << a) != 0)
                .max_by_key(|a| (self.desired.axes[*a as usize] as i32 - self.known.axes[*a as usize] as i32).abs())
                .unwrap_or(0);
            self.dirty_axes &= !(1 << a);
            let v = self.desired.axes[a as usize];
            self.known.axes[a as usize] = v;
            return Some((Control::Axis(a), v));
        }
        // Refresco rotatorio de lo que no está a cero
        for _ in 0..protocol::CONTROLS {
            let c = Control::from_index(self.cursor);
            self.cursor = (self.cursor + 1) % protocol::CONTROLS;
            let v = protocol::value(&self.known, c);
            if v != 0 {
                return Some((c, v));
            }
        }
        None
    }
}

/// Planificador puro (sin sockets ni relojes): qué datagrama toca por jugador.
#[derive(Debug, Default)]
pub struct Scheduler {
    users: [User; MAX_PLAYERS],
    /// Teclas de mantener sostenidas ahora, por slot (bit = índice en HOTKEYS).
    holds: [u64; MAX_PLAYERS],
    /// Comandos de un fotograma pendientes de enviar.
    pulses: VecDeque<&'static str>,
    /// Últimos bits del INPUT por slot (flancos de Home → menú).
    last_bits: [u32; MAX_PLAYERS],
}

impl Scheduler {
    pub fn set_present(&mut self, slot: usize, present: bool) {
        let u = &mut self.users[slot];
        if u.present && !present {
            // El móvil se va con algo pulsado: RetroArch lo tendría pulsado
            // para siempre (latch). Soltar todo antes de callar.
            u.push(PadState::default());
            self.holds[slot] = 0;
            self.last_bits[slot] = 0;
        }
        u.present = present;
    }

    pub fn push(&mut self, slot: usize, state: PadState) {
        self.users[slot].push(state);
    }

    /// Un paquete INPUT del móvil `slot` que hace de mando `kind`.
    pub fn push_input(&mut self, slot: usize, kind: RetroPadKind, p: &InputPacket) {
        self.push(slot, mapping::pad_state(kind, p));
        let last = self.last_bits[slot];
        if p.buttons & mapping::BIT_MENU != 0 && last & mapping::BIT_MENU == 0 {
            self.pulse("menu");
        }
        if kind == RetroPadKind::RetroPad {
            self.hold(slot, "fast_forward", p.buttons & mapping::BIT_FAST_FORWARD != 0);
        }
        self.last_bits[slot] = p.buttons;
    }

    /// Tecla rápida por nombre del protocolo (mensaje `hotkey`). Devuelve
    /// false si el nombre no existe.
    pub fn hotkey(&mut self, slot: usize, name: &str, down: bool) -> bool {
        let Some(h) = protocol::hotkey(name) else { return false };
        if h.hold {
            self.hold(slot, name, down);
        } else if down {
            self.pulses.push_back(h.command);
        }
        true
    }

    fn hold(&mut self, slot: usize, name: &str, down: bool) {
        if let Some(i) = HOTKEYS.iter().position(|h| h.name == name) {
            if down {
                self.holds[slot] |= 1 << i;
            } else {
                self.holds[slot] &= !(1 << i);
            }
        }
    }

    fn pulse(&mut self, name: &str) {
        if let Some(h) = protocol::hotkey(name) {
            self.pulses.push_back(h.command);
        }
    }

    /// Comandos de un fotograma que esperan (se vacía).
    pub fn take_pulses(&mut self) -> Vec<&'static str> {
        self.pulses.drain(..).collect()
    }

    /// Comandos de mantener a repetir en este fotograma (unión de todos los móviles).
    pub fn held_commands(&self) -> Vec<&'static str> {
        let all = self.holds.iter().fold(0u64, |a, b| a | b);
        HOTKEYS
            .iter()
            .enumerate()
            .filter(|(i, h)| h.hold && all & (1 << i) != 0)
            .map(|(_, h)| h.command)
            .collect()
    }

    pub fn resync(&mut self) {
        for u in &mut self.users {
            u.resync();
        }
    }

    /// Los datagramas de este fotograma: (slot, control, valor).
    pub fn frame(&mut self) -> Vec<(u8, Control, i16)> {
        let mut out = Vec::new();
        for (slot, u) in self.users.iter_mut().enumerate() {
            if !u.active() {
                continue;
            }
            if let Some((c, v)) = u.next() {
                out.push((slot as u8, c, v));
            }
        }
        out
    }

    /// Solo los cambios pendientes, sin refresco (modo degradado).
    pub fn changes(&mut self) -> Vec<(u8, Control, i16)> {
        let mut out = Vec::new();
        for (slot, u) in self.users.iter_mut().enumerate() {
            if u.events.is_empty() && u.dirty_axes == 0 {
                continue;
            }
            if let Some((c, v)) = u.next() {
                out.push((slot as u8, c, v));
            }
        }
        out
    }

    pub fn any_present(&self) -> bool {
        self.users.iter().any(|u| u.present)
    }

    pub fn known(&self, slot: usize) -> PadState {
        self.users[slot].known
    }
}

/// Qué se sabe de RetroArch ahora mismo (para la ventana y los avisos).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Live {
    /// Responde a la interfaz de comandos (reloj sincronizado).
    pub reachable: bool,
    /// Abierto (proceso visto) pero mudo: modo degradado.
    pub degraded: bool,
    pub version: Option<String>,
    pub activity: Option<Activity>,
    /// Sondeos por segundo que se le ven (≈ fotogramas/s).
    pub polls_per_sec: f32,
}

/// Puertos de RetroArch (los de retroarch.cfg que escribe PepoMote).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ports {
    pub base: u16,
    pub cmd: u16,
}

pub fn ports() -> Ports {
    let env = |k: &str, d: u16| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    Ports {
        base: env("PEPOMOTE_RETROARCH_PORT", protocol::DEFAULT_BASE_PORT),
        cmd: env("PEPOMOTE_RETROARCH_CMD_PORT", protocol::DEFAULT_CMD_PORT),
    }
}

/// El enlace: el planificador bajo candado y lo que el hilo publica.
pub struct Link {
    sched: Mutex<Scheduler>,
    live: Mutex<Live>,
    /// Avisos para la pantalla de RetroArch (`SHOW_MSG`) pendientes.
    osd: Mutex<VecDeque<String>>,
    ports: Ports,
}

static LINK: OnceLock<Arc<Link>> = OnceLock::new();

/// El enlace (None antes de `start`, o en tests).
pub fn link() -> Option<Arc<Link>> {
    LINK.get().cloned()
}

impl Link {
    pub fn push_input(&self, slot: u8, kind: RetroPadKind, p: &InputPacket) {
        self.sched.lock_tolerant().push_input(slot as usize, kind, p);
    }

    pub fn set_present(&self, slot: u8, present: bool) {
        self.sched.lock_tolerant().set_present(slot as usize, present);
    }

    pub fn hotkey(&self, slot: u8, name: &str, down: bool) -> bool {
        self.sched.lock_tolerant().hotkey(slot as usize, name, down)
    }

    /// Aviso en la pantalla de RetroArch (si responde).
    pub fn show_message(&self, text: String) {
        let mut q = self.osd.lock_tolerant();
        if q.len() < 8 {
            q.push_back(text);
        }
    }

    pub fn live(&self) -> Live {
        self.live.lock_tolerant().clone()
    }

    pub fn ports(&self) -> Ports {
        self.ports
    }
}

/// Arranca el hilo del enlace (una vez por proceso).
pub fn start(shared: SharedState) -> Option<Arc<Link>> {
    if let Some(l) = LINK.get() {
        return Some(l.clone());
    }
    let link = Arc::new(Link {
        sched: Mutex::new(Scheduler::default()),
        live: Mutex::new(Live::default()),
        osd: Mutex::new(VecDeque::new()),
        ports: ports(),
    });
    let _ = LINK.set(link.clone());
    let l = link.clone();
    let _ = crate::threads::spawn_guarded(
        "retroarch-link",
        crate::threads::OnPanic::Restart { after: Duration::from_secs(5), max: 50 },
        move || run(&l, &shared),
    );
    Some(link)
}

/// Espera corta y precisa (300 µs entre la respuesta y nuestro envío: que el
/// datagrama no entre en el MISMO sondeo que acaba de contestar la sonda).
fn precise_wait(d: Duration) {
    let t0 = Instant::now();
    while t0.elapsed() < d {
        std::thread::yield_now();
    }
}

const GUARD: Duration = Duration::from_micros(300);
/// Sincronizado y sin respuesta en este tiempo (RetroArch cargando algo),
/// la sonda se da por perdida y se repite.
const PROBE_RETRY: Duration = Duration::from_secs(2);
/// Sin sincronizar (RetroArch cerrado o arrancando): sonda cada medio segundo.
const PROBE_RETRY_IDLE: Duration = Duration::from_millis(500);
/// Sin respuesta en este tiempo, RetroArch ya no está (o no contesta).
const REACH_TTL: Duration = Duration::from_millis(1200);
/// Modo degradado: RetroArch abierto y mudo durante este tiempo.
const DEGRADED_AFTER: Duration = Duration::from_secs(3);
const DEGRADED_TICK: Duration = Duration::from_micros(16_667);
const DEGRADED_REFRESH: Duration = Duration::from_millis(50);
/// Cada cuántas sondas va un GET_STATUS en vez de VERSION.
const STATUS_EVERY: u32 = 30;

fn run(link: &Arc<Link>, shared: &SharedState) {
    let ports = link.ports;
    let loopback = Ipv4Addr::LOCALHOST;
    let cmd = match UdpSocket::bind(SocketAddrV4::new(loopback, 0)) {
        Ok(s) => s,
        Err(e) => {
            crate::log_line!("RetroArch: sin socket de comandos: {e}");
            return;
        }
    };
    if let Err(e) = cmd.connect(SocketAddrV4::new(loopback, ports.cmd)) {
        crate::log_line!("RetroArch: no puedo dirigir el socket de comandos: {e}");
        return;
    }
    let _ = cmd.set_read_timeout(Some(Duration::from_millis(5)));
    let pad = match UdpSocket::bind(SocketAddrV4::new(loopback, 0)) {
        Ok(s) => s,
        Err(e) => {
            crate::log_line!("RetroArch: sin socket del mando: {e}");
            return;
        }
    };
    let pad_addr = |slot: u8| SocketAddr::V4(SocketAddrV4::new(loopback, ports.base.wrapping_add(slot as u16)));

    let mut outstanding: Option<Instant> = None;
    let mut probes: u32 = 0;
    let mut last_reply: Option<Instant> = None;
    let mut synced = false;
    let mut replies_window = (Instant::now(), 0u32);
    let mut last_running_check = Instant::now() - Duration::from_secs(10);
    let mut running = false;
    let mut degraded = false;
    let mut next_degraded_tick = Instant::now();
    let mut next_degraded_refresh = Instant::now();
    let mut buf = [0u8; 4096];

    let send_cmd = |text: &str| {
        let _ = cmd.send(text.as_bytes());
    };
    let publish = |link: &Arc<Link>, f: &dyn Fn(&mut Live)| {
        let mut l = link.live.lock_tolerant();
        f(&mut l);
    };

    loop {
        let now = Instant::now();
        // Reloj: siempre una sonda en camino
        let retry = if synced { PROBE_RETRY } else { PROBE_RETRY_IDLE };
        if outstanding.is_none_or(|t| now.duration_since(t) > retry) {
            let probe = if probes % STATUS_EVERY == 0 { protocol::CMD_GET_STATUS } else { protocol::CMD_VERSION };
            probes = probes.wrapping_add(1);
            send_cmd(probe);
            outstanding = Some(now);
        }
        // ¿Sigue ahí?
        if synced && last_reply.is_some_and(|t| now.duration_since(t) > REACH_TTL) {
            synced = false;
            crate::log_line!("RetroArch: ya no responde a la interfaz de comandos");
            publish(link, &|l| {
                l.reachable = false;
                l.polls_per_sec = 0.0;
            });
            shared.lock_tolerant().retroarch_live = link.live();
        }
        if !synced && now.duration_since(last_running_check) >= Duration::from_secs(2) {
            last_running_check = now;
            running = super::running_exe().0 && !std::env::var_os("PEPOMOTE_ASSUME_EMULATOR_CLOSED").is_some();
            let mute_for = last_reply.map_or(now.duration_since(replies_window.0), |t| now.duration_since(t));
            let want = running && mute_for > DEGRADED_AFTER;
            if want != degraded {
                degraded = want;
                crate::log_line!(
                    "RetroArch: {}",
                    if degraded {
                        "abierto pero mudo (interfaz de comandos apagada): mando en modo degradado"
                    } else {
                        "fin del modo degradado"
                    }
                );
                publish(link, &|l| l.degraded = degraded);
                shared.lock_tolerant().retroarch_live = link.live();
            }
        }
        if degraded && !synced && now >= next_degraded_tick {
            next_degraded_tick = now + DEGRADED_TICK;
            let refresh = now >= next_degraded_refresh;
            if refresh {
                next_degraded_refresh = now + DEGRADED_REFRESH;
            }
            let (msgs, held) = {
                let mut s = link.sched.lock_tolerant();
                (if refresh { s.frame() } else { s.changes() }, s.held_commands())
            };
            for (slot, c, v) in msgs {
                let _ = pad.send_to(&protocol::encode(slot, c, v), pad_addr(slot));
            }
            for h in held {
                send_cmd(h);
            }
            for p in link.sched.lock_tolerant().take_pulses() {
                send_cmd(p);
            }
        }

        let n = match cmd.recv(&mut buf) {
            Ok(n) => n,
            Err(_) => continue,
        };
        let t_reply = Instant::now();
        // Todo lo que ya estuviera en el socket vino del MISMO sondeo (una
        // sonda repetida que al final sí se contestó): es un fotograma, no dos
        let mut reply = String::from_utf8_lossy(&buf[..n]).into_owned();
        let _ = cmd.set_nonblocking(true);
        while let Ok(m) = cmd.recv(&mut buf) {
            reply = String::from_utf8_lossy(&buf[..m]).into_owned();
        }
        let _ = cmd.set_nonblocking(false);
        let _ = cmd.set_read_timeout(Some(Duration::from_millis(5)));
        outstanding = None;
        last_reply = Some(t_reply);
        replies_window.1 += 1;
        if t_reply.duration_since(replies_window.0) >= Duration::from_secs(1) {
            let pps = replies_window.1 as f32 / t_reply.duration_since(replies_window.0).as_secs_f32();
            replies_window = (t_reply, 0);
            publish(link, &|l| l.polls_per_sec = pps);
            shared.lock_tolerant().retroarch_live = link.live();
        }
        match protocol::parse_reply(&reply) {
            Reply::Version(v) => publish(link, &|l| l.version = Some(v.clone())),
            Reply::Status(a) => publish(link, &|l| l.activity = Some(a.clone())),
            Reply::Other(_) => {}
        }
        if !synced {
            synced = true;
            degraded = false;
            crate::log_line!("RetroArch responde: mando en red sincronizado");
            {
                let mut s = link.sched.lock_tolerant();
                s.resync();
            }
            publish(link, &|l| {
                l.reachable = true;
                l.degraded = false;
            });
            shared.lock_tolerant().retroarch_live = link.live();
            // Reforzar el estado (por si arrancó con un móvil ya conectado)
            let present = link.sched.lock_tolerant().any_present();
            if present {
                link.show_message(crate::tr!("retroarch.osd_ready").to_owned());
            }
        }

        // Este fotograma: tras el margen, un datagrama por jugador, las
        // teclas mantenidas y los comandos sueltos; luego la siguiente sonda
        precise_wait(GUARD);
        let (msgs, held, pulses) = {
            let mut s = link.sched.lock_tolerant();
            (s.frame(), s.held_commands(), s.take_pulses())
        };
        for (slot, c, v) in msgs {
            let _ = pad.send_to(&protocol::encode(slot, c, v), pad_addr(slot));
        }
        for h in held {
            send_cmd(h);
        }
        for p in pulses {
            send_cmd(p);
        }
        if let Some(text) = link.osd.lock_tolerant().pop_front() {
            send_cmd(&format!("{} {}", protocol::CMD_SHOW_MSG, text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::codec;
    use protocol::RetroPad;

    fn st(buttons: u16, axes: [i16; 4]) -> PadState {
        PadState { buttons, axes }
    }

    fn one(s: &mut Scheduler) -> Option<(u8, Control, i16)> {
        let f = s.frame();
        assert!(f.len() <= 1, "un datagrama por jugador");
        f.into_iter().next()
    }

    #[test]
    fn un_boton_por_fotograma_y_los_flancos_en_orden() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        assert_eq!(one(&mut s), None, "en reposo no hay nada que mandar");
        s.push(0, st(1 << 8 | 1 << 0, [0; 4]));
        let a = one(&mut s).unwrap();
        let b = one(&mut s).unwrap();
        assert_eq!((a.1, a.2), (Control::Button(0), 1));
        assert_eq!((b.1, b.2), (Control::Button(8), 1));
        // Ya nada pendiente: refresco rotatorio de lo pulsado
        let r1 = one(&mut s).unwrap();
        let r2 = one(&mut s).unwrap();
        assert_eq!(r1.1, Control::Button(0));
        assert_eq!(r2.1, Control::Button(8));
        assert_eq!(r1.2, 1);
    }

    #[test]
    fn una_pulsacion_breve_no_se_pierde() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        s.push(0, st(1 << 8, [0; 4]));
        s.push(0, st(0, [0; 4]));
        assert_eq!(one(&mut s).map(|m| (m.1, m.2)), Some((Control::Button(8), 1)));
        assert_eq!(one(&mut s).map(|m| (m.1, m.2)), Some((Control::Button(8), 0)));
        assert_eq!(one(&mut s), None);
    }

    #[test]
    fn los_ejes_se_coalescen_y_va_primero_el_que_mas_cambia() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        s.push(0, st(0, [1000, 0, 0, 0]));
        s.push(0, st(0, [2000, -30000, 0, 0]));
        let m = one(&mut s).unwrap();
        assert_eq!((m.1, m.2), (Control::Axis(1), -30000), "el que más se aleja de lo enviado");
        let m = one(&mut s).unwrap();
        assert_eq!((m.1, m.2), (Control::Axis(0), 2000), "el valor final, no el intermedio");
        // vuelta a cero: dos ejes sucios más
        s.push(0, st(0, [0; 4]));
        assert_eq!(one(&mut s).unwrap().1, Control::Axis(1));
        assert_eq!(one(&mut s).unwrap().1, Control::Axis(0));
        assert_eq!(one(&mut s), None);
    }

    #[test]
    fn los_botones_van_antes_que_los_ejes() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        s.push(0, st(0, [5000, 0, 0, 0]));
        s.push(0, st(1 << 3, [5000, 0, 0, 0]));
        assert_eq!(one(&mut s).unwrap().1, Control::Button(3));
        assert_eq!(one(&mut s).unwrap().1, Control::Axis(0));
    }

    #[test]
    fn el_refresco_recompone_un_mando_vaciado() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        s.push(0, st(1 << 4, [0, 0, 700, 0]));
        assert_eq!(one(&mut s).unwrap().1, Control::Button(4));
        assert_eq!(one(&mut s).unwrap().1, Control::Axis(2));
        // Nada nuevo: refresco alterno de los dos controles vivos, sin fin
        let mut seen = std::collections::HashSet::new();
        for _ in 0..6 {
            let m = one(&mut s).unwrap();
            seen.insert(m.1);
            assert!(m.2 != 0);
        }
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn resincronizar_reenvia_solo_lo_que_no_esta_a_cero() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        s.push(0, st(1 << 0 | 1 << 1, [0, 100, 0, 0]));
        let _ = one(&mut s); // botón 0
        let _ = one(&mut s); // botón 1
        let _ = one(&mut s); // eje 1
        s.push(0, st(1 << 1, [0, 100, 0, 0]));
        let _ = one(&mut s); // suelta el botón 0
        s.resync();
        let a = one(&mut s).unwrap();
        let b = one(&mut s).unwrap();
        assert_eq!((a.1, a.2), (Control::Button(1), 1));
        assert_eq!((b.1, b.2), (Control::Axis(1), 100));
        // después, refresco de lo vivo: nunca un cero
        for _ in 0..4 {
            assert!(one(&mut s).unwrap().2 != 0, "el botón 0 (a cero) no se reenvía");
        }
    }

    #[test]
    fn al_irse_el_movil_se_suelta_todo_y_luego_se_calla() {
        let mut s = Scheduler::default();
        s.set_present(1, true);
        s.push(1, st(1 << 8, [0, 0, 0, -20000]));
        let _ = one(&mut s);
        let _ = one(&mut s);
        s.set_present(1, false);
        let a = one(&mut s).unwrap();
        let b = one(&mut s).unwrap();
        assert_eq!(a.0, 1);
        assert_eq!((a.1, a.2), (Control::Button(8), 0));
        assert_eq!((b.1, b.2), (Control::Axis(3), 0));
        assert_eq!(one(&mut s), None, "ya a cero y sin móvil: silencio");
        assert!(!s.users[1].active());
    }

    #[test]
    fn cada_jugador_su_puerto_y_su_datagrama() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        s.set_present(2, true);
        s.push(0, st(1 << 0, [0; 4]));
        s.push(2, st(1 << 9, [0; 4]));
        let f = s.frame();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].0, 0);
        assert_eq!(f[1].0, 2);
    }

    #[test]
    fn aporrear_mas_rapido_que_los_fotogramas_se_compacta() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        for i in 0..201u16 {
            s.push(0, st((i % 2) << 8, [0; 4]));
        }
        assert!(s.users[0].events.len() <= MAX_EVENTS);
        // el estado final (suelto) acaba llegando
        while !s.users[0].events.is_empty() {
            assert!(one(&mut s).is_some());
        }
        assert_eq!(s.known(0).buttons, 0);
        assert_eq!(one(&mut s), None, "todo a cero: silencio");
    }

    fn packet(buttons: u32) -> InputPacket {
        InputPacket {
            flags: codec::FLAG_STICK_VALID,
            session_id: 1,
            seq: 1,
            t_sensor_us: 0,
            quat: [1.0, 0.0, 0.0, 0.0],
            gyro: [0.0; 3],
            accel: [0.0; 3],
            buttons,
            recenter_count: 0,
            battery_pct: 50,
            touch_scroll_dy: 0,
            stick_x: 0,
            stick_y: 0,
            stick_rx: 0,
            stick_ry: 0,
            touch_x: 0,
            touch_y: 0,
        }
    }

    #[test]
    fn home_abre_el_menu_una_vez_y_capturar_mantiene_el_avance_rapido() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        s.push_input(0, RetroPadKind::RetroPad, &packet(codec::BTN_HOME));
        s.push_input(0, RetroPadKind::RetroPad, &packet(codec::BTN_HOME));
        assert_eq!(s.take_pulses(), vec!["MENU_TOGGLE"], "un flanco, un toggle");
        s.push_input(0, RetroPadKind::RetroPad, &packet(0));
        s.push_input(0, RetroPadKind::RetroPad, &packet(codec::BTN_HOME));
        assert_eq!(s.take_pulses(), vec!["MENU_TOGGLE"]);
        assert!(s.held_commands().is_empty());
        s.push_input(0, RetroPadKind::RetroPad, &packet(codec::BTN_SCREEN));
        assert_eq!(s.held_commands(), vec!["FAST_FORWARD_HOLD"]);
        s.push_input(0, RetroPadKind::RetroPad, &packet(0));
        assert!(s.held_commands().is_empty());
        // en el Mando Wii no hay Capturar: el bit 28 no hace nada
        s.push_input(0, RetroPadKind::Nes, &packet(codec::BTN_SCREEN));
        assert!(s.held_commands().is_empty());
        assert_eq!(one(&mut s), None, "Home y Capturar no son botones del RetroPad");
    }

    #[test]
    fn teclas_rapidas_por_nombre() {
        let mut s = Scheduler::default();
        assert!(s.hotkey(0, "save_state", true));
        assert!(s.hotkey(0, "save_state", false), "soltar no manda nada");
        assert!(!s.hotkey(0, "inventada", true));
        assert_eq!(s.take_pulses(), vec!["SAVE_STATE"]);
        assert!(s.hotkey(1, "rewind", true));
        assert!(s.hotkey(0, "fast_forward", true));
        let mut held = s.held_commands();
        held.sort();
        assert_eq!(held, vec!["FAST_FORWARD_HOLD", "REWIND"]);
        s.hotkey(1, "rewind", false);
        assert_eq!(s.held_commands(), vec!["FAST_FORWARD_HOLD"]);
        // el móvil se va con el avance rápido pulsado: se suelta
        s.set_present(0, true);
        s.set_present(0, false);
        assert!(s.held_commands().is_empty());
    }

    #[test]
    fn el_modo_degradado_solo_manda_cambios() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        s.push(0, st(1 << 0, [0; 4]));
        assert_eq!(s.changes().len(), 1);
        assert!(s.changes().is_empty(), "sin refresco");
        assert_eq!(s.frame().len(), 1, "el refresco normal sí");
    }

    #[test]
    fn el_mando_wii_apaisado_manda_b_y_a_por_uno_y_dos() {
        let mut s = Scheduler::default();
        s.set_present(0, true);
        s.push_input(0, RetroPadKind::Nes, &packet(codec::BTN_ONE));
        let m = one(&mut s).unwrap();
        assert_eq!(m.1, Control::Button(RetroPad::B as u8));
    }

    #[test]
    fn puertos_por_defecto() {
        let p = ports();
        assert!(p.base == protocol::DEFAULT_BASE_PORT || std::env::var_os("PEPOMOTE_RETROARCH_PORT").is_some());
        assert!(p.cmd == protocol::DEFAULT_CMD_PORT || std::env::var_os("PEPOMOTE_RETROARCH_CMD_PORT").is_some());
    }
}
