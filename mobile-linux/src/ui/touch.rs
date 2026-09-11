//! Entrada táctil y lienzo compartidos por el mando, el Nunchuk y el GamePad:
//! formas con hit-test propio, multitouch REAL (cada dedo se sigue por su id
//! de toque; con ratón —pruebas en PC— se usa el puntero, y en cuanto aparece
//! el primer toque el puntero sintetizado del primer dedo se ignora para no
//! contar doble) y la transformación de un espacio virtual apaisado a la
//! pantalla real cuando el móvil se sostiene girado, texto incluido.

use crate::frame::Rotation;
use crate::theme;
use egui::epaint::TextShape;
use egui::{Align2, Color32, Event, FontId, Pos2, Rect, Rounding, Stroke, TouchPhase, Vec2};
use std::f32::consts::FRAC_PI_2;

/// Id de "dedo" del puntero del ratón.
pub const MOUSE_ID: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Circle { c: Pos2, r: f32 },
    Rect(Rect),
}

impl Shape {
    pub fn hit(&self, p: Pos2) -> bool {
        match self {
            Shape::Circle { c, r } => c.distance(p) <= *r,
            Shape::Rect(r) => r.contains(p),
        }
    }
}

/// Primer objetivo cuya forma contiene `p` (el orden de `hits` es la prioridad).
pub fn hit_test<T: Copy>(hits: &[(Shape, T)], p: Pos2) -> Option<T> {
    hits.iter().find(|(s, _)| s.hit(p)).map(|(_, t)| *t)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Begin,
    Move,
    End,
}

/// Un dedo (o el ratón) en la pantalla, en coordenadas de pantalla.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchEvent {
    pub key: u64,
    pub phase: Phase,
    pub pos: Pos2,
}

/// Unifica los toques y el ratón del frame en eventos por dedo.
#[derive(Default)]
pub struct Input {
    touch_seen: bool,
}

impl Input {
    pub fn events(&mut self, ctx: &egui::Context) -> Vec<TouchEvent> {
        let events = ctx.input(|i| i.events.clone());
        self.collect(&events)
    }

    fn collect(&mut self, events: &[Event]) -> Vec<TouchEvent> {
        let mut out = Vec::new();
        for ev in events {
            match ev {
                Event::Touch { id, phase, pos, .. } => {
                    self.touch_seen = true;
                    let phase = match phase {
                        TouchPhase::Start => Phase::Begin,
                        TouchPhase::Move => Phase::Move,
                        TouchPhase::End | TouchPhase::Cancel => Phase::End,
                    };
                    out.push(TouchEvent { key: id.0, phase, pos: *pos });
                }
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    ..
                } if !self.touch_seen => {
                    out.push(TouchEvent {
                        key: MOUSE_ID,
                        phase: if *pressed { Phase::Begin } else { Phase::End },
                        pos: *pos,
                    });
                }
                Event::PointerMoved(pos) if !self.touch_seen => {
                    out.push(TouchEvent {
                        key: MOUSE_ID,
                        phase: Phase::Move,
                        pos: *pos,
                    });
                }
                _ => {}
            }
        }
        out
    }
}

/// Cómo se pinta el espacio virtual (apaisado) sobre la pantalla real.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transform {
    /// La ventana ya es apaisada: coordenadas de pantalla tal cual.
    Straight,
    /// Ventana vertical, móvil girado con el borde superior a la izquierda.
    RotLeft,
    /// Ventana vertical, móvil girado con el borde superior a la derecha.
    RotRight,
}

impl Transform {
    /// Recto si la ventana ya es apaisada; si es vertical (Phosh sin rotar),
    /// girado según el ajuste de giro.
    pub fn for_screen(screen: Rect, rotation: Rotation) -> Transform {
        if screen.width() >= screen.height() {
            Transform::Straight
        } else {
            match rotation {
                Rotation::Left => Transform::RotLeft,
                Rotation::Right => Transform::RotRight,
            }
        }
    }

