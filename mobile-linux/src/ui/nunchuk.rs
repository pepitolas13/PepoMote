//! El Nunchuk (la otra mano): stick analógico táctil, botón C y gatillo Z.
//! Mismo multitouch REAL que el mando (ui/touch.rs): cada dedo se sigue por
//! su id de toque con hit-test propio, así el stick sigue respondiendo
//! mientras se mantiene Z o C. Con ratón (pruebas en PC) se usa el puntero;
//! en cuanto aparece el primer toque, el puntero sintetizado se ignora.
//! El movimiento (acelerómetro) va aparte por el hilo de paquetes, igual que
//! en el mando: aquí solo se pintan y se leen los dedos.

use crate::buttons::Buttons;
use crate::link::{Role, Status};
use crate::theme;
use crate::ui::controller::{mode_label, notice_banner};
use crate::ui::touch::{self, Canvas, Input, Phase, Shape, Transform};
use egui::{Align2, FontId, Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2};
use std::collections::HashMap;
use std::time::Duration;
use crate::tr;

pub enum Action {
    None,
    Exit,
}

/// Zona muerta del stick (fracción del recorrido): un dedo apoyado en el
/// centro no mueve nada.
pub const DEAD_ZONE: f32 = 0.08;

/// Desplazamiento del dedo desde el centro (px de pantalla, +Y hacia abajo)
/// → pomo en −1..1 recortado al círculo unidad. Sin zona muerta: es lo que
/// se pinta, y el dedo y el pomo van juntos.
pub fn knob_pos(dx: f32, dy: f32, travel: f32) -> Vec2 {
    if travel <= 0.0 {
        return Vec2::ZERO;
    }
    let v = Vec2::new(dx, dy) / travel;
    let len = v.length();
    if len > 1.0 {
        v / len
    } else {
        v
    }
}

/// Pomo (−1..1, +Y hacia abajo) → (x, y) del protocolo: −127..127, +X
/// derecha, +Y arriba. La zona muerta se re-escala (sin salto al salir de
/// ella) y el módulo se recorta a 1, así nunca sale −128.
pub fn stick_value(knob: Vec2) -> (i8, i8) {
    let len = knob.length();
    if len <= DEAD_ZONE {
        return (0, 0);
    }
    let mag = ((len - DEAD_ZONE) / (1.0 - DEAD_ZONE)).min(1.0);
    let dir = knob / len;
    let x = (dir.x * mag * 127.0).round() as i8;
    let y = (-dir.y * mag * 127.0).round() as i8;
    (x, y)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Button(u32),
    Stick,
}

pub struct NunchukUi {
    hits: Vec<(Shape, Target)>,
    touches: HashMap<u64, Target>,
    input: Input,
    /// Centro y recorrido del stick del último frame: el dedo se mapea
    /// contra ellos.
    stick_c: Pos2,
    stick_travel: f32,
    /// Pomo que se pinta (−1..1, +Y abajo); al centro al soltar.
    knob: Vec2,
}

impl Default for NunchukUi {
    fn default() -> Self {
        Self::new()
    }
}

