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
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// Giro del móvil apaisado en el GamePad de Wii U (izquierda por defecto).
    pub rotation: Rotation,
    /// Tema: como el sistema (en Linux móvil = claro), claro u oscuro.
    pub theme: crate::theme::ThemePref,
    /// Idioma de la interfaz: el del sistema, español o inglés.
    pub lang: LangPref,
    /// Aviso de versión nueva: consultar GitHub una vez al día.
    pub update_check: bool,
    /// Versión anunciada que se ocultó (una posterior sí se enseña).
    pub update_dismissed: Option<crate::update::Version>,
    /// Última consulta (segundos UNIX) y última versión publicada vista.
    pub update_last_check: u64,
    pub update_latest: Option<crate::update::Version>,
    /// GamePad de Wii U sin pantalla táctil (ni doble pantalla): botones más grandes.
    pub gamepad_no_screen: bool,
    /// Pantalla del GamePad a pantalla completa: solo la pantalla de Cemu y el
    /// táctil, sin sticks ni botones (el mando real va en el PC).
    pub gamepad_full_screen: bool,
    /// En pantalla completa, botón de teclado arriba a la derecha.
    pub gamepad_full_screen_kb: bool,
    /// Nunchuk en el mismo móvil (Dolphin): el mando lleva su propio Nunchuk
    /// (`"nunchuk":"own"` en el hello). Sin trazado apaisado propio todavía en
    /// esta app, así que no se enseña en Inicio y va apagado.
    pub own_nunchuk: bool,
    /// Avisos del receptor en pantalla («Dolphin configurado…»); apagados,
    /// el banner no sale (los avisos locales sí).
    pub receiver_notices: bool,
    /// «Pulsar deslizando»: el botón por el que pasa el dedo se pulsa; al
    /// salir se suelta y se pulsa el siguiente. Apagado de serie.
    pub slide_press: bool,
    /// «Mantener al salir del botón»: un botón pulsado sigue pulsado mientras
    /// no se levante el dedo, aunque se salga. Encendido de serie (es lo que
    /// hacía esta app desde siempre); solo se puede elegir con el anterior
    /// apagado, que manda sobre él.
    pub sticky_press: bool,
    /// El botón «Teclado» del modo puntero hace antes un clic izquierdo donde
    /// apunta el usuario, para dejar el cursor dentro del campo. Apagado, el
    /// clic lo da el usuario con A. Encendido de serie.
    pub keyboard_click_first: bool,
    /// «Multimedia en todos los modos»: la fila multimedia del mando vertical
    /// sale siempre en modo puntero (el único en el que el PC atiende esas
    /// teclas); encendido, sale también en Dolphin, Wii U y RetroArch.
    /// Apagado de serie.
    pub media_everywhere: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rotation: Rotation::default(),
            theme: crate::theme::ThemePref::default(),
            lang: LangPref::default(),
            update_check: true,
            update_dismissed: None,
            update_last_check: 0,
            update_latest: None,
            gamepad_no_screen: false,
            gamepad_full_screen: false,
            gamepad_full_screen_kb: true,
            own_nunchuk: false,
            receiver_notices: true,
            slide_press: false,
            sticky_press: true,
            keyboard_click_first: true,
            media_everywhere: false,
        }
    }
}

/// Idioma elegido (settings.json): el del sistema (español si no es inglés)
/// o uno fijo.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LangPref {
    #[default]
    System,
    Es,
    En,
}

impl LangPref {
    pub fn resolve(self) -> crate::i18n::Lang {
        match self {
            LangPref::System => crate::i18n::detect_system(),
            LangPref::Es => crate::i18n::Lang::Es,
            LangPref::En => crate::i18n::Lang::En,
        }
    }

    pub fn from_lang(l: crate::i18n::Lang) -> Self {
        match l {
            crate::i18n::Lang::Es => LangPref::Es,
            crate::i18n::Lang::En => LangPref::En,
        }
    }
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

/// Mando de consola elegido a mano en RetroArch: `global` («auto» o un id de
/// `pmp::retro::LAYOUT_IDS`) y, por juego (ruta del contenido que anuncia el
/// receptor), la plantilla que se prefiere (tope 50, el más viejo fuera).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetroLayouts {
    pub global: String,
    pub by_path: Vec<(String, String)>,
}

impl Default for RetroLayouts {
    fn default() -> Self {
        Self { global: "auto".to_owned(), by_path: Vec::new() }
    }
}

impl RetroLayouts {
    /// La elegida a mano para ese juego (o la global), o None = automático.
    pub fn choice_for(&self, path: Option<&str>) -> Option<&str> {
        path.and_then(|p| self.by_path.iter().find(|(k, _)| k == p).map(|(_, v)| v.as_str()))
            .or_else(|| (self.global != "auto").then_some(self.global.as_str()))
    }

