use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Código de emparejamiento de 4 dígitos para móviles sin cámara (Linux
/// móvil), PROTOCOL.md §2. Caduca a los 5 min, es de un solo uso y se
/// regenera tras 5 fallos: 10 000 combinaciones no se pueden probar por la
/// LAN sin que el código cambie mucho antes.
pub struct PairCode {
    code: String,
    issued: Instant,
    failures: u8,
}

impl PairCode {
    pub const TTL: Duration = Duration::from_secs(300);
    const MAX_FAILURES: u8 = 5;

    pub fn new() -> Self {
        Self {
            code: Self::generate(),
            issued: Instant::now(),
            failures: 0,
        }
    }

    fn generate() -> String {
        // Solo para los e2e: código fijo en el propio equipo
        if let Ok(c) = std::env::var("PEPOMOTE_PAIR_CODE") {
            if c.len() == 4 && c.bytes().all(|b| b.is_ascii_digit()) {
                return c;
            }
        }
        format!("{:04}", rand::thread_rng().gen_range(0..10_000u32))
    }

    fn refresh(&mut self) {
        if self.issued.elapsed() >= Self::TTL {
            *self = Self::new();
        }
    }

    /// Código vigente y tiempo que le queda (para pintarlo bajo el QR).
    pub fn current(&mut self) -> (String, Duration) {
        self.refresh();
        (
            self.code.clone(),
            Self::TTL.saturating_sub(self.issued.elapsed()),
        )
    }

    /// Valida un intento. Acierto = emparejado y código nuevo (un solo uso).
    pub fn try_accept(&mut self, attempt: &str) -> bool {
        self.refresh();
        if attempt == self.code {
            *self = Self::new();
            return true;
        }
        self.failures += 1;
        if self.failures >= Self::MAX_FAILURES {
            *self = Self::new();
        }
        false
    }
}

impl Default for PairCode {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    Waiting,
    Connected,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Pointer,
    Dolphin,
    /// Wii U: el móvil es un GamePad / Pro Controller / Mando Wii para Cemu.
    /// Como Dolphin (todo al DSU, nada al SO) pero se configura Cemu.
    Cemu,
    /// Switch: el móvil es un Pro Controller (o un par de Joy-Con) con
    /// giroscopio para Eden, que lee el DSU con su engine `cemuhookudp`.
    /// Como Cemu pero se configura Eden (`qt-config.ini`).
    Switch,
}

impl Mode {
    /// Nombre en el protocolo (PROTOCOL.md §3).
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Pointer => "pointer",
            Mode::Dolphin => "dolphin",
            Mode::Cemu => "cemu",
            Mode::Switch => "switch",
        }
    }

    /// Nombre del protocolo → modo. Desconocido = puntero (lo más seguro:
    /// un móvil nuevo contra un receptor viejo ve el eco y se entera).
    pub fn parse(s: Option<&str>) -> Mode {
        match s {
            Some("dolphin") => Mode::Dolphin,
            Some("cemu") => Mode::Cemu,
            Some("switch") => Mode::Switch,
            _ => Mode::Pointer,
        }
    }

    /// Modos que soporta este receptor (`ok.modes`, para que el móvil sepa
    /// si puede ofrecer Wii U y Switch).
    pub const ALL: [Mode; 4] = [Mode::Pointer, Mode::Dolphin, Mode::Cemu, Mode::Switch];

    /// En estos modos el receptor alimenta el DSU y no inyecta nada en el SO.
    pub fn feeds_dsu(self) -> bool {
        matches!(self, Mode::Dolphin | Mode::Cemu | Mode::Switch)
    }
}

fn default_true() -> bool {
    true
}

