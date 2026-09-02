//! Persistencia en ~/.config/pepotech/PepoMote/: emparejamiento
//! (pairing.json) y calibración de ejes del sensor (axes.json).

use crate::calib::Axes;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pairing {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub pc_name: String,
}

fn config_file(name: &str) -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "pepotech", "PepoMote").map(|d| d.config_dir().join(name))
}

fn path() -> Option<PathBuf> {
    config_file("pairing.json")
}

pub fn load_axes() -> Option<Axes> {
    config_file("axes.json")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub fn save_axes(a: &Axes) {
    let Some(path) = config_file("axes.json") else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(a) {
        let _ = std::fs::write(path, s);
    }
}

pub fn clear_axes() {
    if let Some(p) = config_file("axes.json") {
        let _ = std::fs::remove_file(p);
    }
}

pub fn load() -> Option<Pairing> {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub fn save(p: &Pairing) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(p) {
        let _ = std::fs::write(path, s);
    }
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
}
