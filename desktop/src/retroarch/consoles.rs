//! Qué mando enseña el móvil para cada consola de RetroArch: de la ficha del
//! núcleo (`systemid`), del nombre del núcleo si no hay ficha, y de la
//! extensión del contenido en los núcleos multisistema (Genesis Plus GX juega
//! Master System y Mega Drive; mGBA juega Game Boy y Advance). Los ids son
//! los de `pmp::retro::CONSOLE_IDS`, que es lo que entiende el móvil.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Console {
    Nes,
    Gb,
    Gba,
    Snes,
    Ms,
    Md,
    Pce,
    Arcade,
    NeoGeo,
    Atari2600,
    N64,
    Psx,
}

impl Console {
    pub const ALL: [Console; 12] = [
        Console::Nes,
        Console::Gb,
        Console::Gba,
        Console::Snes,
        Console::Ms,
        Console::Md,
        Console::Pce,
        Console::Arcade,
        Console::NeoGeo,
        Console::Atari2600,
        Console::N64,
        Console::Psx,
    ];

    /// Id del protocolo (`game.console`, `pad.layout`).
    pub fn as_str(self) -> &'static str {
        match self {
            Console::Nes => "nes",
            Console::Gb => "gb",
            Console::Gba => "gba",
            Console::Snes => "snes",
            Console::Ms => "ms",
            Console::Md => "md",
            Console::Pce => "pce",
            Console::Arcade => "arcade",
            Console::NeoGeo => "neogeo",
            Console::Atari2600 => "atari2600",
            Console::N64 => "n64",
            Console::Psx => "psx",
        }
    }

    pub fn parse(s: &str) -> Option<Console> {
        Console::ALL.iter().copied().find(|c| c.as_str() == s)
    }

    /// Nombre propio, igual en los dos idiomas (por eso no va en strings.rs).
    pub fn name(self) -> &'static str {
        match self {
            Console::Nes => "NES",
            Console::Gb => "Game Boy",
            Console::Gba => "Game Boy Advance",
            Console::Snes => "SNES",
            Console::Ms => "Master System",
            Console::Md => "Mega Drive",
            Console::Pce => "PC Engine",
            Console::Arcade => "Arcade",
            Console::NeoGeo => "Neo Geo",
            Console::Atari2600 => "Atari 2600",
            Console::N64 => "Nintendo 64",
            Console::Psx => "PlayStation",
        }
    }
}

/// Nombre del archivo (tras la última barra) y, si la ruta lleva `#`
/// (miembro de un archivo comprimido: `pack.zip#Sonic.md`), el del miembro.
pub fn content_file_name(path: &str) -> &str {
    let after_hash = path.rsplit('#').next().unwrap_or(path);
    after_hash.rsplit(['\\', '/']).next().unwrap_or(after_hash)
}

