//! Qué juego acaba de cargar RetroArch. RetroArch escribe su historial
//! (`content_history.lpl`, JSON) al cargar contenido, con la ruta, el núcleo
//! y la base de datos; la ficha del núcleo (`info/<núcleo>_libretro.info`,
//! mismo formato `clave = "valor"` que retroarch.cfg) da el `systemid`. Sin
//! GET_STATUS: cierra la 1.22.2 (ver protocol.rs). Se sondea desde el
//! vigilante `emu-watch` (cada 2 s) por (mtime, tamaño) del archivo, y se
//! relee también cuando RetroArch acaba de abrirse: la misma partida
//! recargada no toca el archivo.
use super::cfg;
use super::consoles::{self, Console};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// La entrada más reciente del historial (`items[0]`).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct HistoryEntry {
    pub path: String,
    pub label: String,
    pub core_path: String,
    pub core_name: String,
    pub db_name: String,
}

/// `Ok(None)`: historial sin entradas. `Err(())`: JSON roto o a medias (se
/// reintenta en el siguiente sondeo).
pub fn parse_history_top(text: &str) -> Result<Option<HistoryEntry>, ()> {
    let v: Value = serde_json::from_str(text).map_err(|_| ())?;
    let Some(item) = v.get("items").and_then(|i| i.as_array()).and_then(|a| a.first()) else {
        return Ok(None);
    };
    let s = |k: &str| item.get(k).and_then(|v| v.as_str()).unwrap_or("").to_owned();
    Ok(Some(HistoryEntry {
        path: s("path"),
        label: s("label"),
        core_path: s("core_path"),
        core_name: s("core_name"),
        db_name: s("db_name"),
    }))
}

/// Lo que interesa de `info/<núcleo>_libretro.info`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CoreInfo {
    pub systemid: Option<String>,
    pub systemname: Option<String>,
    pub corename: Option<String>,
}

pub fn parse_info(text: &str) -> CoreInfo {
    CoreInfo {
        systemid: cfg::get(text, "systemid"),
        systemname: cfg::get(text, "systemname"),
        corename: cfg::get(text, "corename"),
    }
}

/// «Sega - MS/GG/MD/CD (Genesis Plus GX)» → «Genesis Plus GX» (el último
/// grupo entre paréntesis; si no lo hay, todo).
pub fn short_core_name(display_name: &str) -> String {
    let t = display_name.trim();
    if let Some(open) = t.rfind('(') {
        if let Some(close) = t[open..].find(')') {
            let inner = t[open + 1..open + close].trim();
            if !inner.is_empty() {
                return inner.to_owned();
            }
        }
    }
    t.to_owned()
}

/// Quita los grupos finales ` (USA)`, ` [!]`… y espacios.
fn clean_title(s: &str) -> String {
    let mut t = s.trim().to_owned();
    loop {
        let trimmed = t.trim_end();
        let Some(last) = trimmed.chars().last() else { break };
        let open = match last {
            ')' => '(',
            ']' => '[',
            _ => break,
        };
        let Some(pos) = trimmed.rfind(open) else { break };
        if pos == 0 {
            break;
        }
        t = trimmed[..pos].trim_end().to_owned();
    }
    t
}

/// Título: `label` si no está vacío, si no el nombre del archivo (tras `#`)
/// sin extensión; en ambos casos sin los grupos finales entre paréntesis.
pub fn title_for(label: &str, path: &str) -> String {
    if !label.trim().is_empty() {
        return clean_title(label);
    }
    let name = consoles::content_file_name(path);
    let stem = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.contains(' ') => stem,
        _ => name,
    };
    clean_title(stem)
}

/// Rutas como las escribe RetroArch: `:` es la carpeta de la aplicación y
/// `~` el home (`fill_pathname_expand_special`), con `\` o `/`.
pub fn expand_special(value: &str, app_dir: &Path, home: Option<&Path>) -> PathBuf {
    let v = value.trim();
    let (base, rest) = if let Some(rest) = v.strip_prefix(':') {
        (app_dir.to_path_buf(), rest)
    } else if let Some(rest) = v.strip_prefix('~') {
        (home.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("~")), rest)
    } else {
        return PathBuf::from(v);
    };
    let mut p = base;
    for part in rest.split(['\\', '/']).filter(|s| !s.is_empty()) {
        p.push(part);
    }
    p
}

