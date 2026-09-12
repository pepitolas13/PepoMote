//! Persistencia en ~/.config/pepomote/: emparejamiento (pairing.json),
//! calibración de ejes del sensor (axes.json) y ajustes (settings.json).

use crate::calib::Axes;
use crate::frame::Rotation;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pairing {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub pc_name: String,
}

/// Ajustes del usuario (settings.json). Los campos que falten toman el valor
/// por defecto, así un archivo de una versión anterior sigue valiendo. La
/// elección GamePad / Mando de Wii NO se guarda: cada sesión empieza como
/// diga el receptor.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// Giro del móvil apaisado en el GamePad de Wii U (izquierda por defecto).
    pub rotation: Rotation,
    /// Tema: como el sistema (en Linux móvil = claro), claro u oscuro.
    pub theme: crate::theme::ThemePref,
}

fn config_file(name: &str) -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "pepotech", "PepoMote").map(|d| d.config_dir().join(name))
}

fn path() -> Option<PathBuf> {
    config_file("pairing.json")
}

fn write_json<T: Serialize>(name: &str, v: &T) {
    let Some(path) = config_file(name) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(v) {
        let _ = std::fs::write(path, s);
    }
}

pub fn load_axes() -> Option<Axes> {
    config_file("axes.json")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub fn save_axes(a: &Axes) {
    write_json("axes.json", a);
}

pub fn clear_axes() {
    if let Some(p) = config_file("axes.json") {
        let _ = std::fs::remove_file(p);
    }
}

pub fn load_settings() -> Settings {
    config_file("settings.json")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(s: &Settings) {
    write_json("settings.json", s);
}

pub fn load() -> Option<Pairing> {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub fn save(p: &Pairing) {
    write_json("pairing.json", p);
}

/// "192.168.1.5" → (host, puerto por defecto); "192.168.1.5:26800" → (host, 26800).
pub fn split_host_port(s: &str) -> (String, u16) {
    let s = s.trim();
    match s.rsplit_once(':') {
        Some((h, p)) => match p.parse::<u16>() {
            Ok(port) if !h.is_empty() => (h.to_owned(), port),
            _ => (s.to_owned(), pmp::DEFAULT_PORT),
        },
        None => (s.to_owned(), pmp::DEFAULT_PORT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_y_puerto() {
        assert_eq!(split_host_port("192.168.1.5"), ("192.168.1.5".into(), 26761));
        assert_eq!(split_host_port("192.168.1.5:26800"), ("192.168.1.5".into(), 26800));
        assert_eq!(split_host_port(" 10.0.0.2:x "), ("10.0.0.2:x".into(), 26761));
    }

    #[test]
    fn ajustes_con_valores_por_defecto() {
        let d = Settings::default();
        assert_eq!(d.rotation, Rotation::Left);
        assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), d, "archivo vacío: por defecto");
        let s: Settings = serde_json::from_str(r#"{"rotation":"right"}"#).unwrap();
        assert_eq!(s.rotation, Rotation::Right);
        let s: Settings = serde_json::from_str(r#"{"otro":1,"pad_wii":true}"#).unwrap();
        assert_eq!(s.rotation, Rotation::Left, "campo ausente: por defecto; los desconocidos se ignoran");
        let mine = Settings { rotation: Rotation::Right, theme: crate::theme::ThemePref::Dark };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&mine).unwrap()).unwrap();
        assert_eq!(back, mine);
        let s: Settings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
        assert_eq!(s.theme, crate::theme::ThemePref::Light, "el tema se guarda en minúsculas");
        assert_eq!(s.rotation, Rotation::Left);
    }
}
