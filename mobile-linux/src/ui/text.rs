//! Teclado del móvil para el teclado en pantalla de Cemu. Los juegos de Wii U
//! que piden texto (el nombre en Zelda Wind Waker HD…) muestran un teclado
//! que no acepta toques, solo teclas del PC: este diálogo escribe con el
//! QWERTY propio y manda el texto por el canal de control (`text`,
//! PROTOCOL.md §3). «Escribir» lo teclea en Cemu, «Aceptar» añade Intro y
//! cierra, «Borrar» manda un retroceso (sin tocar el campo) y «Cerrar» no
//! manda nada. Mientras está abierto solo tapa la pantalla de juego: los
//! INPUT siguen saliendo y el giro no cambia.

use crate::theme;
use crate::ui::keypad::{self, Key};
use egui::{RichText, Rounding, Stroke, Vec2};

/// Botones del diálogo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// Borra un carácter en Cemu.
    Delete,
    /// Teclea el campo en Cemu y lo vacía; el diálogo sigue.
    Write,
    /// Teclea el campo más Intro y cierra.
    Accept,
    /// Cierra sin mandar nada.
    Close,
}

/// Qué hace un botón: lo que se manda al PC (si algo), si se vacía el campo
/// y si se cierra el diálogo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effect {
    pub send: Option<String>,
    pub clear_field: bool,
    pub close: bool,
}

/// Semántica de los botones (pura). Borrar manda `"\u{8}"` y NO toca el
/// campo; Escribir manda el campo tal cual (nada si está vacío) y lo vacía;
/// Aceptar manda el campo más `"\n"` (solo Intro si está vacío: confirma) y
/// cierra; Cerrar cierra sin mandar.
pub fn effect(button: Button, field: &str) -> Effect {
    match button {
        Button::Delete => Effect {
            send: Some("\u{8}".to_owned()),
            clear_field: false,
            close: false,
        },
        Button::Write => Effect {
            send: (!field.is_empty()).then(|| field.to_owned()),
            clear_field: true,
            close: false,
        },
        Button::Accept => Effect {
            send: Some(format!("{field}\n")),
            clear_field: true,
            close: true,
        },
        Button::Close => Effect {
            send: None,
            clear_field: true,
            close: true,
        },
    }
}

/// El diálogo: el campo y si la siguiente letra va en mayúscula.
#[derive(Default)]
pub struct TextDialog {
    pub field: String,
    shift: bool,
}

