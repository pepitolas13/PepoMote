//! Selector del mando de consola en RetroArch: «Automático (Mega Drive)», las
//! consolas de `pmp::retro` y «RetroPad completo». Tapa la pantalla de juego
//! sin cambiarla (como el teclado): los INPUT siguen saliendo. La elección se
//! recuerda para el juego cargado (o hasta que RetroArch cargue uno).
use crate::theme;
use crate::tr;
use egui::{RichText, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Seguir al PC (la consola del juego que carga RetroArch).
    Auto,
    Layout(&'static str),
    Close,
}

/// `auto_name`: la plantilla que tocaría en automático; `chosen`: la elegida a
/// mano (None = automático); `game`: título del juego cargado, si lo hay.
pub fn show(ui: &mut egui::Ui, auto_name: &str, chosen: Option<&str>, game: Option<&str>) -> Option<Pick> {
    let mut out = None;
    ui.vertical_centered(|ui| {
        ui.add_space(6.0);
        ui.label(RichText::new(tr!("gp.layout_title")).size(18.0).strong().color(theme::text()));
        let hint = match game {
            Some(g) => tr!("gp.layout_hint_game", g),
            None => tr!("gp.layout_hint_no_game").to_owned(),
        };
        ui.label(RichText::new(hint).size(12.0).color(theme::text_dim()));
        ui.add_space(8.0);
    });
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        let w = ui.available_width().min(420.0);
        let row = |ui: &mut egui::Ui, label: &str, on: bool| -> bool {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 36.0), egui::Sense::click());
            let fill = if on { theme::blue() } else { theme::card() };
            ui.painter().rect(rect, egui::Rounding::same(10.0), fill, egui::Stroke::new(1.0_f32, theme::card_border()));
            let color = if on { theme::ON_ACCENT } else { theme::text() };
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(14.0), color);
            resp.clicked()
        };
        ui.vertical_centered(|ui| {
            if row(ui, &tr!("gp.layout_auto", auto_name), chosen.is_none()) {
                out = Some(Pick::Auto);
            }
            ui.add_space(4.0);
            for l in pmp::retro::LAYOUTS {
                let label = if l.id == "retropad" { tr!("gp.layout_full") } else { l.name };
                if row(ui, label, chosen == Some(l.id)) {
                    out = Some(Pick::Layout(l.id));
                }
                ui.add_space(4.0);
            }
            ui.add_space(6.0);
            if ui.button(RichText::new(tr!("kb.close")).size(14.0).color(theme::text())).clicked() {
                out = Some(Pick::Close);
            }
            ui.add_space(8.0);
        });
    });
    out
}
