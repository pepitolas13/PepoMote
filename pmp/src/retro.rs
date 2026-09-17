//! Plantillas de mando de RetroArch por consola: qué botones enseña el móvil,
//! con qué etiqueta y color, y qué bit del `INPUT` emite cada uno. El receptor
//! traduce los bits al RetroPad como siempre (A→A, B→B, X→X, Y→Y, L→L, R→R,
//! ZL→L2, ZR→R2, clics de stick→L3/R3, +→Start, −→Select); cada botón en
//! pantalla emite el bit cuyo id del RetroPad espera el mapeo por defecto del
//! núcleo (docs.libretro.com; FBNeo por su `retro_input.cpp`). Fijos en toda
//! plantilla y fuera de la tabla: la cruceta, «Menú» (BTN_HOME) y «Rápido»
//! (BTN_SCREEN).
//!
//! Fuente de verdad de las tres apps: el móvil Linux la usa tal cual y Android
//! e iOS llevan la tabla a mano con un test de paridad contra
//! `protocol/retro-layouts.json`, que se regenera con
//! `cargo run --example write_layouts -- ../protocol/retro-layouts.json`.
use crate::{
    BTN_A, BTN_B, BTN_L, BTN_MINUS, BTN_PLUS, BTN_R, BTN_STICK_L, BTN_STICK_R, BTN_X, BTN_Y, BTN_ZL, BTN_ZR,
};

/// Disposición de los botones frontales (los huecos de cada forma).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// Un botón: Atari 2600.
    Single,
    /// Dos en fila (`left`, `right`), con `stagger`: NES, Game Boy, Master System, PC Engine, N64.
    Two,
    /// Rombo `top`/`left`/`right`/`bottom`: RetroPad, SNES, Neo Geo, PlayStation.
    Diamond,
    /// Dos filas de tres `tl tm tr` / `bl bm br`: Mega Drive de 6 botones, arcade.
    Grid2x3,
    /// Tres en fila `l m r`: Mega Drive de 3 botones.
    Row3,
}

impl Shape {
    pub fn as_str(self) -> &'static str {
        match self {
            Shape::Single => "single",
            Shape::Two => "two",
            Shape::Diamond => "diamond",
            Shape::Grid2x3 => "grid2x3",
            Shape::Row3 => "row3",
        }
    }

    pub fn slots(self) -> &'static [&'static str] {
        match self {
            Shape::Single => &["center"],
            Shape::Two => &["left", "right"],
            Shape::Diamond => &["top", "left", "right", "bottom"],
            Shape::Grid2x3 => &["tl", "tm", "tr", "bl", "bm", "br"],
            Shape::Row3 => &["l", "m", "r"],
        }
    }
}

/// Qué va en el hueco del stick derecho.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RightStick {
    None,
    Analog,
    /// N64: cuatro botones C que emiten los ejes del stick derecho a ±127.
    CButtons,
}

impl RightStick {
    pub fn as_str(self) -> &'static str {
        match self {
            RightStick::None => "none",
            RightStick::Analog => "analog",
            RightStick::CButtons => "cbuttons",
        }
    }
}

/// Color del botón; cada app lo lleva a su tema (`card` = el de siempre).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Card,
    Red,
    Yellow,
    Green,
    Blue,
    Pink,
    Purple,
}

impl Color {
    pub fn as_str(self) -> &'static str {
        match self {
            Color::Card => "card",
            Color::Red => "red",
            Color::Yellow => "yellow",
            Color::Green => "green",
            Color::Blue => "blue",
            Color::Pink => "pink",
            Color::Purple => "purple",
        }
    }
}

/// Un botón frontal. `label` es el texto tal cual salvo `$coin` y `$fire`,
/// que cada app traduce («Moneda»/«Coin», «Disparo»/«Fire»). `primary`: el
/// azul destacado de siempre (con «pop»). `size`: relativo al botón normal.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FaceButton {
    pub slot: &'static str,
    pub label: &'static str,
    pub bit: u32,
    pub color: Color,
    pub primary: bool,
    pub size: f32,
}

