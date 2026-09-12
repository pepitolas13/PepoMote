//! Paleta y estilo de las dos apps egui (receptor y móvil Linux; el móvil
//! incluye este archivo por `#[path]`). Dos paletas, «PepoWhite» (la de
//! siempre) y «PepoDark», y un interruptor global: los colores se leen con
//! funciones (`theme::text()`, `theme::card()`…) que devuelven la paleta
//! activa. El tema sigue al sistema (egui lo recibe de winit en Windows) o
//! lo que el usuario elija en Ajustes (imprescindible en Linux: winit no
//! informa del tema del escritorio en X11/Wayland).

use egui::{Color32, FontData, FontDefinitions, FontFamily, Theme};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub background: Color32,
    pub card: Color32,
    pub card_border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub blue: Color32,
    pub blue_hover: Color32,
    pub glow: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub error: Color32,
}

// Paleta "PepoWhite" — diseño original PepoMote (inspiración Wii, cero assets ajenos)
pub const LIGHT: Palette = Palette {
    background: Color32::from_rgb(0xF4, 0xF6, 0xF7),
    card: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    card_border: Color32::from_rgb(0xE3, 0xE8, 0xEB),
    text: Color32::from_rgb(0x3B, 0x47, 0x50),
    text_dim: Color32::from_rgb(0x7C, 0x8A, 0x94),
    blue: Color32::from_rgb(0x3F, 0xA9, 0xF5),
    blue_hover: Color32::from_rgb(0x2B, 0x98, 0xE8),
    glow: Color32::from_rgb(0xAE, 0xE2, 0xFF),
    ok: Color32::from_rgb(0x7B, 0xC9, 0x4C),
    warn: Color32::from_rgb(0xF5, 0xA8, 0x3C),
    error: Color32::from_rgb(0xE8, 0x5C, 0x5C),
};

// Paleta "PepoDark" — el mismo azul y los mismos acentos sobre grafito
pub const DARK: Palette = Palette {
    background: Color32::from_rgb(0x14, 0x18, 0x1C),
    card: Color32::from_rgb(0x1F, 0x25, 0x2B),
    card_border: Color32::from_rgb(0x2C, 0x34, 0x3B),
    text: Color32::from_rgb(0xE6, 0xEB, 0xEF),
    text_dim: Color32::from_rgb(0x8E, 0x9B, 0xA6),
    blue: LIGHT.blue,
    blue_hover: LIGHT.blue_hover,
    glow: Color32::from_rgb(0x1D, 0x3D, 0x52),
    ok: LIGHT.ok,
    warn: LIGHT.warn,
    error: LIGHT.error,
};

/// Texto sobre el azul (o cualquier acento): blanco en los dos temas.
pub const ON_ACCENT: Color32 = Color32::WHITE;

pub const RADIUS: f32 = 16.0;

static DARK_ON: AtomicBool = AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK_ON.load(Ordering::Relaxed)
}

/// La paleta activa.
pub fn c() -> &'static Palette {
    if is_dark() {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn background() -> Color32 {
    c().background
}
pub fn card() -> Color32 {
    c().card
}
pub fn card_border() -> Color32 {
    c().card_border
}
pub fn text() -> Color32 {
    c().text
}
pub fn text_dim() -> Color32 {
    c().text_dim
}
pub fn blue() -> Color32 {
    c().blue
}
#[allow(dead_code)] // lo usa el móvil Linux
pub fn blue_hover() -> Color32 {
    c().blue_hover
}
#[allow(dead_code)] // lo usa el móvil Linux
pub fn glow() -> Color32 {
    c().glow
}
pub fn ok() -> Color32 {
    c().ok
}
pub fn warn() -> Color32 {
    c().warn
}
pub fn error() -> Color32 {
    c().error
}

/// Lo que el usuario elige: seguir al sistema, o claro/oscuro fijo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePref {
    pub const ALL: [ThemePref; 3] = [ThemePref::System, ThemePref::Light, ThemePref::Dark];
}

