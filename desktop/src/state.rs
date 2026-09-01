use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

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

/// Ajustes persistentes (settings.json en el directorio de config).
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Config {
    /// Grados de giro para cruzar el ancho de la pantalla (15-60).
    pub sens_deg: f32,
    /// true = posicionamiento absoluto (recomendado); false = relativo (juegos).
    pub abs_mode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sens_deg: 35.0,
            abs_mode: true,
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

/// Estado compartido entre los hilos de red y la UI.
pub struct Shared {
    pub status: LinkStatus,
    pub mode: Mode,
    pub config: Config,
    pub device_name: String,
    pub device_model: String,
    pub battery_pct: u8,
    pub pps: f32,
    pub sensor_hz: f32,
    pub rtt_ms: Option<f32>,
    pub last_error: Option<String>,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            status: LinkStatus::Waiting,
            mode: Mode::Pointer,
            config: Config::load(),
            device_name: String::new(),
            device_model: String::new(),
            battery_pct: 0,
            pps: 0.0,
            sensor_hz: 0.0,
            rtt_ms: None,
            last_error: None,
        }
    }
}

pub type SharedState = Arc<Mutex<Shared>>;

pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(Shared::new()))
}