    /// Espacio virtual: empieza en la esquina de `screen` y mide lo mismo
    /// (recto) o con ancho y alto intercambiados (girado).
    pub fn virtual_rect(self, screen: Rect) -> Rect {
        match self {
            Transform::Straight => screen,
            _ => Rect::from_min_size(screen.min, Vec2::new(screen.height(), screen.width())),
        }
    }

    pub fn to_screen(self, screen: Rect, p: Pos2) -> Pos2 {
        let (dx, dy) = (p.x - screen.left(), p.y - screen.top());
        match self {
            Transform::Straight => p,
            Transform::RotLeft => Pos2::new(screen.right() - dy, screen.top() + dx),
            Transform::RotRight => Pos2::new(screen.left() + dy, screen.bottom() - dx),
        }
    }

    /// Inversa de `to_screen`: un dedo en pantalla → espacio virtual.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_screen(self, screen: Rect, p: Pos2) -> Pos2 {
        match self {
            Transform::Straight => p,
            Transform::RotLeft => Pos2::new(screen.left() + (p.y - screen.top()), screen.top() + (screen.right() - p.x)),
            Transform::RotRight => Pos2::new(screen.left() + (screen.bottom() - p.y), screen.top() + (p.x - screen.left())),
        }
    }

    pub fn rect_to_screen(self, screen: Rect, r: Rect) -> Rect {
        Rect::from_two_pos(self.to_screen(screen, r.min), self.to_screen(screen, r.max))
    }

    /// Ángulo del texto (radianes, horario) para `TextShape`: el eje X
    /// virtual cae hacia abajo en la pantalla con el giro a la izquierda y
    /// hacia arriba con el giro a la derecha.
    pub fn angle(self) -> f32 {
        match self {
            Transform::Straight => 0.0,
            Transform::RotLeft => FRAC_PI_2,
            Transform::RotRight => -FRAC_PI_2,
        }
    }
}

/// Pintor en coordenadas virtuales: todo lo que se dibuja pasa por la
/// transformación, así una pantalla se escribe una sola vez para las tres.
pub struct Canvas<'a> {
    pub painter: &'a egui::Painter,
    pub screen: Rect,
    pub t: Transform,
}

impl<'a> Canvas<'a> {
    pub fn new(painter: &'a egui::Painter, screen: Rect, t: Transform) -> Self {
        Self { painter, screen, t }
    }

    /// Rectángulo virtual donde se dibuja.
    pub fn rect(&self) -> Rect {
        self.t.virtual_rect(self.screen)
    }

    pub fn circle(&self, c: Pos2, r: f32, fill: Color32, stroke: Stroke) {
        self.painter.circle(self.t.to_screen(self.screen, c), r, fill, stroke);
    }

    pub fn circle_filled(&self, c: Pos2, r: f32, fill: Color32) {
        self.painter.circle_filled(self.t.to_screen(self.screen, c), r, fill);
    }

    pub fn rounded_rect(&self, r: Rect, rounding: f32, fill: Color32, stroke: Stroke) {
        self.painter.rect(self.t.rect_to_screen(self.screen, r), Rounding::same(rounding), fill, stroke);
    }

    /// Texto anclado en `pos` (virtual), girado con la pantalla. Devuelve el
    /// rectángulo virtual que ocupa.
    pub fn text(&self, pos: Pos2, anchor: Align2, text: &str, font: FontId, color: Color32) -> Rect {
        let galley = self.painter.layout_no_wrap(text.to_owned(), font, color);
        let vrect = anchor.anchor_size(pos, galley.size());
        let top_left = self.t.to_screen(self.screen, vrect.min);
        self.painter.add(TextShape::new(top_left, galley, color).with_angle(self.t.angle()));
        vrect
    }

