//! Sistemas sin mando virtual (macOS): la vibración de los juegos solo
//! llega por la extensión «rumble» del DSU, que ningún emulador habla hoy.

use super::Status;

pub struct Backend;

pub struct Pad;

impl Backend {
    pub fn probe() -> Result<Backend, Status> {
        Err(Status::Unsupported)
    }

    pub fn create(&self, _slot: u8) -> Result<Pad, String> {
        Err("sin mando virtual en este sistema".to_owned())
    }
}

impl Pad {
    /// Aquí no hay mando que mover; existe para que el receptor compile
    /// igual en los tres sistemas.
    pub fn apply(&mut self, _s: &crate::pad::PadState) -> Result<(), String> {
        Ok(())
    }

    pub fn xinput_index(&self) -> Option<u32> {
        None
    }

    /// Aquí no hay mando, así que tampoco hueco de XInput que averiguar.
    pub fn pending_index(&self) -> bool {
        false
    }

    pub fn settle(&mut self, _slot: u8, _taken: &[bool; 4], _after: &[bool; 4]) -> bool {
        false
    }

    pub fn describe(&self) -> String {
        String::new()
    }
}

pub fn static_status() -> Status {
    Status::Unsupported
}

/// Foto de XInput: aquí no existe.
pub fn xinput_connected() -> [bool; 4] {
    [false; 4]
}

pub fn motor_expression(_slot: u8) -> Option<String> {
    None
}

pub fn cemu_node(_slot: u8, _player: u8) -> Option<String> {
    None
}
