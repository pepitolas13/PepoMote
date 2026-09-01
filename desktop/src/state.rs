use crate::pairing::PairingInfo;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    Waiting,
    #[allow(dead_code)] // h1
    Connected,
}

pub struct AppState {
    pub pairing: PairingInfo,
    pub status: LinkStatus,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            pairing: PairingInfo::generate(),
            status: LinkStatus::Waiting,
        }
    }
}