/// Ajustes persistentes (settings.json en el directorio de config).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Grados de giro para cruzar el ancho de la pantalla (15-60).
    pub sens_deg: f32,
    /// true = posicionamiento absoluto (recomendado); false = relativo (juegos).
    pub abs_mode: bool,
    /// Configurar Dolphin solo (mandos multijugador) al conectar/desconectar.
    #[serde(default = "default_true")]
    pub auto_dolphin: bool,
    /// Configurar Cemu solo (perfiles de mando) en modo Wii U.
    #[serde(default = "default_true")]
    pub auto_cemu: bool,
    /// Configurar Eden solo (mandos en qt-config.ini) en modo Switch.
    #[serde(default = "default_true")]
    pub auto_eden: bool,
    /// Cambiar de modo solo al abrir o cerrar Dolphin, Cemu o Eden.
    #[serde(default = "default_true")]
    pub auto_mode: bool,
    /// Carpeta de Cemu (la del Cemu.exe / AppImage). "" = detectar sola. Se
    /// aprende al ver a Cemu abierto y se guarda para configurarlo cerrado.
    #[serde(default)]
    pub cemu_dir: String,
    /// Carpeta de Dolphin (la del Dolphin.exe): un Dolphin portable guarda
    /// su configuración ahí. "" = detectar sola; se aprende al verlo abierto.
    #[serde(default)]
    pub dolphin_dir: String,
    /// Carpeta de Eden (la del eden.exe / AppImage): un Eden portable (carpeta
    /// `user` al lado) guarda su configuración ahí. "" = detectar sola.
    #[serde(default)]
    pub eden_dir: String,
    /// Linux: ya se ofreció la auto-reparación (firewall/uinput) una vez.
    /// Evita re-abrir el diálogo de contraseña en cada arranque si se canceló.
    #[serde(default)]
    pub fix_attempted: bool,
    /// Linux: puerto del móvil que consta abierto en el firewall (reparación
    /// con éxito, o un móvil que ya entró): con las reglas ilegibles no se avisa.
    #[serde(default)]
    pub firewall_opened_port: Option<u16>,
    /// Linux y macOS con varios monitores: nombre de la pantalla de apuntado
    /// ("" = todas: el escritorio entero).
    #[serde(default)]
    pub screen: String,
    /// Tema de la ventana: como el sistema (Windows lo sabe; en Linux, claro),
    /// o claro/oscuro fijo.
    #[serde(default)]
    pub theme: crate::theme::ThemePref,
    /// Idioma de la interfaz ("es" / "en"); None = el del sistema.
    #[serde(default)]
    pub lang: Option<String>,
    /// Aviso de versión nueva: consultar GitHub una vez al día (Ajustes).
    #[serde(default = "default_true")]
    pub update_check: bool,
    /// Versión anunciada que el usuario ocultó: no se vuelve a enseñar (una
    /// posterior, sí).
    #[serde(default)]
    pub update_dismissed: Option<crate::update::Version>,
    /// Última consulta (segundos UNIX) y última versión publicada que se vio.
    #[serde(default)]
    pub update_last_check: u64,
    #[serde(default)]
    pub update_latest: Option<crate::update::Version>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sens_deg: 40.0,
            abs_mode: true,
            auto_dolphin: true,
            auto_cemu: true,
            auto_eden: true,
            auto_mode: true,
            cemu_dir: String::new(),
            dolphin_dir: String::new(),
            eden_dir: String::new(),
            fix_attempted: false,
            firewall_opened_port: None,
            screen: String::new(),
            theme: crate::theme::ThemePref::System,
            lang: None,
            update_check: true,
            update_dismissed: None,
            update_last_check: 0,
            update_latest: None,
        }
    }
}

/// Directorio de configuración del receptor (settings.json, token.txt).
/// `PEPOMOTE_CONFIG_DIR` lo sustituye (solo para los e2e: un receptor de
/// prueba no debe leer ni pisar la configuración real).
pub fn config_dir() -> Option<std::path::PathBuf> {
    if let Some(d) = std::env::var_os("PEPOMOTE_CONFIG_DIR") {
        return Some(std::path::PathBuf::from(d));
    }
    directories::ProjectDirs::from("dev", "pepotech", "PepoMote").map(|d| d.config_dir().to_path_buf())
}

impl Config {
    fn path() -> Option<std::path::PathBuf> {
        config_dir().map(|d| d.join("settings.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}

/// Papel de un móvil: mando (Wiimote) o Nunchuk en la otra mano.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Wiimote,
    Nunchuk,
}

/// Un móvil conectado (indexado por slot DSU). Los Wiimotes ocupan slots
/// desde el 0 hacia arriba y los Nunchuks desde el 3 hacia abajo
/// (PROTOCOL.md §3): el Nunchuk i-ésimo pertenece al Wiimote i-ésimo.
#[derive(Clone)]
pub struct PlayerInfo {
    pub name: String,
    pub model: String,
    pub battery_pct: u8,
    pub rtt_ms: Option<f32>,
    pub role: Role,
    /// Modo Wii U: el móvil ha pedido ser Mando Wii (Wiimote emulado de
    /// Cemu) en vez de GamePad / Pro Controller.
    pub pad_wii: bool,
    /// El mando lleva su propio Nunchuk (un solo móvil): stick, C y Z van en
    /// su misma trama y, en Dolphin, `Extension = Nunchuk` lee de su mismo
    /// pad DSU. Se anuncia en el `hello` (`"nunchuk":"own"`) o con el
    /// mensaje `nunchuk`.
    pub own_nunchuk: bool,
    /// Modo Wii U: el móvil GamePad solo hace de pantalla táctil (a pantalla
    /// completa); el mando real del usuario sigue siendo el Controller 1 de
    /// Cemu y el receptor fusiona el DSU del móvil en su perfil. Se anuncia en
    /// el `hello` (`"screen_only":true`) o con el mensaje `screen_only`.
    pub screen_only: bool,
    /// Modo Switch: qué mando de Switch pide ser este móvil (`hello` `pad` o
    /// mensaje `pad` con `pro`, `joycons`, `joycon_side`, `joycon_r`).
    pub switch_pad: SwitchPad,
}

/// Tipo de mando de Switch que pide un móvil (modo Switch). Vocabulario del
/// mensaje `pad` en ese modo (PROTOCOL.md §3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SwitchPad {
    /// Pro Controller con giroscopio (por defecto).
    #[default]
    Pro,
    /// Par de Joy-Con en un solo móvil (los dos sensores de movimiento).
    Joycons,
    /// Un Joy-Con de lado (un móvil por jugador; el lado lo reparte el receptor).
    JoyconSide,
    /// El Joy-Con derecho de otro móvil: se empareja con un par (`Joycons`)
    /// sin pareja; sin nadie con quien emparejarse, es un par entero.
    JoyconR,
}

impl SwitchPad {
    pub fn as_str(self) -> &'static str {
        match self {
            SwitchPad::Pro => "pro",
            SwitchPad::Joycons => "joycons",
            SwitchPad::JoyconSide => "joycon_side",
            SwitchPad::JoyconR => "joycon_r",
        }
    }

    /// Nombre del protocolo → tipo; None si no es vocabulario de Switch.
    pub fn parse(s: &str) -> Option<SwitchPad> {
        match s {
            "pro" => Some(SwitchPad::Pro),
            "joycons" => Some(SwitchPad::Joycons),
            "joycon_side" => Some(SwitchPad::JoyconSide),
            "joycon_r" => Some(SwitchPad::JoyconR),
            _ => None,
        }
    }
}

/// Mitad de un par de Joy-Con (izquierdo o derecho): la que lleva un móvil
/// cuando el par va en dos móviles, o el lado de un Joy-Con de lado.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Half {
    Left,
    Right,
}

