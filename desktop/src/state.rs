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

/// Estado compartido entre los hilos de red y la UI.
pub struct Shared {
    pub status: LinkStatus,
    pub mode: Mode,
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