/// Extensión del contenido en minúsculas y sin punto.
pub fn content_ext(path: &str) -> Option<String> {
    let name = content_file_name(path);
    let (_, ext) = name.rsplit_once('.')?;
    if ext.is_empty() || ext.contains(' ') {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Extensiones que solo pueden ser de una consola (las ambiguas, como
/// `.bin`, `.zip` o `.cue`, no deciden).
fn by_ext(ext: &str) -> Option<Console> {
    Some(match ext {
        "nes" | "fds" | "unf" | "unif" => Console::Nes,
        "gb" | "gbc" | "dmg" | "cgb" | "sgb" => Console::Gb,
        "gba" => Console::Gba,
        "sfc" | "smc" | "swc" | "fig" | "bs" | "st" => Console::Snes,
        "sms" | "gg" | "sg" | "sc" => Console::Ms,
        "md" | "smd" | "gen" | "mdx" | "68k" | "sgd" | "32x" => Console::Md,
        "pce" | "sgx" => Console::Pce,
        "a26" => Console::Atari2600,
        "n64" | "v64" | "z64" | "ndd" => Console::N64,
        "neo" => Console::NeoGeo,
        _ => return None,
    })
}

fn by_systemid(id: &str) -> Option<Console> {
    Some(match id {
        "nes" => Console::Nes,
        "game_boy" => Console::Gb,
        "game_boy_advance" => Console::Gba,
        "super_nes" => Console::Snes,
        "master_system" => Console::Ms,
        "mega_drive" => Console::Md,
        "pc_engine" => Console::Pce,
        "mame" | "fb_alpha" => Console::Arcade,
        "neogeo" | "neo_geo" => Console::NeoGeo,
        "atari_2600" => Console::Atari2600,
        "nintendo_64" => Console::N64,
        "playstation" => Console::Psx,
        _ => return None,
    })
}

/// `genesis_plus_gx_libretro.dll` → `genesis_plus_gx` (también `.so`,
/// `.dylib` y el sufijo `_libretro_android`), en minúsculas.
pub fn core_stem(core_path: &str) -> String {
    let name = core_path.rsplit(['\\', '/']).next().unwrap_or(core_path).to_ascii_lowercase();
    let name = name
        .strip_suffix(".dll")
        .or_else(|| name.strip_suffix(".so"))
        .or_else(|| name.strip_suffix(".dylib"))
        .unwrap_or(&name);
    let name = name
        .strip_suffix("_libretro_android")
        .or_else(|| name.strip_suffix("_libretro"))
        .unwrap_or(name);
    name.to_owned()
}

/// Núcleo sin ficha: por el nombre de su archivo. Los prefijos van del más
/// específico al más general (`mesen-s` antes que `mesen`).
fn by_core_name(core_file: &str) -> Option<Console> {
    let stem = core_stem(core_file);
    if stem.contains("neogeo") || stem.contains("neocd") || stem.contains("geolith") {
        return Some(Console::NeoGeo);
    }
    const PREFIXES: &[(&str, Console)] = &[
        ("mesen-s", Console::Snes),
        ("mesen_s", Console::Snes),
        ("snes9x", Console::Snes),
        ("bsnes", Console::Snes),
        ("higan_sfc", Console::Snes),
        ("chimerasnes", Console::Snes),
        ("mednafen_snes", Console::Snes),
        ("beetle_snes", Console::Snes),
        ("supafaust", Console::Snes),
        ("mednafen_supafaust", Console::Snes),
        ("fceumm", Console::Nes),
        ("nestopia", Console::Nes),
        ("mesen", Console::Nes),
        ("quicknes", Console::Nes),
        ("bnes", Console::Nes),
        ("fixnes", Console::Nes),
        ("emux_nes", Console::Nes),
        ("gambatte", Console::Gb),
        ("sameboy", Console::Gb),
        ("tgbdual", Console::Gb),
        ("gearboy", Console::Gb),
        ("doublecherrygb", Console::Gb),
        ("boytacean", Console::Gb),
        ("fixgb", Console::Gb),
        ("emux_gb", Console::Gb),
        ("mgba", Console::Gba),
        ("vbam", Console::Gba),
        ("vba_next", Console::Gba),
        ("gpsp", Console::Gba),
        ("mednafen_gba", Console::Gba),
        ("beetle_gba", Console::Gba),
        ("meteor", Console::Gba),
        ("tempgba", Console::Gba),
        ("genesis_plus_gx", Console::Md),
        ("picodrive", Console::Md),
        ("blastem", Console::Md),
        ("clownmdemu", Console::Md),
        ("smsplus", Console::Ms),
        ("gearsystem", Console::Ms),
        ("emux_sms", Console::Ms),
        ("mednafen_pce", Console::Pce),
        ("beetle_pce", Console::Pce),
        ("mednafen_supergrafx", Console::Pce),
        ("beetle_supergrafx", Console::Pce),
        ("geargrafx", Console::Pce),
        ("mupen64plus", Console::N64),
        ("parallel_n64", Console::N64),
        ("pcsx", Console::Psx),
        ("swanstation", Console::Psx),
        ("duckstation", Console::Psx),
        ("mednafen_psx", Console::Psx),
        ("beetle_psx", Console::Psx),
        ("rustation", Console::Psx),
        ("stella", Console::Atari2600),
        ("fbneo", Console::Arcade),
        ("fbalpha", Console::Arcade),
        ("fba", Console::Arcade),
        ("mame", Console::Arcade),
    ];
    PREFIXES.iter().find(|(p, _)| stem.starts_with(p)).map(|(_, c)| *c)
}

/// `db_name` del historial cuando es UNA base de datos (sin `|`), p. ej.
/// «Sega - Mega Drive - Genesis.lpl»: la pista más fiable con `.bin`/`.cue`.
pub fn by_db_name(db: &str) -> Option<Console> {
    if db.is_empty() || db.contains('|') {
        return None;
    }
    const HINTS: &[(&str, Console)] = &[
        ("Game Boy Advance", Console::Gba),
        ("Game Boy", Console::Gb),
        ("Mega Drive", Console::Md),
        ("32X", Console::Md),
        ("Mega-CD", Console::Md),
        ("Master System", Console::Ms),
        ("Game Gear", Console::Ms),
        ("SG-1000", Console::Ms),
        ("Super Nintendo", Console::Snes),
        ("Satellaview", Console::Snes),
        ("Sufami", Console::Snes),
        ("Nintendo Entertainment System", Console::Nes),
        ("Family Computer", Console::Nes),
        ("PC Engine", Console::Pce),
        ("TurboGrafx", Console::Pce),
        ("SuperGrafx", Console::Pce),
        ("Neo Geo", Console::NeoGeo),
        ("FBNeo", Console::Arcade),
        ("MAME", Console::Arcade),
        ("Arcade", Console::Arcade),
        ("Atari - 2600", Console::Atari2600),
        ("Nintendo 64", Console::N64),
        ("PlayStation", Console::Psx),
    ];
    HINTS.iter().find(|(h, _)| db.contains(h)).map(|(_, c)| *c)
}

/// Consola del contenido. Orden: extensión inequívoca > `db_name` único >
/// núcleo Neo Geo por nombre > `systemid` de la ficha > nombre del núcleo.
pub fn console_for(systemid: Option<&str>, core_file: &str, content_path: &str, db_name: Option<&str>) -> Option<Console> {
    if let Some(c) = content_ext(content_path).and_then(|e| by_ext(&e)) {
        return Some(c);
    }
    if let Some(c) = db_name.and_then(by_db_name) {
        return Some(c);
    }
    let by_name = by_core_name(core_file);
    if by_name == Some(Console::NeoGeo) {
        return by_name;
    }
    systemid.and_then(by_systemid).or(by_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_del_protocolo() {
        assert_eq!(Console::ALL.len(), 12);
        for c in Console::ALL {
            assert_eq!(Console::parse(c.as_str()), Some(c));
            assert!(!c.name().is_empty());
            assert!(pmp::retro::CONSOLE_IDS.contains(&c.as_str()), "{} no está en pmp::retro", c.as_str());
        }
        assert_eq!(pmp::retro::CONSOLE_IDS.len(), Console::ALL.len());
        assert_eq!(Console::parse("snes9x"), None);
    }

    #[test]
    fn por_systemid() {
        assert_eq!(console_for(Some("super_nes"), "snes9x_libretro.dll", "juego.zip", None), Some(Console::Snes));
        assert_eq!(console_for(Some("playstation"), "x.dll", "juego.cue", None), Some(Console::Psx));
        assert_eq!(console_for(Some("dos"), "dosbox_pure_libretro.dll", "juego.zip", None), None);
    }

    #[test]
    fn por_nombre_de_nucleo_sin_ficha() {
        assert_eq!(by_core_name("mesen-s_libretro.dll"), Some(Console::Snes));
        assert_eq!(by_core_name("mesen_libretro.dll"), Some(Console::Nes));
        assert_eq!(by_core_name("fbneo_neogeo_libretro.so"), Some(Console::NeoGeo));
        assert_eq!(by_core_name("mame2003_plus_libretro.dll"), Some(Console::Arcade));
        assert_eq!(by_core_name("/opt/x/stella2014_libretro.dylib"), Some(Console::Atari2600));
        assert_eq!(by_core_name("dosbox_pure_libretro.dll"), None);
        assert_eq!(core_stem("C:\\RetroArch\\cores\\genesis_plus_gx_libretro.dll"), "genesis_plus_gx");
        assert_eq!(core_stem("/data/mgba_libretro_android.so"), "mgba");
        assert_eq!(console_for(None, "C:\\x\\picodrive_libretro.dll", "juego.zip", None), Some(Console::Md));
        // Neo Geo por nombre manda sobre el systemid arcade de la ficha
        assert_eq!(console_for(Some("fb_alpha"), "fbneo_neogeo_libretro.dll", "mslug.zip", None), Some(Console::NeoGeo));
    }

    #[test]
    fn la_extension_manda_en_nucleos_multisistema() {
        let gpgx = "genesis_plus_gx_libretro.dll";
        assert_eq!(console_for(Some("mega_drive"), gpgx, "C:\\roms\\sonic.sms", None), Some(Console::Ms));
        assert_eq!(console_for(Some("mega_drive"), gpgx, "sonic.GG", None), Some(Console::Ms));
        assert_eq!(console_for(Some("mega_drive"), gpgx, "sonic.md", None), Some(Console::Md));
        assert_eq!(console_for(Some("mega_drive"), gpgx, "cave_story.zip", None), Some(Console::Md));
        assert_eq!(console_for(Some("mega_drive"), gpgx, "juego.cue", None), Some(Console::Md));
        assert_eq!(console_for(Some("game_boy_advance"), "mgba_libretro.dll", "tetris.gb", None), Some(Console::Gb));
        assert_eq!(console_for(Some("game_boy_advance"), "mgba_libretro.dll", "tetris.gba", None), Some(Console::Gba));
        assert_eq!(console_for(Some("super_nes"), "x.dll", "tetris.gb", None), Some(Console::Gb));
        assert_eq!(console_for(None, "raro.dll", "zelda.nes", None), Some(Console::Nes));
        assert_eq!(console_for(None, "raro.dll", "juego.bin", None), None);
    }

    #[test]
    fn miembro_de_archivo_tras_almohadilla() {
        assert_eq!(content_file_name("C:\\roms\\pack.zip#Sonic (Europe).md"), "Sonic (Europe).md");
        assert_eq!(content_ext("pack.zip#Sonic (Europe).md").as_deref(), Some("md"));
        assert_eq!(content_ext("C:\\roms\\Sonic.MD").as_deref(), Some("md"));
        assert_eq!(content_ext("sin_extension"), None);
        assert_eq!(console_for(Some("mega_drive"), "genesis_plus_gx_libretro.dll", "pack.7z#rom.bin", None), Some(Console::Md));
        assert_eq!(console_for(None, "x.dll", "pack.zip#Tetris.gb", None), Some(Console::Gb));
    }

    #[test]
    fn db_name_unico_como_pista() {
        assert_eq!(by_db_name("Sega - Mega Drive - Genesis.lpl"), Some(Console::Md));
        assert_eq!(by_db_name("Nintendo - Game Boy Advance.lpl"), Some(Console::Gba));
        assert_eq!(by_db_name("Nintendo - Game Boy.lpl"), Some(Console::Gb));
        assert_eq!(by_db_name("Sega - Game Gear|Sega - Master System - Mark III"), None, "varias: no decide");
        assert_eq!(by_db_name(""), None);
        assert_eq!(
            console_for(Some("mega_drive"), "genesis_plus_gx_libretro.dll", "juego.bin", Some("Sega - Master System - Mark III.lpl")),
            Some(Console::Ms)
        );
    }
}