    pub fn text_width(&self, text: &str, font: FontId) -> f32 {
        self.painter.layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER).size().x
    }

    /// `text` recortado (con «…») para que quepa en `max_w`.
    pub fn fit_text(&self, text: &str, font: FontId, max_w: f32) -> String {
        fit(text, max_w, |t| self.text_width(t, font.clone()))
    }
}

/// Recorta `text` hasta que `measure` diga que cabe en `max_w`.
pub fn fit(text: &str, max_w: f32, measure: impl Fn(&str) -> f32) -> String {
    if measure(text) <= max_w {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    for n in (1..chars.len()).rev() {
        let mut s: String = chars[..n].iter().collect();
        s = s.trim_end().to_owned();
        s.push('…');
        if measure(&s) <= max_w {
            return s;
        }
    }
    String::new()
}

fn button_colors(down: bool, primary: bool) -> (Color32, Color32) {
    if primary {
        (if down { theme::BLUE_HOVER } else { theme::BLUE }, theme::CARD)
    } else {
        (if down { theme::GLOW } else { theme::CARD }, theme::TEXT)
    }
}

/// Botón redondo: relleno según pulsado (primario = azul con texto claro),
/// etiqueta centrada; devuelve su forma (virtual) para el hit-test.
pub fn circle_button(cv: &Canvas, c: Pos2, r: f32, label: &str, font: f32, down: bool, primary: bool) -> Shape {
    let (fill, text) = button_colors(down, primary);
    cv.circle(c, if down { r * 0.94 } else { r }, fill, Stroke::new(1.0_f32, theme::CARD_BORDER));
    cv.text(c, Align2::CENTER_CENTER, label, FontId::proportional(font), text);
    Shape::Circle { c, r }
}

/// Botón rectangular redondeado con etiqueta centrada.
pub fn rect_button(cv: &Canvas, r: Rect, rounding: f32, label: &str, font: f32, down: bool, primary: bool) -> Shape {
    let (fill, text) = button_colors(down, primary);
    cv.rounded_rect(r, rounding, fill, Stroke::new(1.0_f32, theme::CARD_BORDER));
    cv.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(font), text);
    Shape::Rect(r)
}

/// Chip de cabecera (como `selectable_label`): azul si está seleccionado; si
/// no, tarjeta con el texto en `color`.
pub fn chip(cv: &Canvas, r: Rect, label: &str, font: f32, selected: bool, color: Color32) -> Shape {
    let (fill, text) = if selected { (theme::BLUE, theme::CARD) } else { (theme::CARD, color) };
    cv.rounded_rect(r, r.height() / 2.0, fill, Stroke::new(1.0_f32, theme::CARD_BORDER));
    cv.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(font), text);
    Shape::Rect(r)
}

/// Estado de un segmento del selector «En Cemu soy».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seg {
    Off,
    /// El activo: relleno azul.
    On,
    /// Pedido y sin eco todavía: medio tono.
    Pending,
}