fn push_unique(v: &mut Vec<PathBuf>, p: PathBuf) {
    if !v.contains(&p) {
        v.push(p);
    }
}

/// Dónde puede estar el historial, en orden: `content_history_path` del cfg
/// (y `content_history_directory` si no es «default»), `<datos>/playlists/
/// builtin/content_history.lpl` (1.22), `<datos>/content_history.lpl`
/// (anteriores) y lo mismo bajo `playlist_directory`. Sin repetidos.
pub fn history_candidates(cfg_text: &str, data_dir: &Path, app_dir: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    const FILE: &str = "content_history.lpl";
    let mut v = Vec::new();
    if let Some(p) = cfg::get(cfg_text, "content_history_path").filter(|p| !p.trim().is_empty()) {
        push_unique(&mut v, expand_special(&p, app_dir, home));
    }
    if let Some(d) = cfg::get(cfg_text, "content_history_directory").filter(|d| !d.trim().is_empty() && d != "default") {
        push_unique(&mut v, expand_special(&d, app_dir, home).join(FILE));
    }
    push_unique(&mut v, data_dir.join("playlists").join("builtin").join(FILE));
    push_unique(&mut v, data_dir.join(FILE));
    if let Some(d) = cfg::get(cfg_text, "playlist_directory").filter(|d| !d.trim().is_empty() && d != "default") {
        let pl = expand_special(&d, app_dir, home);
        push_unique(&mut v, pl.join("builtin").join(FILE));
        push_unique(&mut v, pl.join(FILE));
    }
    v
}

/// Dónde buscar la ficha del núcleo: `libretro_info_path` del cfg,
/// `<datos>/info`, `<datos>/cores`, la carpeta del núcleo y su `../info`, y
/// en Linux las del sistema.
pub fn info_dirs(cfg_text: &str, data_dir: &Path, app_dir: &Path, core_path: &str, home: Option<&Path>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(d) = cfg::get(cfg_text, "libretro_info_path").filter(|d| !d.trim().is_empty() && d != "default") {
        push_unique(&mut v, expand_special(&d, app_dir, home));
    }
    push_unique(&mut v, data_dir.join("info"));
    push_unique(&mut v, data_dir.join("cores"));
    if let Some(core_dir) = Path::new(core_path).parent().filter(|p| !p.as_os_str().is_empty()) {
        push_unique(&mut v, core_dir.to_path_buf());
        if let Some(parent) = core_dir.parent() {
            push_unique(&mut v, parent.join("info"));
        }
    }
    if cfg!(target_os = "linux") {
        push_unique(&mut v, PathBuf::from("/usr/share/libretro/info"));
        push_unique(&mut v, PathBuf::from("/usr/lib/libretro/info"));
    }
    v
}

/// Lee `<dir>/<núcleo>_libretro.info` en la primera carpeta donde exista.
pub fn find_info(core_path: &str, dirs: &[PathBuf]) -> Option<CoreInfo> {
    let file = format!("{}_libretro.info", consoles::core_stem(core_path));
    dirs.iter()
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| parse_info(&String::from_utf8_lossy(&b)))
}

/// Lo que se anuncia a los móviles (`game`) y enseña la ventana.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GameInfo {
    pub console: Option<Console>,
    /// Nombre de la consola, o `systemname` de la ficha si no tiene plantilla.
    pub system: String,
    pub core: String,
    pub title: String,
    pub path: String,
}