/// Un botón de la fila central (Select/Start y sus nombres en cada consola).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CenterButton {
    pub label: &'static str,
    pub bit: u32,
}

/// Etiquetas de los hombros; `None` = ese hombro no existe en la consola.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shoulders {
    pub l: Option<&'static str>,
    pub r: Option<&'static str>,
    pub l2: Option<&'static str>,
    pub r2: Option<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Layout {
    pub id: &'static str,
    /// Nombre propio, igual en los dos idiomas.
    pub name: &'static str,
    pub shape: Shape,
    /// Solo `Two`: −1 el hueco izquierdo más alto (N64), 0 a nivel, 1 más bajo (Game Boy).
    pub stagger: i8,
    pub face: &'static [FaceButton],
    pub left_stick: bool,
    pub right_stick: RightStick,
    /// L3/R3 (solo con sticks analógicos).
    pub stick_clicks: bool,
    pub shoulders: Shoulders,
    pub center: &'static [CenterButton],
}

const fn fb(slot: &'static str, label: &'static str, bit: u32, color: Color, primary: bool, size: f32) -> FaceButton {
    FaceButton { slot, label, bit, color, primary, size }
}

const fn cb(label: &'static str, bit: u32) -> CenterButton {
    CenterButton { label, bit }
}

const NO_SHOULDERS: Shoulders = Shoulders { l: None, r: None, l2: None, r2: None };
const LR: Shoulders = Shoulders { l: Some("L"), r: Some("R"), l2: None, r2: None };
const SELECT_START: &[CenterButton] = &[cb("Select", BTN_MINUS), cb("Start", BTN_PLUS)];
const MODE_START: &[CenterButton] = &[cb("Mode", BTN_MINUS), cb("Start", BTN_PLUS)];
const COIN_START: &[CenterButton] = &[cb("$coin", BTN_MINUS), cb("Start", BTN_PLUS)];

/// El RetroPad completo: lo de hoy, y lo que se enseña cuando no se sabe la consola.
pub const RETROPAD: Layout = Layout {
    id: "retropad",
    name: "RetroPad",
    shape: Shape::Diamond,
    stagger: 0,
    face: &[
        fb("top", "X", BTN_X, Color::Card, false, 1.0),
        fb("left", "Y", BTN_Y, Color::Card, false, 1.0),
        fb("right", "A", BTN_A, Color::Card, true, 1.0),
        fb("bottom", "B", BTN_B, Color::Card, false, 1.0),
    ],
    left_stick: true,
    right_stick: RightStick::Analog,
    stick_clicks: true,
    shoulders: Shoulders { l: Some("L"), r: Some("R"), l2: Some("L2"), r2: Some("R2") },
    center: SELECT_START,
};