impl NunchukUi {
    pub fn new() -> Self {
        Self {
            hits: Vec::new(),
            touches: HashMap::new(),
            input: Input::default(),
            stick_c: Pos2::ZERO,
            stick_travel: 1.0,
            knob: Vec2::ZERO,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, buttons: &Buttons, status: &Status, sensor_hz: f32) -> Action {
        let mut action = Action::None;

        // Cabecera
        ui.horizontal(|ui| {
            ui.vertical(|ui| match status {
                Status::Connected { pc_name, mode, player, role, rtt_ms, pad, .. } => {
                    ui.label(RichText::new(pc_name).size(17.0).strong().color(theme::text()));
                    // un receptor anterior ignora el role del hello y nos da
                    // un slot de mando: que se vea, no que se sufra. El modo
                    // lo decide el mando; nos llega por difusión
                    let (mut line, color) = match role {
                        Role::Nunchuk => (tr!("nk.line", player, mode_label(mode)), theme::text_dim()),
                        Role::Wiimote => (tr!("nk.old_pc").to_owned(), theme::warn()),
                    };
                    if let Some(r) = rtt_ms {
                        line.push_str(&format!(" · {r:.0} ms"));
                    }
                    if sensor_hz > 0.0 {
                        line.push_str(&format!(" · {sensor_hz:.0} Hz"));
                    }
                    ui.label(RichText::new(line).size(13.0).color(color));
                    // en Wii U, Cemu solo ve el Nunchuk colgado de un Mando de Wii
                    if mode == "cemu" && pad != "wiimote" {
                        ui.label(
                            RichText::new(tr!("nk.help"))
                                .size(12.0)
                                .color(theme::warn()),
                        );
                    }
                }
                Status::Connecting | Status::Reconnecting { .. } => {
                    let txt = if matches!(status, Status::Reconnecting { .. }) { tr!("common.reconnecting") } else { tr!("common.connecting") };
                    ui.label(RichText::new(txt).size(17.0).strong().color(theme::text()));
                }
                _ => {
                    ui.label(RichText::new(tr!("common.no_connection")).size(17.0).strong().color(theme::text()));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new(tr!("common.exit")).size(14.0).color(theme::error()))
                    .clicked()
                {
                    action = Action::Exit;
                }
            });
        });
        notice_banner(ui, status);

        // Cuerpo: un lienzo con hit-test propio
        let avail = ui.available_size();
        let (rect, _) = ui.allocate_exact_size(avail, Sense::hover());
        self.hits.clear();
        self.layout(ui, rect, buttons);
        self.process_events(ui.ctx(), buttons);
        action
    }

    fn layout(&mut self, ui: &mut egui::Ui, rect: Rect, buttons: &Buttons) {
        let painter = ui.painter();
        let cv = Canvas::new(painter, rect, Transform::Straight);
        let pressed = buttons.physical();
        // Escala para que quepa en pantallas bajas (referencia: 660 pt de alto)
        let s = (rect.height() / 660.0).clamp(0.7, 1.05);
        let cx = rect.center().x;

        // Gatillo Z: banda inferior, como B en el mando (el pulgar cae ahí
        // sin mirar; en el Nunchuk real es el gatillo grande)
        let z_h = 96.0 * s;
        let z_rect = Rect::from_min_max(
            Pos2::new(rect.left(), rect.bottom() - z_h - 8.0 * s),
            Pos2::new(rect.right(), rect.bottom() - 8.0 * s),
        );

        // Stick: en la zona del pulgar (justo sobre Z), de tamaño cómodo
        // (anillo de ~80 pt de radio como mucho: un pulgar lo recorre entero
        // sin cambiar el agarre); C, grande, justo encima
        let c_r = 40.0 * s;
        let c_top = rect.top() + 8.0 * s;
        let stick_bottom = z_rect.top() - 22.0 * s;
        let ring_r = ((stick_bottom - (c_top + 2.0 * c_r + 18.0 * s)) / 2.0)
            .min(rect.width() / 2.0 - 16.0 * s)
            .min(80.0 * s)
            .max(40.0);
        let stick_c = Pos2::new(cx, stick_bottom - ring_r);
        // C va primero en los hits: tiene prioridad sobre el margen del stick
        let c_c = Pos2::new(cx, (stick_c.y - ring_r - 18.0 * s - c_r).max(c_top + c_r));
        let shape = touch::circle_button(&cv, c_c, c_r, "C", 26.0 * s, pressed & pmp::BTN_C != 0, false);
        self.hits.push((shape, Target::Button(pmp::BTN_C)));
        let knob_r = ring_r * 0.40;
        // el borde del pomo llega justo al anillo
        let travel = ring_r - knob_r;
        self.stick_c = stick_c;
        self.stick_travel = travel;
        let held = self.touches.values().any(|t| *t == Target::Stick);
        painter.circle(
            stick_c,
            ring_r,
            theme::card(),
            Stroke::new(1.5_f32, if held { theme::blue() } else { theme::card_border() }),
        );
        painter.circle_filled(stick_c, 3.0 * s, theme::card_border());
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            painter.circle_filled(stick_c + Vec2::new(dx, dy) * (ring_r - 9.0 * s), 2.5 * s, theme::card_border());
        }
        painter.circle(
            stick_c + self.knob * travel,
            knob_r,
            if held { theme::blue_hover() } else { theme::blue() },
            Stroke::new(1.0_f32, theme::card_border()),
        );
        self.hits.push((Shape::Circle { c: stick_c, r: ring_r + 12.0 * s }, Target::Stick));

        let z_down = pressed & pmp::BTN_Z != 0;
        painter.rect(
            z_rect,
            Rounding::same(22.0 * s),
            if z_down { theme::blue_hover() } else { theme::blue() },
            Stroke::NONE,
        );
        painter.text(z_rect.center(), Align2::CENTER_CENTER, "Z", FontId::proportional(34.0 * s), theme::ON_ACCENT);
        self.hits.push((Shape::Rect(z_rect), Target::Button(pmp::BTN_Z)));
    }