pub fn game_from(entry: &HistoryEntry, info: Option<&CoreInfo>) -> GameInfo {
    let core_file = consoles::content_file_name(&entry.core_path);
    let systemid = info.and_then(|i| i.systemid.as_deref());
    let console = consoles::console_for(systemid, core_file, &entry.path, Some(entry.db_name.as_str()));
    let system = console
        .map(|c| c.name().to_owned())
        .or_else(|| info.and_then(|i| i.systemname.clone()))
        .unwrap_or_default();
    let core = info
        .and_then(|i| i.corename.clone())
        .filter(|c| !c.trim().is_empty())
        .or_else(|| Some(short_core_name(&entry.core_name)).filter(|c| !c.is_empty()))
        .unwrap_or_else(|| consoles::core_stem(&entry.core_path));
    GameInfo { console, system, core, title: title_for(&entry.label, &entry.path), path: entry.path.clone() }
}

/// `{"m":"game",...}`: las cinco claves siempre; sin juego, `console` null y
/// cadenas vacías.
pub fn game_message(game: Option<&GameInfo>) -> Value {
    match game {
        Some(g) => json!({
            "m": "game",
            "console": g.console.map(Console::as_str),
            "system": g.system,
            "core": g.core,
            "title": g.title,
            "path": g.path,
        }),
        None => json!({"m": "game", "console": null, "system": "", "core": "", "title": "", "path": ""}),
    }
}

/// Una instalación de RetroArch: su `retroarch.cfg`, la carpeta de datos
/// (`playlists/`, `info/`) y la «carpeta de la aplicación» que RetroArch
/// abrevia con `:`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DataDir {
    pub cfg: PathBuf,
    pub dir: PathBuf,
    pub app_dir: PathBuf,
}

type Stamp = (SystemTime, u64);

fn stamp(path: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok()?;
    if !m.is_file() {
        return None;
    }
    Some((m.modified().ok()?, m.len()))
}

