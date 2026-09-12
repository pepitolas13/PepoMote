//! Plataformas sin captura de ventana: la doble pantalla avisa y ya.

use super::Capture;
use crate::tr;

pub struct Capturer;

impl Capturer {
    pub fn new() -> Self {
        Self
    }

    pub fn capture(&mut self) -> Result<Capture, String> {
        Ok(Capture::NoWindow(tr!("screen.unavailable").to_owned()))
    }
}

/// Sin ventanas que esconder.
pub struct Minder;

impl Minder {
    pub fn new() -> Self {
        Self
    }

    pub fn hide(&mut self) -> bool {
        false
    }

    pub fn release(&mut self) {}
}

pub fn type_text(_text: &str) -> bool {
    false
}