    /// Elegir `id` (None = automático) para el juego `path` (None = sin juego
    /// cargado: vale para todos hasta que RetroArch cargue uno).
    pub fn pick(&mut self, path: Option<&str>, id: Option<&str>) {
        match path {
            Some(p) => {
                self.by_path.retain(|(k, _)| k != p);
                match id {
                    Some(id) => {
                        self.by_path.push((p.to_owned(), id.to_owned()));
                        if self.by_path.len() > 50 {
                            self.by_path.remove(0);
                        }
                    }
                    // «Automático» con un juego: ese juego sigue al PC, y la global también
                    None => self.global = "auto".to_owned(),
                }
            }
            None => self.global = id.unwrap_or("auto").to_owned(),
        }
    }
}

#[cfg(test)]
#[test]
fn retro_layouts_por_juego_y_global_con_tope() {
    let mut r = RetroLayouts::default();
    assert_eq!(r.choice_for(Some("a.zip")), None, "de serie, automático");
    r.pick(None, Some("md3"));
    assert_eq!(r.choice_for(None), Some("md3"), "sin juego: global");
    assert_eq!(r.choice_for(Some("a.zip")), Some("md3"), "la global vale para todos");
    r.pick(Some("a.zip"), Some("nes"));
    assert_eq!(r.choice_for(Some("a.zip")), Some("nes"), "el juego manda");
    assert_eq!(r.choice_for(Some("b.zip")), Some("md3"));
    r.pick(Some("a.zip"), None);
    assert_eq!((r.choice_for(Some("a.zip")), r.choice_for(None)), (None, None), "automático con juego: ese juego y la global");
    for i in 0..60 {
        r.pick(Some(&format!("g{i}.zip")), Some("snes"));
    }
    assert_eq!(r.by_path.len(), 50, "tope");
    assert_eq!(r.choice_for(Some("g0.zip")), None, "el más viejo fuera");
    assert_eq!(r.choice_for(Some("g59.zip")), Some("snes"));
    let json = serde_json::to_string(&r).unwrap();
    assert_eq!(serde_json::from_str::<RetroLayouts>(&json).unwrap(), r, "ida y vuelta");
}

pub fn load_retro_layouts() -> RetroLayouts {
    read_json("retro_layouts.json").unwrap_or_default()
}

pub fn save_retro_layouts(r: &RetroLayouts) {
    write_json("retro_layouts.json", r);
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
        assert!(!d.media_everywhere, "la multimedia en todos los modos viene apagada");
        assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), d, "archivo vacío: por defecto");
        let s: Settings = serde_json::from_str(r#"{"rotation":"right"}"#).unwrap();
        assert_eq!(s.rotation, Rotation::Right);
        let s: Settings = serde_json::from_str(r#"{"otro":1,"pad_wii":true}"#).unwrap();
        assert_eq!(s.rotation, Rotation::Left, "campo ausente: por defecto; los desconocidos se ignoran");
        let mine = Settings {
            rotation: Rotation::Right,
            theme: crate::theme::ThemePref::Dark,
            lang: LangPref::En,
            update_check: false,
            update_dismissed: Some(crate::update::Version([1, 6, 0])),
            update_last_check: 1_700_000_000,
            update_latest: Some(crate::update::Version([1, 7, 0])),
            gamepad_no_screen: true,
            gamepad_full_screen: true,
            gamepad_full_screen_kb: false,
            own_nunchuk: true,
            receiver_notices: false,
            slide_press: true,
            sticky_press: false,
            keyboard_click_first: false,
            media_everywhere: true,
        };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&mine).unwrap()).unwrap();
        assert_eq!(back, mine);
        assert!(d.update_check, "el aviso de versión nueva viene activado");
        assert!(!d.gamepad_no_screen, "el GamePad lleva pantalla táctil salvo que se quite");
        assert!(!d.gamepad_full_screen, "la pantalla completa viene apagada");
        assert!(d.gamepad_full_screen_kb, "con pantalla completa, el botón de teclado viene encendido");
        assert!(!d.own_nunchuk, "sin trazado propio, el Nunchuk en el mismo móvil va apagado");
        assert!(d.receiver_notices, "los avisos del receptor vienen encendidos");
        assert!(!d.slide_press, "pulsar deslizando viene apagado");
        assert!(d.keyboard_click_first, "el clic antes de escribir viene encendido");
        assert!(
            serde_json::from_str::<Settings>(r#"{"rotation":"left"}"#).unwrap().keyboard_click_first,
            "un settings.json de una versión anterior sigue valiendo"
        );
        assert!(d.sticky_press, "un botón pulsado sigue pulsado al salirse el dedo, como siempre");
        assert_eq!(d.update_latest, None);
        let s: Settings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
        assert_eq!(s.theme, crate::theme::ThemePref::Light, "el tema se guarda en minúsculas");
        assert_eq!(s.rotation, Rotation::Left);
        assert_eq!(s.lang, LangPref::System, "sin idioma guardado: el del sistema");
        assert_eq!(serde_json::to_string(&LangPref::En).unwrap(), "\"en\"");
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