fn visuals(p: &Palette, mut visuals: egui::Visuals) -> egui::Visuals {
    visuals.panel_fill = p.background;
    visuals.window_fill = p.card;
    visuals.extreme_bg_color = p.card; // fondo de los TextEdit
    visuals.faint_bg_color = p.background;
    visuals.override_text_color = Some(p.text);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.5_f32, p.card_border);
    visuals.widgets.inactive.bg_fill = p.card;
    visuals.widgets.inactive.weak_bg_fill = p.card;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, p.card_border);
    visuals.widgets.hovered.bg_fill = p.glow;
    visuals.widgets.hovered.weak_bg_fill = p.glow;
    visuals.widgets.active.bg_fill = p.blue;
    visuals.widgets.active.weak_bg_fill = p.blue;
    visuals.selection.bg_fill = p.blue;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, ON_ACCENT);
    visuals.widgets.noninteractive.rounding = RADIUS.into();
    visuals.widgets.inactive.rounding = RADIUS.into();
    visuals.widgets.hovered.rounding = RADIUS.into();
    visuals.widgets.active.rounding = RADIUS.into();
    visuals
}

/// Fuentes y los dos estilos (claro y oscuro). Llamar una vez al crear la
/// app; después `sync` cada frame y `set_preference` cuando el usuario elija.
pub fn apply(ctx: &egui::Context) {
    // Tipografía: Nunito (SIL OFL), embebida
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "nunito".to_owned(),
        FontData::from_static(include_bytes!("../Nunito.ttf")),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "nunito".to_owned());
    ctx.set_fonts(fonts);

    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, egui::Visuals::light()));
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, egui::Visuals::dark()));
    ctx.all_styles_mut(|style| style.spacing.item_spacing = egui::vec2(10.0, 10.0));
    // Sin noticia del sistema (Linux), claro: es el tema de siempre
    ctx.options_mut(|o| o.fallback_theme = Theme::Light);
    sync(ctx);
}

/// Primera línea de cada `update`: la paleta activa sigue al tema de egui.
pub fn sync(ctx: &egui::Context) {
    DARK_ON.store(ctx.theme() == Theme::Dark, Ordering::Relaxed);
}

/// Lo que ha elegido el usuario (o seguir al sistema).
pub fn set_preference(ctx: &egui::Context, pref: ThemePref) {
    ctx.set_theme(match pref {
        ThemePref::System => egui::ThemePreference::System,
        ThemePref::Light => egui::ThemePreference::Light,
        ThemePref::Dark => egui::ThemePreference::Dark,
    });
    sync(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Luminancia relativa (WCAG) de un color.
    fn lum(c: Color32) -> f64 {
        let ch = |v: u8| {
            let s = v as f64 / 255.0;
            if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (lum(a), lum(b));
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn las_dos_paletas_contrastan() {
        for (name, p) in [("clara", &LIGHT), ("oscura", &DARK)] {
            assert!(contrast(p.text, p.background) >= 7.0, "{name}: texto/fondo {:.1}", contrast(p.text, p.background));
            assert!(contrast(p.text, p.card) >= 7.0, "{name}: texto/tarjeta {:.1}", contrast(p.text, p.card));
            assert!(contrast(p.text_dim, p.card) >= 3.0, "{name}: texto tenue/tarjeta {:.1}", contrast(p.text_dim, p.card));
            assert!(contrast(ON_ACCENT, p.blue) >= 2.5, "{name}: blanco sobre azul {:.1}", contrast(ON_ACCENT, p.blue));
            assert!(p.card != p.background, "{name}: la tarjeta tiene que distinguirse del fondo");
        }
    }

    #[test]
    fn los_accesores_siguen_al_flag() {
        DARK_ON.store(false, Ordering::Relaxed);
        assert_eq!(text(), LIGHT.text);
        assert_eq!(background(), LIGHT.background);
        DARK_ON.store(true, Ordering::Relaxed);
        assert_eq!(text(), DARK.text);
        assert_eq!(card_border(), DARK.card_border);
        assert_eq!(blue(), LIGHT.blue, "el azul es el mismo en los dos temas");
        DARK_ON.store(false, Ordering::Relaxed);
    }

    #[test]
    fn la_preferencia_se_serializa_en_minusculas() {
        assert_eq!(serde_json::to_string(&ThemePref::Dark).unwrap(), "\"dark\"");
        assert_eq!(serde_json::from_str::<ThemePref>("\"system\"").unwrap(), ThemePref::System);
        assert_eq!(ThemePref::default(), ThemePref::System);
    }
}
