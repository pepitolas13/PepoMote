//! Teclado numérico propio. Los teclados en pantalla de Linux móvil
//! (squeekboard, maliit) se abren por el protocolo text-input de Wayland,
//! que winit no implementa: con botones grandes de egui el táctil va
//! perfecto y no dependemos de nada.

use crate::theme;
use egui::{RichText, Vec2};

pub enum Key {
    Char(char),
    Backspace,
    Ok,
}

/// Dibuja el teclado; `extra` = teclas adicionales (p. ej. '.' y ':').
pub fn keypad(ui: &mut egui::Ui, extra: &[char], show_ok: bool) -> Option<Key> {
    let mut out = None;
    let w = ui.available_width().min(380.0);
    let gap = 8.0;
    let bw = (w - gap * 2.0) / 3.0;
    let bh = 60.0;
    let key = |ui: &mut egui::Ui, label: &str| -> bool {
        ui.add_sized(
            Vec2::new(bw, bh),
            egui::Button::new(RichText::new(label).size(26.0).color(theme::TEXT)),
        )
        .clicked()
    };

    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(gap);
        for row in [["1", "2", "3"], ["4", "5", "6"], ["7", "8", "9"]] {
            ui.horizontal(|ui| {
                for label in row {
                    if key(ui, label) {
                        out = Some(Key::Char(label.chars().next().unwrap()));
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            match extra.first() {
                Some(c) => {
                    if key(ui, &c.to_string()) {
                        out = Some(Key::Char(*c));
                    }
                }
                None => {
                    ui.add_space(bw + gap);
                }
            }
            if key(ui, "0") {
                out = Some(Key::Char('0'));
            }
            if key(ui, "⌫") {
                out = Some(Key::Backspace);
            }
        });
        if extra.len() > 1 || show_ok {
            ui.horizontal(|ui| {
                for c in extra.iter().skip(1) {
                    if key(ui, &c.to_string()) {
                        out = Some(Key::Char(*c));
                    }
                }
                if show_ok
                    && ui
                        .add_sized(
                            Vec2::new(bw * 2.0 + gap, bh),
                            egui::Button::new(RichText::new("OK").size(24.0).color(theme::CARD))
                                .fill(theme::BLUE),
                        )
                        .clicked()
                {
                    out = Some(Key::Ok);
                }
            });
        }
    });
    out
}