impl TextDialog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Una tecla del QWERTY sobre el campo (puro): las letras llegan ya en
    /// mayúscula si tocaba, Shift es de un toque y borrar quita el último
    /// carácter (no un byte).
    pub fn press(&mut self, key: Key) {
        match key {
            Key::Char(c) => {
                self.field.push(c);
                self.shift = false;
            }
            Key::Backspace => {
                self.field.pop();
            }
            Key::Shift => self.shift = !self.shift,
            Key::Ok => {}
        }
    }

    /// Dibuja el diálogo entero y devuelve el botón pulsado (la app aplica
    /// `effect` y manda lo que toque).
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Button> {
        let mut out = None;
        // alto de tecla según el sitio (cabecera, campo y botones aparte); en
        // ventanas bajas el conjunto hace scroll
        let key_h = ((ui.available_height() - 250.0) / 5.0 - 4.0).clamp(34.0, 48.0);
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Teclado para Cemu").size(22.0).strong().color(theme::text()));
            ui.label(
                RichText::new("Lo que escribas va al teclado en pantalla del juego. El mando sigue funcionando.")
                    .size(12.0)
                    .color(theme::text_dim()),
            );
            ui.add_space(6.0);
            egui::Frame::none()
                .fill(theme::card())
                .stroke(Stroke::new(1.5_f32, theme::card_border()))
                .rounding(Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::horizontal()
                        .id_salt("campo-texto")
                        .stick_to_right(true)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            if self.field.is_empty() {
                                ui.label(RichText::new("Escribe aquí…").size(22.0).color(theme::text_dim()));
                            } else {
                                ui.label(RichText::new(format!("{}|", self.field)).size(22.0).color(theme::text()));
                            }
                        });
                });
            ui.add_space(6.0);
            if let Some(k) = keypad::qwerty(ui, self.shift, key_h) {
                self.press(k);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let gap = 6.0;
                ui.spacing_mut().item_spacing = Vec2::splat(gap);
                let w = (ui.available_width() - gap * 2.0) / 3.0;
                let btn = |ui: &mut egui::Ui, label: &str, fill: egui::Color32, color: egui::Color32| -> bool {
                    ui.add_sized(
                        Vec2::new(w, 46.0),
                        egui::Button::new(RichText::new(label).size(15.0).color(color))
                            .fill(fill)
                            .stroke(Stroke::new(1.0_f32, theme::card_border())),
                    )
                    .clicked()
                };
                if btn(ui, "Borrar", theme::card(), theme::text()) {
                    out = Some(Button::Delete);
                }
                if btn(ui, "Escribir", theme::card(), theme::text()) {
                    out = Some(Button::Write);
                }
                if btn(ui, "Aceptar", theme::blue(), theme::ON_ACCENT) {
                    out = Some(Button::Accept);
                }
            });
            ui.label(
                RichText::new("Borrar quita una letra en Cemu · Escribir la teclea · Aceptar la teclea y confirma")
                    .size(11.0)
                    .color(theme::text_dim()),
            );
            ui.add_space(4.0);
            if ui
                .add_sized(
                    Vec2::new(ui.available_width(), 40.0),
                    egui::Button::new(RichText::new("Cerrar").size(14.0).color(theme::text_dim())).fill(theme::card()),
                )
                .clicked()
            {
                out = Some(Button::Close);
            }
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrar_escribir_aceptar_y_cerrar() {
        // Borrar: un retroceso a Cemu y el campo se queda como está
        assert_eq!(
            effect(Button::Delete, "Lin"),
            Effect { send: Some("\u{8}".into()), clear_field: false, close: false }
        );
        assert_eq!(effect(Button::Delete, "").send.as_deref(), Some("\u{8}"), "también con el campo vacío");
        // Escribir: el campo tal cual, se vacía, el diálogo sigue
        assert_eq!(
            effect(Button::Write, "Link"),
            Effect { send: Some("Link".into()), clear_field: true, close: false }
        );
        assert_eq!(
            effect(Button::Write, ""),
            Effect { send: None, clear_field: true, close: false },
            "vacío: nada que teclear"
        );
        // Aceptar: el campo más Intro, y cierra; vacío = solo Intro (confirma)
        assert_eq!(
            effect(Button::Accept, "Link"),
            Effect { send: Some("Link\n".into()), clear_field: true, close: true }
        );
        assert_eq!(effect(Button::Accept, "").send.as_deref(), Some("\n"));
        // Cerrar: nada al PC
        assert_eq!(effect(Button::Close, "Link"), Effect { send: None, clear_field: true, close: true });
        assert_eq!(effect(Button::Close, "").send, None);
    }

    #[test]
    fn teclas_sobre_el_campo() {
        let mut d = TextDialog::new();
        assert!(!d.shift);
        d.press(Key::Shift);
        assert!(d.shift);
        d.press(Key::Char('L'));
        assert!(!d.shift, "Shift es de un toque");
        d.press(Key::Char('i'));
        d.press(Key::Char('ñ'));
        assert_eq!(d.field, "Liñ");
        d.press(Key::Backspace);
        assert_eq!(d.field, "Li", "borra un carácter entero, no un byte");
        d.press(Key::Ok);
        assert_eq!(d.field, "Li", "OK no hace nada aquí");
        d.press(Key::Shift);
        d.press(Key::Shift);
        assert!(!d.shift, "dos toques: se apaga");
        d.press(Key::Backspace);
        d.press(Key::Backspace);
        d.press(Key::Backspace);
        assert_eq!(d.field, "", "borrar de más no estalla");
    }
}
