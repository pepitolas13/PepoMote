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
}

impl Mode {
    /// Nombre en el protocolo (PROTOCOL.md §3).
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Pointer => "pointer",
            Mode::Dolphin => "dolphin",
            Mode::Cemu => "cemu",
        }
    }

    /// Nombre del protocolo → modo. Desconocido = puntero (lo más seguro:
    /// un móvil nuevo contra un receptor viejo ve el eco y se entera).
    pub fn parse(s: Option<&str>) -> Mode {
        match s {
            Some("dolphin") => Mode::Dolphin,
            Some("cemu") => Mode::Cemu,
            _ => Mode::Pointer,
        }
    }

    /// Modos que soporta este receptor (`ok.modes`, para que el móvil sepa
    /// si puede ofrecer Wii U).
    pub const ALL: [Mode; 3] = [Mode::Pointer, Mode::Dolphin, Mode::Cemu];

    /// En estos modos el receptor alimenta el DSU y no inyecta nada en el SO.
    pub fn feeds_dsu(self) -> bool {
        matches!(self, Mode::Dolphin | Mode::Cemu)
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
    /// Carpeta de Cemu (la del Cemu.exe / AppImage). "" = detectar sola. Se
    /// aprende al ver a Cemu abierto y se guarda para configurarlo cerrado.
    #[serde(default)]
    pub cemu_dir: String,
    /// Linux: ya se ofreció la auto-reparación (firewall/uinput) una vez.
    /// Evita re-abrir el diálogo de contraseña en cada arranque si se canceló.
    #[serde(default)]
    pub fix_attempted: bool,
    /// Linux multi-monitor: nombre de la pantalla de apuntado ("" = automática:
    /// la primaria, o la mayor).
    #[serde(default)]
    pub screen: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sens_deg: 40.0,
            abs_mode: true,
            auto_dolphin: true,
            auto_cemu: true,
            cemu_dir: String::new(),
            fix_attempted: false,
            screen: String::new(),
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
}

/// Reparto para Cemu: el Jugador 1 es el GamePad y los demás Pro Controller,
/// salvo los que pidieron Mando Wii; el Nunchuk solo acompaña a un Mando Wii.
pub fn cemu_layout(players: &[Option<PlayerInfo>]) -> Vec<CemuPlayer> {
    player_layout(players)
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
            }
        })
        .collect()
}

/// Lo que se le dice a cada móvil en `ok.pad` / eco `pad`: su tipo de mando
/// en Cemu; a un Nunchuk, `wiimote` si su jugador es Mando de Wii (está en
/// uso) y `nunchuk` si no (en Wii U no tiene a quién acompañar).
pub fn effective_pad(players: &[Option<PlayerInfo>], slot: u8) -> &'static str {
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

/// Jugadores en orden: (slot del Wiimote, slot del Nunchuk asociado).
pub fn player_layout(players: &[Option<PlayerInfo>]) -> Vec<(u8, Option<u8>)> {
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

/// Estado compartido entre los hilos de red y la UI.
pub struct Shared {
    pub status: LinkStatus,
    pub mode: Mode,
    pub config: Config,
    pub players: [Option<PlayerInfo>; crate::net::MAX_PLAYERS],
    pub pps: f32,
    pub sensor_hz: f32,
    pub dsu_clients: usize,
    /// Resultado del último intento de configurar Dolphin (para la UI).
    pub dolphin_cfg_status: Option<String>,
    /// Resultado del último intento de configurar Cemu (para la UI).
    pub cemu_cfg_status: Option<String>,
    /// Doble pantalla: estado de la captura de la ventana GamePad View.
    pub cemu_screen_status: Option<String>,
    pub last_error: Option<String>,
    /// Aviso de firewall Linux bloqueando el puerto (None = todo bien).
    pub firewall_hint: Option<String>,
    /// Linux: /dev/uinput denegado (telemetría reintenta cada pocos segundos).
    pub uinput_denied: bool,
    /// Hay un diálogo de reparación (pkexec) abierto ahora mismo.
    pub fixing: bool,
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
            dsu_clients: 0,
            dolphin_cfg_status: None,
            cemu_cfg_status: None,
            cemu_screen_status: None,
            last_error: None,
            firewall_hint: None,
            uinput_denied: false,
            fixing: false,
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

pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(Shared::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(role: Role) -> Option<PlayerInfo> {
        Some(PlayerInfo {
            name: "m".into(),
            model: String::new(),
            battery_pct: 0,
            rtt_ms: None,
            role,
            pad_wii: false,
        })
    }

    #[test]
    fn modos_por_nombre() {
        assert_eq!(Mode::parse(Some("cemu")), Mode::Cemu);
        assert_eq!(Mode::parse(Some("dolphin")), Mode::Dolphin);
        assert_eq!(Mode::parse(Some("pointer")), Mode::Pointer);
        assert_eq!(Mode::parse(Some("loquesea")), Mode::Pointer);
        assert_eq!(Mode::parse(None), Mode::Pointer);
        for m in Mode::ALL {
            assert_eq!(Mode::parse(Some(m.as_str())), m);
        }
        assert!(Mode::Cemu.feeds_dsu() && Mode::Dolphin.feeds_dsu() && !Mode::Pointer.feeds_dsu());
    }

    #[test]
    fn reparto_para_cemu() {
        // J1 GamePad, J2 Pro; el Nunchuk (slot 3) acompaña a J1 solo si es Mando Wii
        let p = [player(Role::Wiimote), player(Role::Wiimote), None, player(Role::Nunchuk)];
        assert_eq!(
            cemu_layout(&p),
            vec![
                CemuPlayer { index: 0, kind: PadKind::GamePad, dsu_slot: 0, nunchuk_slot: None },
                CemuPlayer { index: 1, kind: PadKind::Pro, dsu_slot: 1, nunchuk_slot: None },
            ]
        );
        assert_eq!(effective_pad(&p, 0), "gamepad");
        assert_eq!(effective_pad(&p, 1), "pro");
        assert_eq!(effective_pad(&p, 3), "nunchuk", "sin uso: J1 es GamePad");
        assert_eq!(effective_pad(&p, 2), "gamepad", "slot vacío: valor por defecto");
        // J1 pide Mando Wii: se lleva el Nunchuk y J2 sigue siendo Pro (no GamePad)
        let mut p = p;
        p[0].as_mut().unwrap().pad_wii = true;
        assert_eq!(
            cemu_layout(&p),
            vec![
                CemuPlayer { index: 0, kind: PadKind::Wiimote, dsu_slot: 0, nunchuk_slot: Some(3) },
                CemuPlayer { index: 1, kind: PadKind::Pro, dsu_slot: 1, nunchuk_slot: None },
            ]
        );
        assert_eq!(effective_pad(&p, 0), "wiimote");
        assert_eq!(effective_pad(&p, 3), "wiimote", "el Nunchuk ya está en uso");
        // se va J1: J2 pasa a ser el GamePad (su móvil debe enterarse por `pad`)
        p[0] = None;
        assert_eq!(effective_pad(&p, 1), "gamepad");
        assert_eq!(effective_pad(&p, 3), "nunchuk");
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