impl Half {
    pub fn as_str(self) -> &'static str {
        match self {
            Half::Left => "left",
            Half::Right => "right",
        }
    }
}

/// Un jugador tal como se configura en Eden (`player_{index}_*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SwitchPlayer {
    /// Índice del jugador en Eden (jugador − 1).
    pub index: u8,
    pub kind: SwitchPad,
    /// Pad DSU del móvil principal (su slot): el mando entero, o la mitad
    /// izquierda de un par en dos móviles.
    pub dsu_slot: u8,
    /// Pad DSU del segundo móvil (el Joy-Con derecho) si el par va en dos móviles.
    pub right_slot: Option<u8>,
    /// Joy-Con de lado: cuál es (izquierdo para J1, J3…; derecho para J2, J4…).
    pub side: Option<Half>,
}

/// Reparto para Eden: cada móvil mando es un jugador (J1.. por slot), salvo
/// los Joy-Con derechos (`joycon_r`), que se emparejan por orden con el
/// primer par (`joycons`) sin pareja; un Joy-Con derecho sin pareja juega
/// como un par entero. Los Nunchuk no tienen papel en Switch.
pub fn switch_layout(players: &[Option<PlayerInfo>]) -> Vec<SwitchPlayer> {
    let mains: Vec<(u8, SwitchPad)> = (0..players.len())
        .filter_map(|i| players[i].as_ref().map(|p| (i as u8, p)))
        .filter(|(_, p)| p.role == Role::Wiimote)
        .map(|(slot, p)| (slot, p.switch_pad))
        .collect();
    // emparejar: cada derecho con el primer par libre que lo precede o sigue
    let mut paired_right: Vec<Option<u8>> = vec![None; mains.len()]; // por índice de `mains`
    let mut taken: Vec<bool> = vec![false; mains.len()];
    for (ri, (rslot, rpad)) in mains.iter().enumerate() {
        if *rpad != SwitchPad::JoyconR {
            continue;
        }
        if let Some(mi) = mains
            .iter()
            .enumerate()
            .position(|(mi, (_, pad))| *pad == SwitchPad::Joycons && paired_right[mi].is_none() && !taken[mi])
        {
            paired_right[mi] = Some(*rslot);
            taken[ri] = true;
        }
    }
    let mut out = Vec::new();
    let mut sides = 0u8;
    for (mi, (slot, pad)) in mains.iter().enumerate() {
        if taken[mi] {
            continue; // va dentro de su par
        }
        let index = out.len() as u8;
        let kind = if *pad == SwitchPad::JoyconR { SwitchPad::Joycons } else { *pad };
        let side = if kind == SwitchPad::JoyconSide {
            let s = if sides % 2 == 0 { Half::Left } else { Half::Right };
            sides += 1;
            Some(s)
        } else {
            None
        };
        out.push(SwitchPlayer { index, kind, dsu_slot: *slot, right_slot: paired_right[mi], side });
    }
    out
}

/// Mitad que lleva el móvil del `slot` en modo Switch: la izquierda si su par
/// tiene un Joy-Con derecho de otro móvil, la derecha si es ese Joy-Con
/// derecho ya emparejado; None si lleva el mando entero.
pub fn switch_half(players: &[Option<PlayerInfo>], slot: u8) -> Option<Half> {
    let layout = switch_layout(players);
    if layout.iter().any(|p| p.dsu_slot == slot && p.right_slot.is_some()) {
        Some(Half::Left)
    } else if layout.iter().any(|p| p.right_slot == Some(slot)) {
        Some(Half::Right)
    } else {
        None
    }
}

/// Tipo de mando emulado en Cemu (modo Wii U) de un jugador.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PadKind {
    GamePad,
    Pro,
    Wiimote,
}

impl PadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PadKind::GamePad => "gamepad",
            PadKind::Pro => "pro",
            PadKind::Wiimote => "wiimote",
        }
    }
}

/// Un jugador tal como se configura en Cemu: `controller{index}.xml`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CemuPlayer {
    /// Índice del mando en Cemu (jugador − 1).
    pub index: u8,
    pub kind: PadKind,
    /// Pad DSU del móvil (su slot).
    pub dsu_slot: u8,
    /// Pad DSU del Nunchuk emparejado (solo si el jugador es Mando Wii).
    pub nunchuk_slot: Option<u8>,
    /// El móvil solo hace de pantalla táctil junto al mando real del usuario
    /// (solo tiene sentido en el GamePad).
    pub screen_only: bool,
}

