use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Código de emparejamiento de 4 dígitos para móviles sin cámara (Linux
/// móvil), PROTOCOL.md §2. Caduca a los 120 s, es de un solo uso y se
/// regenera tras 5 fallos: 10 000 combinaciones no se pueden probar por la
/// LAN sin que el código cambie mucho antes.
pub struct PairCode {
    code: String,
    issued: Instant,
    failures: u8,
}

impl PairCode {
    pub const TTL: Duration = Duration::from_secs(120);
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pointer,
    Dolphin,
}

fn default_true() -> bool {
    true
}

/// Ajustes persistentes (settings.json en el directorio de config).
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Config {
    /// Grados de giro para cruzar el ancho de la pantalla (15-60).
    pub sens_deg: f32,
    /// true = posicionamiento absoluto (recomendado); false = relativo (juegos).
    pub abs_mode: bool,
    /// Configurar Dolphin solo (mandos multijugador) al conectar/desconectar.
    #[serde(default = "default_true")]
    pub auto_dolphin: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sens_deg: 40.0,
            abs_mode: true,
            auto_dolphin: true,
        }
    }
}

impl Config {
    fn path() -> Option<std::path::PathBuf> {
        directories::ProjectDirs::from("dev", "pepotech", "PepoMote")
            .map(|d| d.config_dir().join("settings.json"))
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

/// Un móvil conectado (indexado por slot: Jugador N = slot N-1).
#[derive(Clone)]
pub struct PlayerInfo {
    pub name: String,
    pub model: String,
    pub battery_pct: u8,
    pub rtt_ms: Option<f32>,
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
    pub last_error: Option<String>,
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
            last_error: None,
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
