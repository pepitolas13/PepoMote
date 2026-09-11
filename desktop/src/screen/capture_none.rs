//! Plataformas sin captura de ventana: la doble pantalla avisa y ya.

use super::Capture;

pub struct Capturer;

impl Capturer {
    pub fn new() -> Self {
        Self
    }

    pub fn capture(&mut self) -> Result<Capture, String> {
        Ok(Capture::NoWindow("La doble pantalla no está disponible en este sistema".into()))
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