/// Segmento del selector: como un chip, con el estado pendiente a medio tono.
pub fn segment(cv: &Canvas, r: Rect, label: &str, font: f32, state: Seg) -> Shape {
    let (fill, text) = match state {
        Seg::On => (theme::BLUE, theme::CARD),
        Seg::Pending => (theme::GLOW, theme::TEXT),
        Seg::Off => (theme::CARD, theme::TEXT),
    };
    cv.rounded_rect(r, 8.0, fill, Stroke::new(1.0_f32, theme::CARD_BORDER));
    cv.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(font), text);
    Shape::Rect(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Modifiers, PointerButton, TouchDeviceId, TouchId};

    #[test]
    fn formas_y_prioridad() {
        let c = Shape::Circle { c: Pos2::new(10.0, 10.0), r: 5.0 };
        assert!(c.hit(Pos2::new(14.0, 10.0)));
        assert!(!c.hit(Pos2::new(14.0, 14.0)), "la esquina del cuadrado circunscrito queda fuera");
        let r = Shape::Rect(Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(20.0, 20.0)));
        assert!(r.hit(Pos2::new(14.0, 14.0)));
        let hits = [(c, "circulo"), (r, "rect")];
        assert_eq!(hit_test(&hits, Pos2::new(10.0, 10.0)), Some("circulo"), "el primero gana");
        assert_eq!(hit_test(&hits, Pos2::new(1.0, 1.0)), Some("rect"));
        assert_eq!(hit_test(&hits, Pos2::new(50.0, 50.0)), None);
    }

    fn touch(id: u64, phase: TouchPhase, x: f32) -> Event {
        Event::Touch {
            device_id: TouchDeviceId(0),
            id: TouchId(id),
            phase,
            pos: Pos2::new(x, 0.0),
            force: None,
        }
    }

    fn click(pressed: bool, x: f32) -> Event {
        Event::PointerButton {
            pos: Pos2::new(x, 0.0),
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn raton_hasta_el_primer_toque() {
        let mut input = Input::default();
        let evs = input.collect(&[click(true, 1.0), Event::PointerMoved(Pos2::new(2.0, 0.0)), click(false, 3.0)]);
        assert_eq!(
            evs,
            vec![
                TouchEvent { key: MOUSE_ID, phase: Phase::Begin, pos: Pos2::new(1.0, 0.0) },
                TouchEvent { key: MOUSE_ID, phase: Phase::Move, pos: Pos2::new(2.0, 0.0) },
                TouchEvent { key: MOUSE_ID, phase: Phase::End, pos: Pos2::new(3.0, 0.0) },
            ]
        );
        // el botón secundario no es un dedo
        let secondary = Event::PointerButton {
            pos: Pos2::ZERO,
            button: PointerButton::Secondary,
            pressed: true,
            modifiers: Modifiers::default(),
        };
        assert!(input.collect(&[secondary]).is_empty());
        // en cuanto hay un toque real, el puntero sintetizado se ignora
        let evs = input.collect(&[
            touch(7, TouchPhase::Start, 10.0),
            click(true, 10.0),
            Event::PointerMoved(Pos2::new(11.0, 0.0)),
            touch(7, TouchPhase::Move, 11.0),
            touch(8, TouchPhase::Start, 20.0),
            touch(8, TouchPhase::Cancel, 20.0),
            touch(7, TouchPhase::End, 12.0),
            click(false, 12.0),
        ]);
        assert_eq!(
            evs,
            vec![
                TouchEvent { key: 7, phase: Phase::Begin, pos: Pos2::new(10.0, 0.0) },
                TouchEvent { key: 7, phase: Phase::Move, pos: Pos2::new(11.0, 0.0) },
                TouchEvent { key: 8, phase: Phase::Begin, pos: Pos2::new(20.0, 0.0) },
                TouchEvent { key: 8, phase: Phase::End, pos: Pos2::new(20.0, 0.0) },
                TouchEvent { key: 7, phase: Phase::End, pos: Pos2::new(12.0, 0.0) },
            ]
        );
        assert!(input.collect(&[click(true, 1.0)]).is_empty(), "y sigue ignorándose después");
    }

    /// Ventana vertical de 360×640 con margen (origen en 16,16).
    fn portrait() -> Rect {
        Rect::from_min_max(Pos2::new(16.0, 16.0), Pos2::new(376.0, 656.0))
    }

    fn near(a: Pos2, b: Pos2) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    #[test]
    fn recto_si_la_ventana_ya_es_apaisada() {
        let land = Rect::from_min_max(Pos2::ZERO, Pos2::new(640.0, 360.0));
        assert_eq!(Transform::for_screen(land, Rotation::Left), Transform::Straight);
        assert_eq!(Transform::for_screen(land, Rotation::Right), Transform::Straight);
        assert_eq!(Transform::for_screen(portrait(), Rotation::Left), Transform::RotLeft);
        assert_eq!(Transform::for_screen(portrait(), Rotation::Right), Transform::RotRight);
        let p = Pos2::new(100.0, 50.0);
        assert_eq!(Transform::Straight.to_screen(land, p), p);
        assert_eq!(Transform::Straight.from_screen(land, p), p);
        assert_eq!(Transform::Straight.virtual_rect(land), land);
        assert_eq!(Transform::Straight.angle(), 0.0);
    }

    #[test]
    fn girado_el_espacio_virtual_es_apaisado_y_cubre_la_pantalla() {
        let s = portrait();
        for t in [Transform::RotLeft, Transform::RotRight] {
            let v = t.virtual_rect(s);
            assert_eq!(v.min, s.min);
            assert_eq!(v.size(), Vec2::new(640.0, 360.0), "ancho y alto intercambiados");
            assert_eq!(t.rect_to_screen(s, v), s, "el espacio virtual entero es la pantalla entera");
        }
    }

    #[test]
    fn giro_izquierda_borde_superior_del_movil_a_la_izquierda() {
        let s = portrait();
        let t = Transform::RotLeft;
        let v = t.virtual_rect(s);
        // arriba-izquierda del jugador = esquina superior DERECHA del móvil
        assert!(near(t.to_screen(s, v.min), s.right_top()));
        // abajo-derecha del jugador = esquina inferior izquierda del móvil
        assert!(near(t.to_screen(s, v.max), s.left_bottom()));
        // el eje X virtual (hacia la derecha del jugador) baja por la pantalla
        assert!(near(t.to_screen(s, v.min + Vec2::new(100.0, 0.0)), s.right_top() + Vec2::new(0.0, 100.0)));
        assert_eq!(t.angle(), FRAC_PI_2, "texto girado en sentido horario");
    }

    #[test]
    fn giro_derecha_borde_superior_del_movil_a_la_derecha() {
        let s = portrait();
        let t = Transform::RotRight;
        let v = t.virtual_rect(s);
        assert!(near(t.to_screen(s, v.min), s.left_bottom()));
        assert!(near(t.to_screen(s, v.max), s.right_top()));
        assert!(near(t.to_screen(s, v.min + Vec2::new(100.0, 0.0)), s.left_bottom() - Vec2::new(0.0, 100.0)));
        assert_eq!(t.angle(), -FRAC_PI_2, "texto girado en sentido antihorario");
    }

    #[test]
    fn ida_y_vuelta() {
        let s = portrait();
        for t in [Transform::Straight, Transform::RotLeft, Transform::RotRight] {
            for (x, y) in [(16.0, 16.0), (100.0, 40.0), (300.0, 200.0), (655.0, 375.0)] {
                let p = Pos2::new(x, y);
                assert!(near(t.from_screen(s, t.to_screen(s, p)), p), "{t:?} {p:?}");
                assert!(near(t.to_screen(s, t.from_screen(s, p)), p), "{t:?} {p:?}");
            }
            let r = Rect::from_min_max(Pos2::new(20.0, 30.0), Pos2::new(120.0, 80.0));
            let sr = t.rect_to_screen(s, r);
            assert!(sr.min.x <= sr.max.x && sr.min.y <= sr.max.y, "rect normalizado");
            assert!((sr.area() - r.area()).abs() < 1e-2, "misma área");
        }
    }

    #[test]
    fn recortar_texto_para_que_quepa() {
        let w = |t: &str| t.chars().count() as f32 * 10.0;
        assert_eq!(fit("PepoTech", 80.0, w), "PepoTech");
        assert_eq!(fit("PepoTech", 79.0, w), "PepoTe…", "«…» cuenta como un carácter");
        assert_eq!(fit("Pepo Tech", 50.0, w), "Pepo…", "sin espacio colgando antes de «…»");
        assert_eq!(fit("PepoTech", 5.0, w), "");
    }
}