fn read_lossy(path: &Path) -> String {
    std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// Estado del sondeo (vive en el hilo `emu-watch`).
#[derive(Default)]
pub struct GameWatch {
    /// (archivo elegido, mtime, tamaño) de la última lectura que valió.
    seen: Option<(PathBuf, Stamp)>,
    /// Sello de cada retroarch.cfg: los candidatos solo se recalculan si cambian.
    cfg_seen: Vec<(PathBuf, Option<Stamp>)>,
    candidates: Vec<(DataDir, PathBuf)>,
    was_running: bool,
    last: Option<GameInfo>,
}

impl GameWatch {
    /// Una muestra. `running` = RetroArch abierto ahora. Devuelve si lo que
    /// se anuncia cambió (ver `game()`).
    pub fn poll(&mut self, dirs: &[DataDir], home: Option<&Path>, running: bool) -> bool {
        let stamps: Vec<(PathBuf, Option<Stamp>)> = dirs.iter().map(|d| (d.cfg.clone(), stamp(&d.cfg))).collect();
        if stamps != self.cfg_seen || self.candidates.len() < dirs.len() {
            self.cfg_seen = stamps;
            self.candidates = dirs
                .iter()
                .flat_map(|d| {
                    let text = read_lossy(&d.cfg);
                    history_candidates(&text, &d.dir, &d.app_dir, home)
                        .into_iter()
                        .map(move |p| (d.clone(), p))
                        .collect::<Vec<_>>()
                })
                .collect();
        }
        let was_running = std::mem::replace(&mut self.was_running, running);
        let now = newest(&self.candidates);
        let key = now.as_ref().map(|(_, p, s)| (p.clone(), *s));
        if !needs_read(was_running, running, self.seen.as_ref(), key.as_ref()) {
            return false;
        }
        let new = match &now {
            None => None,
            Some((d, p, _)) => {
                let text = read_lossy(p);
                match parse_history_top(&text) {
                    Err(()) => {
                        // A medias (RetroArch la está escribiendo): se conserva
                        // lo anterior y se reintenta sin dar el archivo por leído
                        return false;
                    }
                    Ok(None) => None,
                    Ok(Some(entry)) => {
                        let cfg_text = read_lossy(&d.cfg);
                        let dirs = info_dirs(&cfg_text, &d.dir, &d.app_dir, &entry.core_path, home);
                        let info = find_info(&entry.core_path, &dirs);
                        Some(game_from(&entry, info.as_ref()))
                    }
                }
            }
        };
        self.seen = key;
        if new != self.last {
            self.last = new;
            true
        } else {
            false
        }
    }

    pub fn game(&self) -> Option<&GameInfo> {
        self.last.as_ref()
    }
}

/// Hay que releer si el archivo (o el elegido) cambió, o si RetroArch acaba
/// de abrirse: la misma partida recargada deja el archivo intacto.
fn needs_read(was_running: bool, running: bool, seen: Option<&(PathBuf, Stamp)>, now: Option<&(PathBuf, Stamp)>) -> bool {
    seen != now || (running && !was_running)
}

/// De los candidatos que existen, el más reciente (varias instalaciones a la
/// vez: nativa y Flatpak, portable y del instalador).
fn newest(candidates: &[(DataDir, PathBuf)]) -> Option<(DataDir, PathBuf, Stamp)> {
    candidates
        .iter()
        .filter_map(|(d, p)| stamp(p).map(|s| (d.clone(), p.clone(), s)))
        .max_by_key(|(_, _, (m, _))| *m)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAVE: &str = r#"{
  "version": "1.5",
  "default_core_path": "",
  "items": [
    {
      "path": "C:\\Users\\DAN\\RetroArch\\roms\\cave_story_v0.7.0.zip",
      "label": "",
      "core_path": "C:\\Users\\DAN\\RetroArch\\cores\\genesis_plus_gx_libretro.dll",
      "core_name": "Sega - MS/GG/MD/CD (Genesis Plus GX)",
      "crc32": "",
      "db_name": "Sega - Game Gear|Sega - Master System - Mark III|Sega - Mega Drive - Genesis"
    },
    { "path": "otro.nes", "core_path": "fceumm_libretro.dll", "core_name": "Nintendo - NES / Famicom (FCEUmm)" }
  ]
}"#;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pepomote-ra-hist-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn lee_la_entrada_mas_reciente_del_historial() {
        let e = parse_history_top(CAVE).unwrap().unwrap();
        assert_eq!(e.path, r"C:\Users\DAN\RetroArch\roms\cave_story_v0.7.0.zip");
        assert_eq!(e.core_path, r"C:\Users\DAN\RetroArch\cores\genesis_plus_gx_libretro.dll");
        assert_eq!(e.core_name, "Sega - MS/GG/MD/CD (Genesis Plus GX)");
        assert!(e.db_name.contains('|'));
        assert_eq!(e.label, "");
        assert_eq!(parse_history_top(r#"{"version":"1.5","items":[]}"#), Ok(None));
        assert_eq!(parse_history_top(r#"{"version":"1.5"}"#), Ok(None));
        assert_eq!(parse_history_top("garbage"), Err(()));
        assert_eq!(parse_history_top(&CAVE[..CAVE.len() / 2]), Err(()), "a medias");
        assert_eq!(parse_history_top(""), Err(()));
    }

    #[test]
    fn titulo_del_archivo_y_nombre_corto_del_nucleo() {
        assert_eq!(title_for("", r"C:\roms\cave_story_v0.7.0.zip"), "cave_story_v0.7.0");
        assert_eq!(title_for("", "pack.zip#Sonic (Europe).md"), "Sonic");
        assert_eq!(title_for("Super Mario World (USA) [!]", "x.sfc"), "Super Mario World");
        assert_eq!(title_for("", "/roms/Zelda, The (U).nes"), "Zelda, The");
        assert_eq!(title_for("", "sin_extension"), "sin_extension");
        assert_eq!(short_core_name("Sega - MS/GG/MD/CD (Genesis Plus GX)"), "Genesis Plus GX");
        assert_eq!(short_core_name("Nintendo - SNES / SFC (Snes9x - Current)"), "Snes9x - Current");
        assert_eq!(short_core_name("DOSBox Pure"), "DOSBox Pure");
    }

    #[test]
    fn rutas_con_prefijo_dos_puntos_y_virgulilla() {
        let app = Path::new(r"C:\RetroArch");
        let home = Path::new(r"C:\Users\DAN");
        let want = app.join("playlists").join("builtin").join("content_history.lpl");
        assert_eq!(expand_special(r":\playlists\builtin\content_history.lpl", app, Some(home)), want);
        assert_eq!(expand_special(":/playlists/builtin/content_history.lpl", app, Some(home)), want);
        assert_eq!(expand_special("~/x.lpl", app, Some(home)), home.join("x.lpl"));
        assert_eq!(expand_special(r"D:\otra\h.lpl", app, Some(home)), PathBuf::from(r"D:\otra\h.lpl"));

        let data = Path::new(r"C:\datos");
        let c = history_candidates("", data, app, Some(home));
        assert_eq!(c, vec![data.join("playlists").join("builtin").join("content_history.lpl"), data.join("content_history.lpl")]);
        let cfg = "content_history_path = \":\\playlists\\builtin\\content_history.lpl\"\nplaylist_directory = \":\\playlists\"\n";
        let c = history_candidates(cfg, app, app, Some(home));
        assert_eq!(
            c,
            vec![want.clone(), app.join("content_history.lpl"), app.join("playlists").join("content_history.lpl")],
            "sin repetidos"
        );
        let cfg = "content_history_directory = \"D:\\hist\"\nplaylist_directory = \"D:\\pl\"\n";
        let c = history_candidates(cfg, data, app, Some(home));
        assert_eq!(c[0], PathBuf::from(r"D:\hist").join("content_history.lpl"));
        assert!(c.contains(&PathBuf::from(r"D:\pl").join("builtin").join("content_history.lpl")));
        assert!(c.contains(&PathBuf::from(r"D:\pl").join("content_history.lpl")));
    }

    #[test]
    fn ficha_del_nucleo() {
        let d = tmp("info");
        let core = d.join("cores").join("genesis_plus_gx_libretro.dll");
        write(&d.join("info").join("genesis_plus_gx_libretro.info"), "display_name = \"Sega - MS/GG/MD/CD (Genesis Plus GX)\"\ncorename = \"Genesis Plus GX\"\nsystemname = \"Sega 8/16-bit (Various)\"\nsystemid = \"mega_drive\"\n");
        let dirs = info_dirs("", &d, &d, &core.to_string_lossy(), None);
        assert_eq!(dirs[0], d.join("info"));
        assert!(dirs.contains(&d.join("cores")));
        let info = find_info(&core.to_string_lossy(), &dirs).unwrap();
        assert_eq!(info.systemid.as_deref(), Some("mega_drive"));
        assert_eq!(info.corename.as_deref(), Some("Genesis Plus GX"));
        assert_eq!(find_info("C:\\x\\dosbox_pure_libretro.dll", &dirs), None);
        // la clave del cfg va primero
        let dirs = info_dirs("libretro_info_path = \":\\fichas\"\n", &d, &d, "", None);
        assert_eq!(dirs[0], d.join("fichas"));
    }

    #[test]
    fn monta_la_ficha_del_juego() {
        let e = parse_history_top(CAVE).unwrap().unwrap();
        let info = CoreInfo { systemid: Some("mega_drive".into()), systemname: Some("Sega 8/16-bit (Various)".into()), corename: Some("Genesis Plus GX".into()) };
        let g = game_from(&e, Some(&info));
        assert_eq!(g.console, Some(Console::Md));
        assert_eq!((g.system.as_str(), g.core.as_str(), g.title.as_str()), ("Mega Drive", "Genesis Plus GX", "cave_story_v0.7.0"));
        assert_eq!(g.path, e.path);
        let g = game_from(&e, None);
        assert_eq!(g.console, Some(Console::Md), "sin ficha: por el nombre del núcleo");
        assert_eq!(g.core, "Genesis Plus GX", "nombre corto del historial");
        let dos = HistoryEntry { path: "juego.zip".into(), core_path: "C:\\c\\dosbox_pure_libretro.dll".into(), core_name: "".into(), ..Default::default() };
        let g = game_from(&dos, None);
        assert_eq!(g.console, None);
        assert_eq!(g.core, "dosbox_pure");
        assert_eq!(g.system, "");
        let m = game_message(Some(&g));
        assert_eq!(m["m"], "game");
        assert!(m["console"].is_null());
        assert_eq!(m["core"], "dosbox_pure");
        let m = game_message(None);
        assert!(m["console"].is_null());
        assert_eq!(m["title"], "");
        assert_eq!(m.as_object().unwrap().len(), 6);
    }

    #[test]
    fn el_sondeo_solo_anuncia_cambios() {
        let d = tmp("watch");
        let cfg = d.join("retroarch.cfg");
        write(&cfg, "history_list_enable = \"true\"\n");
        write(&d.join("info").join("fceumm_libretro.info"), "corename = \"FCEUmm\"\nsystemid = \"nes\"\n");
        let hist = d.join("playlists").join("builtin").join("content_history.lpl");
        let dirs = vec![DataDir { cfg: cfg.clone(), dir: d.clone(), app_dir: d.clone() }];
        let mut w = GameWatch::default();
        assert!(!w.poll(&dirs, None, false), "sin historial: nada (y nada que anunciar)");
        assert_eq!(w.game(), None);
        write(&hist, CAVE);
        assert!(w.poll(&dirs, None, false), "primera lectura");
        assert_eq!(w.game().unwrap().console, Some(Console::Md));
        assert!(!w.poll(&dirs, None, false), "mismo archivo");
        let nes = r#"{"items":[{"path":"C:\\roms\\Zelda.nes","core_path":"C:\\c\\fceumm_libretro.dll","core_name":"x"}]}"#;
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&hist, nes);
        assert!(w.poll(&dirs, None, false), "otra entrada");
        let g = w.game().unwrap();
        assert_eq!((g.console, g.core.as_str(), g.title.as_str()), (Some(Console::Nes), "FCEUmm", "Zelda"));
        // misma entrada con mtime nuevo: se relee pero no se anuncia
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&hist, &format!("{nes}\n"));
        assert!(!w.poll(&dirs, None, false));
        // a medias: se conserva lo anterior y se reintenta
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&hist, &nes[..nes.len() / 2]);
        assert!(!w.poll(&dirs, None, false));
        assert_eq!(w.game().unwrap().console, Some(Console::Nes));
        assert!(w.seen.as_ref().is_some_and(|(_, (_, len))| *len == nes.len() as u64 + 1), "el sello sigue en la última lectura buena");
        write(&hist, CAVE);
        assert!(w.poll(&dirs, None, false), "arreglado: Cave Story otra vez");
        std::fs::remove_file(&hist).unwrap();
        assert!(w.poll(&dirs, None, false), "borrado: sin juego");
        assert_eq!(w.game(), None);
    }

    #[test]
    fn reabrir_retroarch_fuerza_relectura() {
        let p = PathBuf::from("h.lpl");
        let s = (p.clone(), (SystemTime::UNIX_EPOCH, 10));
        assert!(!needs_read(true, true, Some(&s), Some(&s)));
        assert!(!needs_read(false, false, Some(&s), Some(&s)));
        assert!(!needs_read(true, false, Some(&s), Some(&s)), "cerrar no relee");
        assert!(needs_read(false, true, Some(&s), Some(&s)), "abrirse sí");
        let other = (p, (SystemTime::UNIX_EPOCH, 11));
        assert!(needs_read(true, true, Some(&s), Some(&other)));
        assert!(needs_read(true, true, Some(&s), None));
        assert!(!needs_read(true, true, None, None));
    }

    #[test]
    fn la_misma_partida_reabierta_no_se_reanuncia_pero_si_se_relee() {
        let d = tmp("reopen");
        let cfg = d.join("retroarch.cfg");
        write(&cfg, "");
        let hist = d.join("playlists").join("builtin").join("content_history.lpl");
        write(&hist, CAVE);
        let dirs = vec![DataDir { cfg, dir: d.clone(), app_dir: d.clone() }];
        let mut w = GameWatch::default();
        assert!(w.poll(&dirs, None, true));
        assert!(!w.poll(&dirs, None, false), "RetroArch se cierra: se conserva el juego");
        assert_eq!(w.game().unwrap().console, Some(Console::Md));
        assert!(!w.poll(&dirs, None, true), "se reabre con la misma partida: releído, sin cambio");
    }
}