/// Reparto para Cemu: el Jugador 1 es el GamePad y los demás Pro Controller,
/// salvo los que pidieron Mando Wii; el Nunchuk (de otro móvil) solo acompaña
/// a un Mando Wii. El Nunchuk propio no se aplica en Cemu de momento.
pub fn cemu_layout(players: &[Option<PlayerInfo>]) -> Vec<CemuPlayer> {
    external_layout(players)
        .iter()
        .enumerate()
        .map(|(i, (wslot, nslot))| {
            let pad_wii = players[*wslot as usize].as_ref().is_some_and(|p| p.pad_wii);
            let kind = if pad_wii {
                PadKind::Wiimote
            } else if i == 0 {
                PadKind::GamePad
            } else {
                PadKind::Pro
            };
            CemuPlayer {
                index: i as u8,
                kind,
                dsu_slot: *wslot,
                nunchuk_slot: if kind == PadKind::Wiimote { *nslot } else { None },
                screen_only: kind == PadKind::GamePad
                    && players[*wslot as usize].as_ref().is_some_and(|p| p.screen_only),
            }
        })
        .collect()
}

/// Lo que se le dice a cada móvil en `ok.pad` / eco `pad`, según el modo: en
/// Switch su tipo de mando de Switch (`pro`, `joycons`…; a un Nunchuk,
/// `nunchuk`: no tiene papel); en los demás modos el vocabulario de Cemu
/// ([`effective_pad_cemu`]), también en puntero y Dolphin (como siempre).
pub fn effective_pad(mode: Mode, players: &[Option<PlayerInfo>], slot: u8) -> &'static str {
    if mode != Mode::Switch {
        return effective_pad_cemu(players, slot);
    }
    match players.get(slot as usize).and_then(|p| p.as_ref()) {
        Some(p) if p.role == Role::Wiimote => p.switch_pad.as_str(),
        Some(_) => "nunchuk",
        None => SwitchPad::Pro.as_str(),
    }
}

/// Lo que se le dice a cada móvil en `ok.pad` / eco `pad`: su tipo de mando
/// en Cemu; a un Nunchuk, `wiimote` si su jugador es Mando de Wii (está en
/// uso) y `nunchuk` si no (en Wii U no tiene a quién acompañar).
pub fn effective_pad_cemu(players: &[Option<PlayerInfo>], slot: u8) -> &'static str {
    let layout = cemu_layout(players);
    match players.get(slot as usize).and_then(|p| p.as_ref()).map(|p| p.role) {
        Some(Role::Nunchuk) => {
            if layout.iter().any(|c| c.nunchuk_slot == Some(slot)) {
                "wiimote"
            } else {
                "nunchuk"
            }
        }
        _ => layout
            .iter()
            .find(|c| c.dsu_slot == slot)
            .map(|c| c.kind.as_str())
            .unwrap_or("gamepad"),
    }
}

/// Jugadores en orden: (slot del Wiimote, slot del Nunchuk asociado). El
/// Nunchuk de otro móvil i-ésimo es del Wiimote i-ésimo (`external_layout`),
/// salvo que ese mando lleve Nunchuk propio: entonces se empareja consigo
/// mismo (mismo pad DSU) y el externo queda sin uso.
pub fn player_layout(players: &[Option<PlayerInfo>]) -> Vec<(u8, Option<u8>)> {
    external_layout(players)
        .into_iter()
        .map(|(w, n)| {
            let own = players[w as usize].as_ref().is_some_and(|p| p.own_nunchuk);
            (w, if own { Some(w) } else { n })
        })
        .collect()
}

/// Reparto clásico, solo con Nunchuks de otro móvil: Wiimotes por slot
/// ascendente y Nunchuks por slot descendente, el i-ésimo con el i-ésimo. Es
/// el que numera a los jugadores y el que usa Cemu.
pub fn external_layout(players: &[Option<PlayerInfo>]) -> Vec<(u8, Option<u8>)> {
    let wiimotes: Vec<u8> = (0..players.len())
        .filter(|i| players[*i].as_ref().is_some_and(|p| p.role == Role::Wiimote))
        .map(|i| i as u8)
        .collect();
    let nunchuks: Vec<u8> = (0..players.len())
        .rev()
        .filter(|i| players[*i].as_ref().is_some_and(|p| p.role == Role::Nunchuk))
        .map(|i| i as u8)
        .collect();
    wiimotes
        .iter()
        .enumerate()
        .map(|(i, w)| (*w, nunchuks.get(i).copied()))
        .collect()
}

/// Número de jugador (1..4) del móvil del `slot`: el Wiimote i-ésimo por slot
/// ascendente y el Nunchuk i-ésimo por slot descendente son el jugador i.
pub fn player_number(players: &[Option<PlayerInfo>], slot: u8) -> u8 {
    let Some(p) = players.get(slot as usize).and_then(|p| p.as_ref()) else {
        return slot + 1;
    };
    let same: Vec<u8> = match p.role {
        Role::Wiimote => (0..players.len())
            .filter(|i| players[*i].as_ref().is_some_and(|q| q.role == Role::Wiimote))
            .map(|i| i as u8)
            .collect(),
        Role::Nunchuk => (0..players.len())
            .rev()
            .filter(|i| players[*i].as_ref().is_some_and(|q| q.role == Role::Nunchuk))
            .map(|i| i as u8)
            .collect(),
    };
    same.iter().position(|s| *s == slot).map(|i| i as u8 + 1).unwrap_or(slot + 1)
}

/// Latidos del Jugador 1 que guarda la sparkline de la ventana.
pub const RTT_HIST: usize = 60;

