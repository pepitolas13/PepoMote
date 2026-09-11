//! El Nunchuk (la otra mano): stick analógico táctil, botón C y gatillo Z.
//! Mismo multitouch REAL que el mando (ui/controller.rs): cada dedo se sigue
//! por su id de toque con hit-test propio, así el stick sigue respondiendo
//! mientras se mantiene Z o C. Con ratón (pruebas en PC) se usa el puntero;
//! en cuanto aparece el primer toque, el puntero sintetizado se ignora.
//! El movimiento (acelerómetro) va aparte por el hilo de paquetes, igual que
//! en el mando: aquí solo se pintan y se leen los dedos.

use crate::buttons::Buttons;
use crate::link::{Role, Status};
use crate::theme;
use egui::{Align2, Event, FontId, Pos2, Rect, RichText, Rounding, Sense, Stroke, TouchPhase, Vec2};
use std::collections::HashMap;
use std::time::Duration;

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

#[derive(Clone, Copy)]
enum Shape {
    Circle { c: Pos2, r: f32 },
    Rect(Rect),
}

impl Shape {
    fn hit(&self, p: Pos2) -> bool {
        match self {
            Shape::Circle { c, r } => c.distance(p) <= *r,
            Shape::Rect(r) => r.contains(p),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Button(u32),
    Stick,
}

const MOUSE_ID: u64 = u64::MAX;

pub struct NunchukUi {
    hits: Vec<(Shape, Target)>,
    touches: HashMap<u64, Target>,
    touch_seen: bool,
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
            touch_seen: false,
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
                Status::Connected { pc_name, player, role, rtt_ms, .. } => {
                    ui.label(RichText::new(pc_name).size(17.0).strong().color(theme::TEXT));
                    // un receptor anterior ignora el role del hello y nos da
                    // un slot de mando: que se vea, no que se sufra
                    let (mut line, color) = match role {
                        Role::Nunchuk => (format!("Nunchuk · Jugador {player}"), theme::TEXT_DIM),
                        Role::Wiimote => ("El PC te ve como mando: actualiza el receptor".to_owned(), theme::WARN),
                    };
                    if let Some(r) = rtt_ms {
                        line.push_str(&format!(" · {r:.0} ms"));
                    }
                    if sensor_hz > 0.0 {
                        line.push_str(&format!(" · {sensor_hz:.0} Hz"));
                    }
                    ui.label(RichText::new(line).size(13.0).color(color));
                }
                Status::Connecting => {
                    ui.label(RichText::new("Conectando…").size(17.0).strong().color(theme::TEXT));
                }
                _ => {
                    ui.label(RichText::new("Sin conexión").size(17.0).strong().color(theme::TEXT));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("Salir").size(14.0).color(theme::ERROR))
                    .clicked()
                {
                    action = Action::Exit;
                }
            });
        });

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
        let pressed = buttons.physical();
        // Escala para que quepa en pantallas bajas (referencia: 660 pt de alto)
        let s = (rect.height() / 660.0).clamp(0.7, 1.05);
        let cx = rect.center().x;

        // C: mediano, arriba. Va primero en los hits: tiene prioridad sobre
        // el margen del stick
        let c_r = 34.0 * s;
        let c_c = Pos2::new(cx, rect.top() + 10.0 * s + c_r);
        let c_down = pressed & pmp::BTN_C != 0;
        painter.circle(
            c_c,
            if c_down { c_r * 0.94 } else { c_r },
            if c_down { theme::GLOW } else { theme::CARD },
            Stroke::new(1.0_f32, theme::CARD_BORDER),
        );
        painter.text(c_c, Align2::CENTER_CENTER, "C", FontId::proportional(24.0 * s), theme::TEXT);
        self.hits.push((Shape::Circle { c: c_c, r: c_r }, Target::Button(pmp::BTN_C)));

        // Gatillo Z: banda inferior, como B en el mando
        let z_h = 84.0 * s;
        let z_rect = Rect::from_min_max(
            Pos2::new(rect.left(), rect.bottom() - z_h - 8.0 * s),
            Pos2::new(rect.right(), rect.bottom() - 8.0 * s),
        );

        // Stick: lo más grande que quepa entre C y Z
        let top = c_c.y + c_r + 16.0 * s;
        let bottom = z_rect.top() - 16.0 * s;
        let ring_r = ((bottom - top) / 2.0)
            .min(rect.width() / 2.0 - 12.0 * s)
            .min(150.0 * s)
            .max(40.0);
        let stick_c = Pos2::new(cx, (top + bottom) / 2.0);
        let knob_r = ring_r * 0.40;
        // el borde del pomo llega justo al anillo
        let travel = ring_r - knob_r;
        self.stick_c = stick_c;
        self.stick_travel = travel;
        let held = self.touches.values().any(|t| *t == Target::Stick);
        painter.circle(
            stick_c,
            ring_r,
            theme::CARD,
            Stroke::new(1.5_f32, if held { theme::BLUE } else { theme::CARD_BORDER }),
        );
        painter.circle_filled(stick_c, 3.0 * s, theme::CARD_BORDER);
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            painter.circle_filled(stick_c + Vec2::new(dx, dy) * (ring_r - 9.0 * s), 2.5 * s, theme::CARD_BORDER);
        }
        painter.circle(
            stick_c + self.knob * travel,
            knob_r,
            if held { theme::BLUE_HOVER } else { theme::BLUE },
            Stroke::new(1.0_f32, theme::CARD_BORDER),
        );
        self.hits.push((Shape::Circle { c: stick_c, r: ring_r + 12.0 * s }, Target::Stick));

        let z_down = pressed & pmp::BTN_Z != 0;
        painter.rect(
            z_rect,
            Rounding::same(22.0 * s),
            if z_down { theme::BLUE_HOVER } else { theme::BLUE },
            Stroke::NONE,
        );
        painter.text(z_rect.center(), Align2::CENTER_CENTER, "Z", FontId::proportional(30.0 * s), theme::CARD);
        self.hits.push((Shape::Rect(z_rect), Target::Button(pmp::BTN_Z)));
    }

    fn hit(&self, p: Pos2) -> Option<Target> {
        self.hits.iter().find(|(s, _)| s.hit(p)).map(|(_, t)| *t)
    }

    fn process_events(&mut self, ctx: &egui::Context, buttons: &Buttons) {
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                Event::Touch { id, phase, pos, .. } => {
                    self.touch_seen = true;
                    match phase {
                        TouchPhase::Start => self.begin(id.0, pos, buttons),
                        TouchPhase::Move => self.moved(id.0, pos, buttons),
                        TouchPhase::End | TouchPhase::Cancel => self.end(id.0, buttons),
                    }
                }
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    ..
                } if !self.touch_seen => {
                    if pressed {
                        self.begin(MOUSE_ID, pos, buttons);
                    } else {
                        self.end(MOUSE_ID, buttons);
                    }
                }
                Event::PointerMoved(pos) if !self.touch_seen => self.moved(MOUSE_ID, pos, buttons),
                _ => {}
            }
        }
        if !self.touches.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn begin(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        let Some(target) = self.hit(pos) else { return };
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
