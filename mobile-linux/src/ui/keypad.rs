//! Teclados propios en pantalla. Los teclados de Linux móvil (squeekboard,
//! maliit) se abren por el protocolo text-input de Wayland, que winit no
//! implementa: con botones grandes de egui el táctil va perfecto y no
//! dependemos de nada. Dos: el numérico (IP y código) y el QWERTY completo
//! (texto para el teclado en pantalla de Cemu).

use crate::theme;
use egui::{RichText, Stroke, Vec2};
use crate::tr;

pub enum Key {
    Char(char),
    Backspace,
    Ok,
    /// Mayúsculas de un toque: la siguiente letra.
    Shift,
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
            egui::Button::new(RichText::new(label).size(26.0).color(theme::text())),
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
                            egui::Button::new(RichText::new("OK").size(24.0).color(theme::ON_ACCENT))
                                .fill(theme::blue()),
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

/// Filas de letras del QWERTY (español: con ñ).
pub const QWERTY_ROWS: [&str; 3] = ["qwertyuiop", "asdfghjklñ", "zxcvbnm"];
pub const DIGIT_ROW: &str = "1234567890";
pub const SYMBOL_ROW: &str = "-.,'!?";
/// Columnas del teclado (la fila más ancha).
const COLS: f32 = 10.0;

/// La tecla de carácter según Shift: mayúscula (también la ñ).
pub fn shifted(c: char, shift: bool) -> char {
    if shift {
        c.to_uppercase().next().unwrap_or(c)
    } else {
        c
    }
}

/// Teclado QWERTY completo (dígitos, letras con ñ, símbolos, mayúsculas,
/// espacio y borrar) para el texto que va al teclado en pantalla de Cemu.
/// `key_h` = alto de cada tecla (lo decide quien lo dibuja según el sitio).
pub fn qwerty(ui: &mut egui::Ui, shift: bool, key_h: f32) -> Option<Key> {
    let mut out = None;
    let w = ui.available_width().min(480.0);
    let gap = 4.0;
    let bw = (w - gap * (COLS - 1.0)) / COLS;
    let font = (key_h * 0.42).clamp(15.0, 20.0);
    let key = |ui: &mut egui::Ui, label: &str, kw: f32, on: bool| -> bool {
        let (fill, color) = if on { (theme::blue(), theme::card()) } else { (theme::card(), theme::text()) };
        ui.add_sized(
            Vec2::new(kw, key_h),
            egui::Button::new(RichText::new(label).size(font).color(color))
                .fill(fill)
                .stroke(Stroke::new(1.0_f32, theme::card_border())),
        )
        .clicked()
    };
    // las teclas anchas (⇧, ⌫) valen tecla y media; así cada fila mide lo mismo
    let wide = bw * 1.5 + gap * 0.5;

    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(gap);
        ui.horizontal(|ui| {
            for c in DIGIT_ROW.chars() {
                if key(ui, &c.to_string(), bw, false) {
                    out = Some(Key::Char(c));
                }
            }
        });
        for (i, row) in QWERTY_ROWS.iter().enumerate() {
            ui.horizontal(|ui| {
                let last = i == QWERTY_ROWS.len() - 1;
                if last {
                    if key(ui, "⇧", wide, shift) {
                        out = Some(Key::Shift);
                    }
                } else {
                    let n = row.chars().count() as f32;
                    ui.add_space((COLS - n) * (bw + gap) / 2.0);
                }
                for c in row.chars() {
                    let l = shifted(c, shift);
                    if key(ui, &l.to_string(), bw, false) {
                        out = Some(Key::Char(l));
                    }
                }
                if last && key(ui, "⌫", wide, false) {
                    out = Some(Key::Backspace);
                }
            });
        }
        ui.horizontal(|ui| {
            for c in SYMBOL_ROW.chars() {
                if key(ui, &c.to_string(), bw, false) {
                    out = Some(Key::Char(c));
                }
            }
            if key(ui, tr!("kb.space"), bw * 4.0 + gap * 3.0, false) {
                out = Some(Key::Char(' '));
            }
        });
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mayusculas_con_shift() {
        assert_eq!(shifted('a', false), 'a');
        assert_eq!(shifted('a', true), 'A');
        assert_eq!(shifted('ñ', true), 'Ñ');
        assert_eq!(shifted('1', true), '1', "los dígitos no cambian");
        assert_eq!(shifted(' ', true), ' ');
    }

    #[test]
    fn el_qwerty_tiene_todas_las_letras() {
        let letras: String = QWERTY_ROWS.concat();
        assert_eq!(letras.chars().count(), 27, "26 letras y la ñ");
        for c in "abcdefghijklmnopqrstuvwxyzñ".chars() {
            assert!(letras.contains(c), "falta {c}");
        }
        assert_eq!(DIGIT_ROW.len(), 10);
        assert!(QWERTY_ROWS.iter().all(|r| r.chars().count() as f32 <= COLS), "ninguna fila más ancha que el teclado");
    }
}