/// Estado de una configuración (Dolphin, Cemu, pantalla del GamePad) para la
/// ventana: el texto y si es «todo bien» (verde) o un aviso.
#[derive(Clone, Debug, PartialEq)]
pub struct CfgStatus {
    pub ok: bool,
    pub text: String,
}

impl CfgStatus {
    pub fn ok(text: String) -> Self {
        Self { ok: true, text }
    }

    pub fn warn(text: String) -> Self {
        Self { ok: false, text }
    }
}

/// Estado compartido entre los hilos de red y la UI.
pub struct Shared {
    pub status: LinkStatus,
    pub mode: Mode,
    pub config: Config,
    pub players: [Option<PlayerInfo>; crate::net::MAX_PLAYERS],
    pub pps: f32,
    pub sensor_hz: f32,
    /// RTT (ms) de los últimos latidos del Jugador 1 (sparkline).
    pub rtt_hist: std::collections::VecDeque<f32>,
    pub dsu_clients: usize,
    /// Resultado del último intento de configurar Dolphin (para la UI).
    pub dolphin_cfg_status: Option<CfgStatus>,
    /// El emulador estaba abierto: se configurará en cuanto se cierre.
    pub dolphin_pending: bool,
    pub cemu_pending: bool,
    /// Eden estaba abierto: su qt-config.ini se escribe en cuanto se cierre.
    pub eden_pending: bool,
    /// Cemu estaba abierto cuando se fue el último móvil «solo pantalla»: su
    /// nodo DSU se quita del perfil del usuario en cuanto Cemu se cierre.
    pub cemu_cleanup_pending: bool,
    /// Un puerto estaba ocupado y se cerró al proceso que lo tenía (aviso).
    pub port_notice: Option<String>,
    /// Resultado del último intento de configurar Cemu (para la UI).
    pub cemu_cfg_status: Option<CfgStatus>,
    /// Doble pantalla: estado de la captura de la ventana GamePad View.
    pub cemu_screen_status: Option<CfgStatus>,
    /// Resultado del último intento de configurar Eden (para la UI).
    pub eden_cfg_status: Option<CfgStatus>,
    /// Texto que un móvil quiere teclear en el PC (teclado en pantalla de
    /// Cemu) y que el inyector del SO aún no ha escrito.
    pub text_queue: Vec<String>,
    pub last_error: Option<String>,
    /// `last_error` es un error de inyección (lo limpia el inyector al
    /// recuperarse, no una conexión nueva).
    pub injection_error: bool,
    /// Linux: firewall que bloquea (o podría bloquear) el puerto del móvil.
    pub firewall: Option<crate::firewall::FirewallIssue>,
    /// Linux: la ventana debe ofrecer la reparación automática (cuenta atrás).
    pub auto_fix_due: bool,
    /// Linux: texto de la última reparación con éxito y cuándo (tarjeta «Listo»).
    pub fix_done: Option<(String, Instant)>,
    /// Backend de inyección activo (pie de la ventana y log); None = sin
    /// inyector todavía.
    pub injector: Option<&'static str>,
    /// Linux: /dev/uinput denegado (telemetría reintenta cada pocos segundos).
    pub uinput_denied: bool,
    /// Linux: no existe /dev/uinput (módulo uinput sin cargar).
    pub uinput_missing: bool,
    /// macOS: falta el permiso de Accesibilidad (la tarjeta de permisos lo
    /// explica; telemetría reintenta cada pocos segundos).
    pub ax_denied: bool,
    /// Hay un diálogo de reparación (pkexec) abierto ahora mismo.
    pub fixing: bool,
    /// Linux: por qué falló la última reparación (None = nunca, bien o cancelada).
    pub fix_failed: Option<String>,
    /// Linux: monitores detectados (nombre, ancho, alto lógicos) para el
    /// selector de pantalla de apuntado.
    pub screens: Vec<(String, i32, i32)>,
    /// Linux: mapeo de apuntado ya resuelto por el hilo de pantallas —
    /// (rect_norm [x0,y0,w,h], aspect, ancho_px) para la config actual. None
    /// = sin datos aún: el inyector usa todo el escritorio (identidad).
    pub pointing: Option<([f32; 4], f32, f32)>,
    pub pair_code: PairCode,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            status: LinkStatus::Waiting,
            mode: Mode::Pointer,
            config: Config::load(),
            players: [None, None, None, None],
            pps: 0.0,
            sensor_hz: 0.0,
            rtt_hist: std::collections::VecDeque::with_capacity(RTT_HIST),
            dsu_clients: 0,
            dolphin_cfg_status: None,
            dolphin_pending: false,
            cemu_pending: false,
            eden_pending: false,
            cemu_cleanup_pending: false,
            port_notice: None,
            cemu_cfg_status: None,
            cemu_screen_status: None,
            eden_cfg_status: None,
            text_queue: Vec::new(),
            last_error: None,
            injection_error: false,
            firewall: None,
            auto_fix_due: false,
            fix_done: None,
            injector: None,
            uinput_denied: false,
            uinput_missing: false,
            ax_denied: false,
            fixing: false,
            fix_failed: None,
            screens: Vec::new(),
            pointing: None,
            pair_code: PairCode::new(),
        }
    }

    pub fn player_count(&self) -> usize {
        self.players.iter().filter(|p| p.is_some()).count()
    }
}

pub type SharedState = Arc<Mutex<Shared>>;