/// Todas las plantillas, con `retropad` la primera.
pub const LAYOUTS: &[Layout] = &[
    RETROPAD,
    // FCEUmm / Nestopia: A→A, B→B
    Layout {
        id: "nes",
        name: "NES",
        shape: Shape::Two,
        stagger: 0,
        face: &[fb("left", "B", BTN_B, Color::Red, false, 1.0), fb("right", "A", BTN_A, Color::Red, true, 1.0)],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: NO_SHOULDERS,
        center: SELECT_START,
    },
    // Gambatte / SameBoy: A→A, B→B (Game Boy y Color)
    Layout {
        id: "gb",
        name: "Game Boy",
        shape: Shape::Two,
        stagger: 1,
        face: &[fb("left", "B", BTN_B, Color::Purple, false, 1.0), fb("right", "A", BTN_A, Color::Purple, true, 1.0)],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: NO_SHOULDERS,
        center: SELECT_START,
    },
    // mGBA: A→A, B→B, L→L, R→R
    Layout {
        id: "gba",
        name: "Game Boy Advance",
        shape: Shape::Two,
        stagger: 1,
        face: &[fb("left", "B", BTN_B, Color::Card, false, 1.0), fb("right", "A", BTN_A, Color::Card, true, 1.0)],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: LR,
        center: SELECT_START,
    },
    // Snes9x: identidad (colores del mando PAL/Super Famicom)
    Layout {
        id: "snes",
        name: "SNES",
        shape: Shape::Diamond,
        stagger: 0,
        face: &[
            fb("top", "X", BTN_X, Color::Blue, false, 1.0),
            fb("left", "Y", BTN_Y, Color::Green, false, 1.0),
            fb("right", "A", BTN_A, Color::Red, true, 1.0),
            fb("bottom", "B", BTN_B, Color::Yellow, false, 1.0),
        ],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: LR,
        center: SELECT_START,
    },
    // Genesis Plus GX / PicoDrive (Master System, Game Gear): 1→B, 2→A, Pause→Start
    Layout {
        id: "ms",
        name: "Master System",
        shape: Shape::Two,
        stagger: 0,
        face: &[fb("left", "1", BTN_B, Color::Card, true, 1.0), fb("right", "2", BTN_A, Color::Card, false, 1.0)],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: NO_SHOULDERS,
        center: &[cb("Pause", BTN_PLUS)],
    },
    // Genesis Plus GX / PicoDrive: A→Y, B→B, C→A, X→L, Y→X, Z→R, Mode→Select
    Layout {
        id: "md",
        name: "Mega Drive",
        shape: Shape::Grid2x3,
        stagger: 0,
        face: &[
            fb("tl", "X", BTN_L, Color::Card, false, 0.8),
            fb("tm", "Y", BTN_X, Color::Card, false, 0.8),
            fb("tr", "Z", BTN_R, Color::Card, false, 0.8),
            fb("bl", "A", BTN_Y, Color::Card, false, 1.0),
            fb("bm", "B", BTN_B, Color::Card, false, 1.0),
            fb("br", "C", BTN_A, Color::Card, true, 1.0),
        ],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: NO_SHOULDERS,
        center: MODE_START,
    },
    // El mando de 3 botones, solo a mano
    Layout {
        id: "md3",
        name: "Mega Drive (3)",
        shape: Shape::Row3,
        stagger: 0,
        face: &[
            fb("l", "A", BTN_Y, Color::Card, false, 1.0),
            fb("m", "B", BTN_B, Color::Card, false, 1.0),
            fb("r", "C", BTN_A, Color::Card, true, 1.0),
        ],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: NO_SHOULDERS,
        center: MODE_START,
    },
    // Beetle PCE: I→A, II→B, Run→Start
    Layout {
        id: "pce",
        name: "PC Engine",
        shape: Shape::Two,
        stagger: 0,
        face: &[fb("left", "II", BTN_B, Color::Card, false, 1.0), fb("right", "I", BTN_A, Color::Card, true, 1.0)],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: NO_SHOULDERS,
        center: &[cb("Select", BTN_MINUS), cb("Run", BTN_PLUS)],
    },
    // FBNeo (dispositivo «Classic») y MAME: 1..6 → B A Y X R L; en lucha LP MP HP = Y X L, LK MK HK = B A R
    Layout {
        id: "arcade",
        name: "Arcade",
        shape: Shape::Grid2x3,
        stagger: 0,
        face: &[
            fb("tl", "Y", BTN_Y, Color::Card, false, 1.0),
            fb("tm", "X", BTN_X, Color::Card, false, 1.0),
            fb("tr", "L", BTN_L, Color::Card, false, 1.0),
            fb("bl", "B", BTN_B, Color::Card, true, 1.0),
            fb("bm", "A", BTN_A, Color::Card, false, 1.0),
            fb("br", "R", BTN_R, Color::Card, false, 1.0),
        ],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: NO_SHOULDERS,
        center: COIN_START,
    },
    // FBNeo Neo Geo («Classic»): A→B, B→A, C→Y, D→X, en el rombo del mando de Neo Geo CD
    Layout {
        id: "neogeo",
        name: "Neo Geo",
        shape: Shape::Diamond,
        stagger: 0,
        face: &[
            fb("top", "D", BTN_X, Color::Blue, false, 1.0),
            fb("left", "C", BTN_Y, Color::Green, false, 1.0),
            fb("right", "B", BTN_A, Color::Yellow, false, 1.0),
            fb("bottom", "A", BTN_B, Color::Red, true, 1.0),
        ],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: NO_SHOULDERS,
        center: COIN_START,
    },
    // Stella: disparo→B, Select→Select, Reset→Start
    Layout {
        id: "atari2600",
        name: "Atari 2600",
        shape: Shape::Single,
        stagger: 0,
        face: &[fb("center", "$fire", BTN_B, Color::Red, true, 1.0)],
        left_stick: false,
        right_stick: RightStick::None,
        stick_clicks: false,
        shoulders: NO_SHOULDERS,
        center: &[cb("Select", BTN_MINUS), cb("Reset", BTN_PLUS)],
    },
    // Mupen64Plus: A→B, B→Y, Z→L2, L→L, R→R; botones C = stick derecho
    Layout {
        id: "n64",
        name: "Nintendo 64",
        shape: Shape::Two,
        stagger: -1,
        face: &[fb("left", "B", BTN_Y, Color::Green, false, 0.9), fb("right", "A", BTN_B, Color::Blue, true, 1.2)],
        left_stick: true,
        right_stick: RightStick::CButtons,
        stick_clicks: false,
        shoulders: Shoulders { l: Some("L"), r: Some("R"), l2: Some("Z"), r2: None },
        center: &[cb("Start", BTN_PLUS)],
    },
    // PCSX-ReARMed / Beetle PSX: ✕→B, ○→A, □→Y, △→X, L1/R1/L2/R2, L3/R3
    Layout {
        id: "psx",
        name: "PlayStation",
        shape: Shape::Diamond,
        stagger: 0,
        face: &[
            fb("top", "△", BTN_X, Color::Green, false, 1.0),
            fb("left", "□", BTN_Y, Color::Pink, false, 1.0),
            fb("right", "○", BTN_A, Color::Red, false, 1.0),
            fb("bottom", "✕", BTN_B, Color::Blue, true, 1.0),
        ],
        left_stick: true,
        right_stick: RightStick::Analog,
        stick_clicks: true,
        shoulders: Shoulders { l: Some("L1"), r: Some("R1"), l2: Some("L2"), r2: Some("R2") },
        center: SELECT_START,
    },
];

