use egui::{Color32, FontData, FontDefinitions, FontFamily};

// Paleta "PepoWhite" — diseño original PepoMote (inspiración Wii, cero assets ajenos)
pub const BACKGROUND: Color32 = Color32::from_rgb(0xF4, 0xF6, 0xF7);
pub const CARD: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const CARD_BORDER: Color32 = Color32::from_rgb(0xE3, 0xE8, 0xEB);
pub const TEXT: Color32 = Color32::from_rgb(0x3B, 0x47, 0x50);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x7C, 0x8A, 0x94);
pub const BLUE: Color32 = Color32::from_rgb(0x3F, 0xA9, 0xF5);
pub const BLUE_HOVER: Color32 = Color32::from_rgb(0x2B, 0x98, 0xE8);
pub const GLOW: Color32 = Color32::from_rgb(0xAE, 0xE2, 0xFF);
pub const OK: Color32 = Color32::from_rgb(0x7B, 0xC9, 0x4C);
pub const WARN: Color32 = Color32::from_rgb(0xF5, 0xA8, 0x3C);
pub const ERROR: Color32 = Color32::from_rgb(0xE8, 0x5C, 0x5C);

pub const RADIUS: f32 = 16.0;

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

    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = CARD;
    visuals.override_text_color = Some(TEXT);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.5_f32, CARD_BORDER);
    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.weak_bg_fill = CARD;
    visuals.widgets.hovered.bg_fill = GLOW;
    visuals.widgets.active.bg_fill = BLUE;
    visuals.selection.bg_fill = BLUE;
    visuals.widgets.noninteractive.rounding = RADIUS.into();
    visuals.widgets.inactive.rounding = RADIUS.into();
    visuals.widgets.hovered.rounding = RADIUS.into();
    visuals.widgets.active.rounding = RADIUS.into();
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    ctx.set_style(style);
}
