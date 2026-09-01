//! Inyección de entrada en el SO. Windows: SendInput. Linux: uinput.

#[cfg(target_os = "linux")]
mod linux_uinput;
#[cfg(windows)]
mod windows_input;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyCode {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Enter,
    Escape,
    VolumeUp,
    VolumeDown,
    Mute,
    PlayPause,
    NextTrack,
    PrevTrack,
}

pub trait Injector: Send {
    fn move_rel(&mut self, dx: i32, dy: i32);
    /// Coordenadas normalizadas 0..1 sobre la pantalla primaria.
    fn move_abs(&mut self, nx: f32, ny: f32);
    fn button(&mut self, btn: MouseButton, down: bool);
    fn key(&mut self, key: KeyCode, down: bool);
    fn wheel(&mut self, delta: i32);
    /// Posición actual del cursor, normalizada a la pantalla primaria
    /// (puede salirse de 0..1 con varios monitores). None si el SO no
    /// permite leerla (Wayland).
    fn cursor_pos(&mut self) -> Option<(f32, f32)> {
        None
    }
}

#[cfg(windows)]
pub fn new_injector() -> Result<Box<dyn Injector>, String> {
    Ok(Box::new(windows_input::WinInjector::new()))
}

#[cfg(target_os = "linux")]
pub fn new_injector() -> Result<Box<dyn Injector>, String> {
    linux_uinput::UinputInjector::new().map(|i| Box::new(i) as Box<dyn Injector>)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn new_injector() -> Result<Box<dyn Injector>, String> {
    Err("plataforma sin soporte de inyección".into())
}