/// Candado que sobrevive a un pánico de otro hilo: el hook de `log.rs` ya
/// dejó ese pánico apuntado; aquí se recupera el estado tal cual quedó en vez
/// de propagar el envenenamiento (que tumbaría la ventana en el siguiente
/// frame). Vale para cualquier `Mutex`; es lo que usa todo el receptor.
pub trait LockTolerant<T> {
    fn lock_tolerant(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockTolerant<T> for Mutex<T> {
    fn lock_tolerant(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(Shared::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_candado_tolerante_se_recupera_tras_un_panico() {
        crate::log::quiet_panics();
        let m = Arc::new(Mutex::new(7));
        let m2 = m.clone();
        let r = std::panic::catch_unwind(move || {
            let _g = m2.lock().unwrap();
            panic!("boom con el candado cogido");
        });
        assert!(r.is_err());
        assert!(m.is_poisoned(), "el pánico con el candado cogido lo envenena");
        assert_eq!(*m.lock_tolerant(), 7);
        *m.lock_tolerant() = 8;
        assert_eq!(*m.lock_tolerant(), 8);
    }

    #[test]
    fn config_sin_campos_de_update_carga_con_defaults() {
        // settings.json de una versión anterior: el aviso queda activado y sin historial
        let c: Config = serde_json::from_str(r#"{"sens_deg":40.0,"abs_mode":true}"#).unwrap();
        assert!(c.update_check);
        assert_eq!(c.update_dismissed, None);
        assert_eq!(c.update_last_check, 0);
        assert_eq!(c.update_latest, None);
        // y lo guardado se recupera tal cual
        let mut c2 = c.clone();
        c2.update_latest = Some(crate::update::Version([1, 6, 0]));
        c2.update_dismissed = Some(crate::update::Version([1, 6, 0]));
        c2.update_last_check = 1_700_000_000;
        let back: Config = serde_json::from_str(&serde_json::to_string(&c2).unwrap()).unwrap();
        assert!(back == c2, "ida y vuelta por JSON");
    }

    fn player(role: Role) -> Option<PlayerInfo> {
        Some(PlayerInfo {
            name: "m".into(),
            model: String::new(),
            battery_pct: 0,
            rtt_ms: None,
            role,
            pad_wii: false,
            own_nunchuk: false,
            screen_only: false,
            switch_pad: SwitchPad::Pro,
        })
    }

    /// Un mando en modo Switch con el tipo de mando pedido.
    fn switch_player(pad: SwitchPad) -> Option<PlayerInfo> {
        player(Role::Wiimote).map(|mut p| {
            p.switch_pad = pad;
            p
        })
    }

    /// Un mando con Nunchuk en el mismo móvil.
    fn player_own() -> Option<PlayerInfo> {
        player(Role::Wiimote).map(|mut p| {
            p.own_nunchuk = true;
            p
        })
    }

    #[test]
    fn modos_por_nombre() {
        assert_eq!(Mode::parse(Some("cemu")), Mode::Cemu);
        assert_eq!(Mode::parse(Some("dolphin")), Mode::Dolphin);
        assert_eq!(Mode::parse(Some("switch")), Mode::Switch);
        assert_eq!(Mode::parse(Some("pointer")), Mode::Pointer);
        assert_eq!(Mode::parse(Some("loquesea")), Mode::Pointer);
        assert_eq!(Mode::parse(None), Mode::Pointer);
        for m in Mode::ALL {
            assert_eq!(Mode::parse(Some(m.as_str())), m);
        }
        assert_eq!(Mode::ALL.len(), 4);
        assert!(Mode::Cemu.feeds_dsu() && Mode::Dolphin.feeds_dsu() && Mode::Switch.feeds_dsu() && !Mode::Pointer.feeds_dsu());
    }

    #[test]
    fn tipos_de_mando_de_switch_por_nombre() {
        for p in [SwitchPad::Pro, SwitchPad::Joycons, SwitchPad::JoyconSide, SwitchPad::JoyconR] {
            assert_eq!(SwitchPad::parse(p.as_str()), Some(p));
        }
        assert_eq!(SwitchPad::parse("gamepad"), None, "vocabulario de Cemu, no de Switch");
        assert_eq!(SwitchPad::parse("wiimote"), None);
        assert_eq!(SwitchPad::default(), SwitchPad::Pro);
    }

    #[test]
    fn reparto_para_switch() {
        // J1 Pro, J2 par en un móvil; el Nunchuk no cuenta
        let p = [switch_player(SwitchPad::Pro), switch_player(SwitchPad::Joycons), None, player(Role::Nunchuk)];
        assert_eq!(
            switch_layout(&p),
            vec![
                SwitchPlayer { index: 0, kind: SwitchPad::Pro, dsu_slot: 0, right_slot: None, side: None },
                SwitchPlayer { index: 1, kind: SwitchPad::Joycons, dsu_slot: 1, right_slot: None, side: None },
            ]
        );
        assert_eq!(effective_pad(Mode::Switch, &p, 0), "pro");
        assert_eq!(effective_pad(Mode::Switch, &p, 1), "joycons");
        assert_eq!(effective_pad(Mode::Switch, &p, 3), "nunchuk", "sin papel en Switch");
        assert_eq!(effective_pad(Mode::Switch, &p, 2), "pro", "slot vacío: valor por defecto");
        assert_eq!(switch_half(&p, 1), None, "par entero en un móvil");
        // en los otros modos sigue el vocabulario de Cemu
        assert_eq!(effective_pad(Mode::Cemu, &p, 0), "gamepad");
        assert_eq!(effective_pad(Mode::Pointer, &p, 1), "pro");
        assert_eq!(effective_pad(Mode::Dolphin, &p, 3), "nunchuk");
    }

    #[test]
    fn joycon_derecho_se_empareja_con_el_primer_par_libre() {
        // slot 0 Pro, slot 1 par, slot 2 Joy-Con derecho → J2 = par en dos móviles (1 + 2)
        let p = [switch_player(SwitchPad::Pro), switch_player(SwitchPad::Joycons), switch_player(SwitchPad::JoyconR), None];
        let l = switch_layout(&p);
        assert_eq!(l.len(), 2, "el derecho no es un jugador aparte");
        assert_eq!(l[1], SwitchPlayer { index: 1, kind: SwitchPad::Joycons, dsu_slot: 1, right_slot: Some(2), side: None });
        assert_eq!(switch_half(&p, 1), Some(Half::Left));
        assert_eq!(switch_half(&p, 2), Some(Half::Right));
        assert_eq!(switch_half(&p, 0), None);
        assert_eq!(effective_pad(Mode::Switch, &p, 2), "joycon_r", "el eco lleva lo pedido; la mitad va aparte");
        // el derecho antes que el par (por slot) también se empareja
        let p = [switch_player(SwitchPad::JoyconR), switch_player(SwitchPad::Joycons), None, None];
        let l = switch_layout(&p);
        assert_eq!(l, vec![SwitchPlayer { index: 0, kind: SwitchPad::Joycons, dsu_slot: 1, right_slot: Some(0), side: None }]);
        assert_eq!(switch_half(&p, 0), Some(Half::Right));
        // sin par libre: el derecho juega como par entero (y con trazado entero en el móvil)
        let p = [switch_player(SwitchPad::Pro), switch_player(SwitchPad::JoyconR), None, None];
        let l = switch_layout(&p);
        assert_eq!(l[1], SwitchPlayer { index: 1, kind: SwitchPad::Joycons, dsu_slot: 1, right_slot: None, side: None });
        assert_eq!(switch_half(&p, 1), None);
        // dos pares y dos derechos: cada derecho con un par, por orden
        let p = [
            switch_player(SwitchPad::Joycons),
            switch_player(SwitchPad::Joycons),
            switch_player(SwitchPad::JoyconR),
            switch_player(SwitchPad::JoyconR),
        ];
        let l = switch_layout(&p);
        assert_eq!(l.iter().map(|q| (q.dsu_slot, q.right_slot)).collect::<Vec<_>>(), vec![(0, Some(2)), (1, Some(3))]);
    }

    #[test]
    fn joycon_de_lado_alterna_izquierdo_y_derecho() {
        let p = [
            switch_player(SwitchPad::JoyconSide),
            switch_player(SwitchPad::Pro),
            switch_player(SwitchPad::JoyconSide),
            switch_player(SwitchPad::JoyconSide),
        ];
        let l = switch_layout(&p);
        assert_eq!(l.iter().map(|q| (q.index, q.kind, q.side)).collect::<Vec<_>>(), vec![
            (0, SwitchPad::JoyconSide, Some(Half::Left)),
            (1, SwitchPad::Pro, None),
            (2, SwitchPad::JoyconSide, Some(Half::Right)),
            (3, SwitchPad::JoyconSide, Some(Half::Left)),
        ]);
        assert_eq!(switch_half(&p, 0), None, "el lado va en el reparto, no como mitad de un par");
    }

    #[test]
    fn solo_pantalla_solo_para_el_gamepad() {
        let mut p = [player(Role::Wiimote), player(Role::Wiimote), None, None];
        p[0].as_mut().unwrap().screen_only = true;
        p[1].as_mut().unwrap().screen_only = true;
        let l = cemu_layout(&p);
        assert!(l[0].screen_only, "J1 GamePad solo pantalla");
        assert!(!l[1].screen_only, "J2 es Pro: sin pantalla que dar");
        // como Mando de Wii no aplica
        p[0].as_mut().unwrap().pad_wii = true;
        assert!(!cemu_layout(&p)[0].screen_only);
    }

    #[test]
    fn reparto_para_cemu() {
        // J1 GamePad, J2 Pro; el Nunchuk (slot 3) acompaña a J1 solo si es Mando Wii
        let p = [player(Role::Wiimote), player(Role::Wiimote), None, player(Role::Nunchuk)];
        assert_eq!(
            cemu_layout(&p),
            vec![
                CemuPlayer { index: 0, kind: PadKind::GamePad, dsu_slot: 0, nunchuk_slot: None, screen_only: false },
                CemuPlayer { index: 1, kind: PadKind::Pro, dsu_slot: 1, nunchuk_slot: None, screen_only: false },
            ]
        );
        assert_eq!(effective_pad_cemu(&p, 0), "gamepad");
        assert_eq!(effective_pad_cemu(&p, 1), "pro");
        assert_eq!(effective_pad_cemu(&p, 3), "nunchuk", "sin uso: J1 es GamePad");
        assert_eq!(effective_pad_cemu(&p, 2), "gamepad", "slot vacío: valor por defecto");
        // J1 pide Mando Wii: se lleva el Nunchuk y J2 sigue siendo Pro (no GamePad)
        let mut p = p;
        p[0].as_mut().unwrap().pad_wii = true;
        assert_eq!(
            cemu_layout(&p),
            vec![
                CemuPlayer { index: 0, kind: PadKind::Wiimote, dsu_slot: 0, nunchuk_slot: Some(3), screen_only: false },
                CemuPlayer { index: 1, kind: PadKind::Pro, dsu_slot: 1, nunchuk_slot: None, screen_only: false },
            ]
        );
        assert_eq!(effective_pad_cemu(&p, 0), "wiimote");
        assert_eq!(effective_pad_cemu(&p, 3), "wiimote", "el Nunchuk ya está en uso");
        // se va J1: J2 pasa a ser el GamePad (su móvil debe enterarse por `pad`)
        p[0] = None;
        assert_eq!(effective_pad_cemu(&p, 1), "gamepad");
        assert_eq!(effective_pad_cemu(&p, 3), "nunchuk");
        // solo un Nunchuk: nada que configurar
        let p = [None, None, None, player(Role::Nunchuk)];
        assert!(cemu_layout(&p).is_empty());
    }

    #[test]
    fn reparto_de_jugadores_con_nunchuks() {
        // slots: 0 mando, 1 mando, 2 Nunchuk, 3 Nunchuk → J1 = 0+3, J2 = 1+2
        let p = [player(Role::Wiimote), player(Role::Wiimote), player(Role::Nunchuk), player(Role::Nunchuk)];
        assert_eq!(player_layout(&p), vec![(0, Some(3)), (1, Some(2))]);
        assert_eq!(player_number(&p, 0), 1);
        assert_eq!(player_number(&p, 3), 1);
        assert_eq!(player_number(&p, 1), 2);
        assert_eq!(player_number(&p, 2), 2);
        // un mando y un Nunchuk
        let p = [player(Role::Wiimote), None, None, player(Role::Nunchuk)];
        assert_eq!(player_layout(&p), vec![(0, Some(3))]);
        // Nunchuk solo: sin mando al que acompañar (layout vacío) pero se numera
        let p = [None, None, None, player(Role::Nunchuk)];
        assert!(player_layout(&p).is_empty());
        assert_eq!(player_number(&p, 3), 1);
        // slot vacío → número por posición
        assert_eq!(player_number(&p, 1), 2);
    }

    #[test]
    fn nunchuk_en_el_mismo_movil() {
        // J1 lleva su Nunchuk: se empareja consigo mismo (mismo pad DSU)
        let p = [player_own(), None, None, None];
        assert_eq!(player_layout(&p), vec![(0, Some(0))]);
        assert_eq!(player_number(&p, 0), 1);
        // un Nunchuk de otro móvil es del J1 por orden, pero J1 ya lleva el
        // suyo: queda sin uso; J2 (sin propio) sigue sin Nunchuk
        let p = [player_own(), player(Role::Wiimote), None, player(Role::Nunchuk)];
        assert_eq!(player_layout(&p), vec![(0, Some(0)), (1, None)]);
        assert_eq!(external_layout(&p), vec![(0, Some(3)), (1, None)]);
        assert_eq!(player_number(&p, 3), 1, "la numeración no cambia");
        // dos mandos con Nunchuk propio y uno sin él
        let p = [player_own(), player(Role::Wiimote), player_own(), None];
        assert_eq!(player_layout(&p), vec![(0, Some(0)), (1, None), (2, Some(2))]);
        // en Cemu el Nunchuk propio no se aplica (de momento): J1 Mando Wii
        // se lleva el de otro móvil como siempre
        let mut p = [player_own(), player(Role::Wiimote), None, player(Role::Nunchuk)];
        p[0].as_mut().unwrap().pad_wii = true;
        assert_eq!(cemu_layout(&p)[0].nunchuk_slot, Some(3));
        assert_eq!(effective_pad_cemu(&p, 3), "wiimote");
    }

    #[test]
    fn codigo_de_un_solo_uso_y_rotacion_por_fallos() {
        let mut pc = PairCode::new();
        let (code, left) = pc.current();
        assert_eq!(code.len(), 4);
        assert!(code.bytes().all(|b| b.is_ascii_digit()));
        assert!(left <= PairCode::TTL);

        // 4 fallos: sigue el mismo código; el 5º lo rota y limpia el contador
        for _ in 0..4 {
            assert!(!pc.try_accept("xxxx"));
        }
        assert_eq!(pc.failures, 4);
        assert_eq!(pc.current().0, code, "aún vigente tras 4 fallos");
        assert!(!pc.try_accept("xxxx"));
        assert_eq!(pc.failures, 0, "el 5º fallo regenera el código");

        // acierto: acepta, y el código se renueva (un solo uso)
        let good = pc.current().0;
        let issued_before = pc.issued;
        std::thread::sleep(Duration::from_millis(5));
        assert!(pc.try_accept(&good));
        assert!(pc.issued > issued_before, "tras aceptar hay código nuevo");
        assert_eq!(pc.failures, 0);
    }

    #[test]
    fn codigo_caducado_se_regenera() {
        let mut pc = PairCode::new();
        pc.issued = Instant::now() - PairCode::TTL - Duration::from_secs(1);
        let (_, left) = pc.current();
        assert!(left > PairCode::TTL - Duration::from_secs(2), "regenerado: TTL casi entero");
    }
}