/// Las consolas que el receptor puede anunciar en `game.console` (todas las
/// plantillas menos `retropad` y `md3`, que solo se eligen a mano).
pub const CONSOLE_IDS: &[&str] = &["nes", "gb", "gba", "snes", "ms", "md", "pce", "arcade", "neogeo", "atari2600", "n64", "psx"];

/// Ids válidos en `pad.layout` (todas las plantillas).
pub const LAYOUT_IDS: &[&str] = &["retropad", "nes", "gb", "gba", "snes", "ms", "md", "md3", "pce", "arcade", "neogeo", "atari2600", "n64", "psx"];

pub fn layout(id: &str) -> Option<&'static Layout> {
    LAYOUTS.iter().find(|l| l.id == id)
}

pub fn is_console(id: &str) -> bool {
    CONSOLE_IDS.contains(&id)
}

/// Qué plantilla se enseña: la elegida a mano si es válida, si no la consola
/// que anunció el receptor, si no el RetroPad completo.
pub fn effective_layout(console: Option<&str>, chosen: Option<&str>) -> &'static str {
    if let Some(c) = chosen.filter(|c| LAYOUT_IDS.contains(c)) {
        return layout(c).map_or("retropad", |l| l.id);
    }
    match console {
        Some(c) if is_console(c) => layout(c).map_or("retropad", |l| l.id),
        _ => "retropad",
    }
}