    fn process_events(&mut self, ctx: &egui::Context, buttons: &Buttons) {
        for ev in self.input.events(ctx) {
            match ev.phase {
                Phase::Begin => self.begin(ev.key, ev.pos, buttons),
                Phase::Move => self.moved(ev.key, ev.pos, buttons),
                Phase::End => self.end(ev.key, buttons),
            }
        }
        if !self.touches.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn begin(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        let Some(target) = touch::hit_test(&self.hits, pos) else { return };
        match target {
            Target::Button(bit) => buttons.set(bit, true),
            Target::Stick => {
                // un solo dedo lleva el stick; el segundo se ignora
                if self.touches.values().any(|t| *t == Target::Stick) {
                    return;
                }
                self.drag(pos, buttons);
            }
        }
        self.touches.insert(key, target);
    }

    fn moved(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        if self.touches.get(&key) == Some(&Target::Stick) {
            self.drag(pos, buttons);
        }
    }

    fn end(&mut self, key: u64, buttons: &Buttons) {
        match self.touches.remove(&key) {
            Some(Target::Button(bit)) => buttons.set(bit, false),
            Some(Target::Stick) => {
                // al soltar, al centro
                self.knob = Vec2::ZERO;
                buttons.set_stick(0, 0);
            }
            None => {}
        }
    }

    /// Dedo en `pos` → pomo y valor en el cable.
    fn drag(&mut self, pos: Pos2, buttons: &Buttons) {
        let d = pos - self.stick_c;
        self.knob = knob_pos(d.x, d.y, self.stick_travel);
        let (x, y) = stick_value(self.knob);
        buttons.set_stick(x, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dedo a (dx, dy) px del centro con recorrido `travel` → cable.
    fn v(dx: f32, dy: f32, travel: f32) -> (i8, i8) {
        stick_value(knob_pos(dx, dy, travel))
    }

    #[test]
    fn centro_y_zona_muerta_dan_cero() {
        assert_eq!(v(0.0, 0.0, 100.0), (0, 0));
        assert_eq!(v(5.0, -5.0, 100.0), (0, 0));
        assert_eq!(v(0.0, DEAD_ZONE * 100.0, 100.0), (0, 0), "el borde de la zona muerta aún es cero");
    }

    #[test]
    fn ejes_y_signos_del_protocolo() {
        assert_eq!(v(100.0, 0.0, 100.0), (127, 0), "+X derecha");
        assert_eq!(v(-100.0, 0.0, 100.0), (-127, 0));
        assert_eq!(v(0.0, -100.0, 100.0), (0, 127), "dedo hacia arriba en pantalla = +Y");
        assert_eq!(v(0.0, 100.0, 100.0), (0, -127));
    }

    #[test]
    fn fuera_del_anillo_se_recorta_al_modulo_maximo() {
        assert_eq!(v(300.0, 0.0, 100.0), (127, 0));
        let (x, y) = v(300.0, -300.0, 100.0);
        assert_eq!(x, y, "diagonal: mismo módulo por eje");
        let m = ((x as f32).powi(2) + (y as f32).powi(2)).sqrt();
        assert!((m - 127.0).abs() <= 1.0, "módulo {m}");
    }

    #[test]
    fn sin_salto_al_salir_de_la_zona_muerta() {
        let (x, y) = v(DEAD_ZONE * 100.0 + 1.0, 0.0, 100.0);
        assert!((0..=2).contains(&x), "x={x}");
        assert_eq!(y, 0);
        // medio recorrido: (0.5 − 0.08) / 0.92 · 127 ≈ 58
        assert_eq!(v(50.0, 0.0, 100.0), (58, 0));
    }

    #[test]
    fn nunca_sale_menos_128() {
        for deg in 0..360 {
            let a = (deg as f32).to_radians();
            let (x, y) = v(a.cos() * 1000.0, a.sin() * 1000.0, 100.0);
            assert!(x >= -127 && y >= -127, "{deg}°: ({x}, {y})");
        }
    }

    #[test]
    fn pomo_recortado_al_circulo() {
        let k = knob_pos(300.0, 400.0, 100.0);
        assert!((k.length() - 1.0).abs() < 1e-5);
        assert!((k.x - 0.6).abs() < 1e-5 && (k.y - 0.8).abs() < 1e-5, "misma dirección");
        assert_eq!(knob_pos(30.0, -40.0, 100.0), Vec2::new(0.3, -0.4), "dentro: sin recorte");
        assert_eq!(knob_pos(50.0, 50.0, 0.0), Vec2::ZERO, "sin recorrido: al centro");
    }
}
