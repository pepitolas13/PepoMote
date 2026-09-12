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

/// Los PCs emparejados (pairings.json) y cuál es el actual. Un token es un
/// PC; su nombre y su IP pueden cambiar y se actualizan sin perder el token.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Pairings {
    pub current: Option<String>,
    pub list: Vec<Pairing>,
}

impl Pairings {
    /// Añade o actualiza (por token) conservando el orden, y lo deja como actual.
    pub fn upsert(&mut self, p: Pairing) {
        match self.list.iter_mut().find(|x| x.token == p.token) {
            Some(x) => *x = p.clone(),
            None => self.list.push(p.clone()),
        }
        self.current = Some(p.token);
    }

    /// Olvida un PC; si era el actual, el primero que quede pasa a serlo.
    pub fn forget(&mut self, token: &str) {
        self.list.retain(|x| x.token != token);
        if !self.list.iter().any(|x| Some(&x.token) == self.current.as_ref()) {
            self.current = self.list.first().map(|x| x.token.clone());
        }
    }

    /// Otro de los guardados pasa a ser el actual (uno desconocido se ignora).
    pub fn select(&mut self, token: &str) {
        if self.list.iter().any(|x| x.token == token) {
            self.current = Some(token.to_owned());
        }
    }

    pub fn current(&self) -> Option<Pairing> {
        self.list
            .iter()
            .find(|x| Some(&x.token) == self.current.as_ref())
            .or(self.list.first())
            .cloned()
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    config_file(name)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
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

/// Todos los PCs guardados; el pairing.json de versiones anteriores se migra.
pub fn load_all() -> Pairings {
    if let Some(all) = read_json::<Pairings>("pairings.json") {
        return all;
    }
    let mut all = Pairings::default();
    if let Some(p) = read_json::<Pairing>("pairing.json") {
        all.upsert(p);
    }
    all
}

pub fn save_all(all: &Pairings) {
    write_json("pairings.json", all);
}

/// El PC actual.
pub fn load() -> Option<Pairing> {
    load_all().current()
}

/// Guarda (por token) y lo deja como actual.
pub fn save(p: &Pairing) {
    let mut all = load_all();
    all.upsert(p.clone());
    save_all(&all);
}

/// Otro de los guardados pasa a ser el actual; devuelve el actual.
pub fn select(token: &str) -> Option<Pairing> {
    let mut all = load_all();
    all.select(token);
    save_all(&all);
    all.current()
}

/// Olvida un PC; devuelve el nuevo actual (si queda alguno).
pub fn forget(token: &str) -> Option<Pairing> {
    let mut all = load_all();
    all.forget(token);
    save_all(&all);
    all.current()
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

    #[test]
    fn varios_pcs_por_token() {
        let salon = Pairing { host: "192.168.1.5".into(), port: 26761, token: "tok-salon".into(), pc_name: "SALÓN".into() };
        let cuarto = Pairing { host: "192.168.1.9".into(), port: 26800, token: "tok-cuarto".into(), pc_name: "Cuarto".into() };
        let mut all = Pairings::default();
        assert_eq!(all.current(), None);
        all.upsert(salon.clone());
        all.upsert(cuarto.clone());
        assert_eq!(all.list, vec![salon.clone(), cuarto.clone()]);
        assert_eq!(all.current(), Some(cuarto.clone()), "el último emparejado es el actual");
        // mismo token: se actualiza sin duplicar (y pasa a ser el actual)
        all.upsert(Pairing { host: "192.168.1.50".into(), ..salon.clone() });
        assert_eq!(all.list.len(), 2);
        assert_eq!(all.current().unwrap().host, "192.168.1.50");
        all.select("tok-cuarto");
        assert_eq!(all.current(), Some(cuarto.clone()));
        all.select("no-existe");
        assert_eq!(all.current(), Some(cuarto.clone()), "uno desconocido se ignora");
        // olvidar el actual: el primero que quede
        all.forget("tok-cuarto");
        assert_eq!(all.current().unwrap().token, "tok-salon");
        all.forget("tok-salon");
        assert_eq!(all.current(), None);
        assert!(all.list.is_empty());
        // ida y vuelta JSON, y un archivo de una versión anterior (sin current)
        all.upsert(salon.clone());
        let back: Pairings = serde_json::from_str(&serde_json::to_string(&all).unwrap()).unwrap();
        assert_eq!(back, all);
        let old: Pairings = serde_json::from_str(r#"{"list":[{"host":"h","port":1,"token":"t","pc_name":"n"}]}"#).unwrap();
        assert_eq!(old.current().unwrap().token, "t", "sin current: el primero");
    }
}