/// Nombre del bit para el JSON (los que puede emitir un botón de plantilla).
pub const BITS: &[(&str, u32)] = &[
    ("A", BTN_A),
    ("B", BTN_B),
    ("X", BTN_X),
    ("Y", BTN_Y),
    ("L", BTN_L),
    ("R", BTN_R),
    ("ZL", BTN_ZL),
    ("ZR", BTN_ZR),
    ("STICK_L", BTN_STICK_L),
    ("STICK_R", BTN_STICK_R),
    ("PLUS", BTN_PLUS),
    ("MINUS", BTN_MINUS),
];

pub fn bit_name(bit: u32) -> Option<&'static str> {
    BITS.iter().find(|(_, b)| *b == bit).map(|(n, _)| *n)
}

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn json_opt(out: &mut String, s: Option<&str>) {
    match s {
        Some(s) => json_str(out, s),
        None => out.push_str("null"),
    }
}

/// La tabla como JSON determinista (`protocol/retro-layouts.json`), con los
/// valores de los bits para que Android e iOS comprueben también sus constantes.
pub fn to_json() -> String {
    let mut o = String::new();
    o.push_str("{\n  \"version\": 1,\n  \"bits\": {");
    for (i, (name, bit)) in BITS.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push(' ');
        json_str(&mut o, name);
        o.push_str(&format!(": {bit}"));
    }
    o.push_str(" },\n  \"console_ids\": [");
    for (i, id) in CONSOLE_IDS.iter().enumerate() {
        if i > 0 {
            o.push_str(", ");
        }
        json_str(&mut o, id);
    }
    o.push_str("],\n  \"layouts\": [\n");
    for (i, l) in LAYOUTS.iter().enumerate() {
        if i > 0 {
            o.push_str(",\n");
        }
        o.push_str("    {\n      \"id\": ");
        json_str(&mut o, l.id);
        o.push_str(",\n      \"name\": ");
        json_str(&mut o, l.name);
        o.push_str(",\n      \"shape\": ");
        json_str(&mut o, l.shape.as_str());
        o.push_str(&format!(",\n      \"stagger\": {},\n      \"face\": [\n", l.stagger));
        for (j, f) in l.face.iter().enumerate() {
            if j > 0 {
                o.push_str(",\n");
            }
            o.push_str("        { \"slot\": ");
            json_str(&mut o, f.slot);
            o.push_str(", \"label\": ");
            json_str(&mut o, f.label);
            o.push_str(", \"bit\": ");
            json_str(&mut o, bit_name(f.bit).unwrap_or("?"));
            o.push_str(", \"color\": ");
            json_str(&mut o, f.color.as_str());
            o.push_str(&format!(", \"primary\": {}, \"size\": {:.2} }}", f.primary, f.size));
        }
        o.push_str(&format!(
            "\n      ],\n      \"left_stick\": {},\n      \"right_stick\": \"{}\",\n      \"stick_clicks\": {},\n      \"shoulders\": {{ \"l\": ",
            l.left_stick,
            l.right_stick.as_str(),
            l.stick_clicks
        ));
        json_opt(&mut o, l.shoulders.l);
        o.push_str(", \"r\": ");
        json_opt(&mut o, l.shoulders.r);
        o.push_str(", \"l2\": ");
        json_opt(&mut o, l.shoulders.l2);
        o.push_str(", \"r2\": ");
        json_opt(&mut o, l.shoulders.r2);
        o.push_str(" },\n      \"center\": [");
        for (j, c) in l.center.iter().enumerate() {
            if j > 0 {
                o.push(',');
            }
            o.push_str(" { \"label\": ");
            json_str(&mut o, c.label);
            o.push_str(", \"bit\": ");
            json_str(&mut o, bit_name(c.bit).unwrap_or("?"));
            o.push_str(" }");
        }
        o.push_str(" ]\n    }");
    }
    o.push_str("\n  ]\n}\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_unicos_y_listas_coherentes() {
        let ids: Vec<&str> = LAYOUTS.iter().map(|l| l.id).collect();
        for (i, id) in ids.iter().enumerate() {
            assert!(!ids[..i].contains(id), "id repetido: {id}");
            assert!(LAYOUT_IDS.contains(id), "{id} no está en LAYOUT_IDS");
        }
        assert_eq!(LAYOUT_IDS.len(), LAYOUTS.len());
        for c in CONSOLE_IDS {
            assert!(layout(c).is_some(), "consola sin plantilla: {c}");
        }
        assert!(!is_console("retropad") && !is_console("md3"));
        assert_eq!(LAYOUTS[0].id, "retropad");
        assert_eq!(LAYOUTS.len(), 14);
    }

    #[test]
    fn cada_plantilla_es_coherente() {
        for l in LAYOUTS {
            let slots = l.shape.slots();
            assert_eq!(l.face.len(), slots.len(), "{}: huecos", l.id);
            for f in l.face {
                assert!(slots.contains(&f.slot), "{}: hueco {} no es de {:?}", l.id, f.slot, l.shape);
                assert!(bit_name(f.bit).is_some(), "{}: bit sin nombre en {}", l.id, f.label);
                assert!(f.size > 0.5 && f.size < 1.5, "{}: tamaño raro", l.id);
            }
            let bits: Vec<u32> = l.face.iter().map(|f| f.bit).collect();
            for (i, b) in bits.iter().enumerate() {
                assert!(!bits[..i].contains(b), "{}: bit repetido {:#x}", l.id, b);
            }
            assert!(l.face.iter().filter(|f| f.primary).count() <= 1, "{}: más de un primario", l.id);
            assert!(!l.stick_clicks || (l.left_stick && l.right_stick == RightStick::Analog), "{}: clics sin sticks", l.id);
            assert!(l.shape == Shape::Two || l.stagger == 0, "{}: stagger solo en Two", l.id);
            assert!(!l.center.is_empty() && l.center.len() <= 2, "{}: centro", l.id);
            for c in l.center {
                assert!(c.bit == BTN_PLUS || c.bit == BTN_MINUS, "{}: centro con otro bit", l.id);
            }
            assert!(!l.name.is_empty());
        }
    }

    #[test]
    fn la_plantilla_efectiva() {
        assert_eq!(effective_layout(None, None), "retropad");
        assert_eq!(effective_layout(Some("md"), None), "md");
        assert_eq!(effective_layout(Some("md"), Some("auto")), "md", "«auto» no es un id: manda la consola");
        assert_eq!(effective_layout(Some("md"), Some("nes")), "nes");
        assert_eq!(effective_layout(Some("md"), Some("md3")), "md3");
        assert_eq!(effective_layout(Some("md"), Some("retropad")), "retropad");
        assert_eq!(effective_layout(Some("dos"), None), "retropad", "consola sin plantilla");
        assert_eq!(effective_layout(None, Some("md3")), "md3");
        assert_eq!(effective_layout(Some("md3"), None), "retropad", "md3 no lo anuncia el receptor");
    }

    #[test]
    fn el_json_esta_al_dia() {
        let repo = include_str!("../../protocol/retro-layouts.json").replace("\r\n", "\n");
        assert_eq!(
            repo,
            to_json(),
            "regenera con: cargo run --example write_layouts -- ../protocol/retro-layouts.json"
        );
    }

    #[test]
    fn el_json_es_json() {
        // Sin serde en el crate: una comprobación de forma (llaves y corchetes
        // equilibrados, comillas pares) que atrapa los despistes del generador
        let j = to_json();
        let (mut braces, mut brackets, mut quotes) = (0i32, 0i32, 0usize);
        for c in j.chars() {
            match c {
                '{' => braces += 1,
                '}' => braces -= 1,
                '[' => brackets += 1,
                ']' => brackets -= 1,
                '"' => quotes += 1,
                _ => {}
            }
            assert!(braces >= 0 && brackets >= 0);
        }
        assert_eq!((braces, brackets, quotes % 2), (0, 0, 0));
        assert!(j.contains("\"id\": \"n64\""));
        assert!(j.contains("\"bit\": \"MINUS\""));
    }
}
