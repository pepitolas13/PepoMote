//! El mando (vertical, estilo Wiimote): cruceta, −/diana/+, A, 1/2,
//! multimedia, gatillo B y tira de scroll. Multitouch REAL: cada dedo se
//! sigue por su id de toque (egui::Event::Touch) con hit-test propio, así se
//! puede mantener B mientras se pulsa A. Con ratón (pruebas en PC) se usa el
//! puntero; en cuanto aparece el primer toque, el puntero sintetizado del
//! primer dedo se ignora para no contar doble.

use crate::buttons::Buttons;
use crate::link::Status;
use crate::theme;
use egui::{Align2, Color32, Event, FontId, Pos2, Rect, RichText, Rounding, Sense, Stroke, TouchPhase, Vec2};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub enum Action {
    None,
    Exit,
    Mode(&'static str),
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
    Recenter,
    Scroll,
}

struct Touch {
    target: Target,
    start: Instant,
    last: Pos2,
    recentered: bool,
}

const MOUSE_ID: u64 = u64::MAX;
const RECENTER_HOLD: Duration = Duration::from_millis(150);

pub struct ControllerUi {
    hits: Vec<(Shape, Target)>,
    touches: HashMap<u64, Touch>,
    touch_seen: bool,
    show_media: bool,
}

impl Default for ControllerUi {
    fn default() -> Self {
        Self::new()
    }
}

impl ControllerUi {
    pub fn new() -> Self {
        Self {
            hits: Vec::new(),
            touches: HashMap::new(),
            touch_seen: false,
            show_media: false,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, buttons: &Buttons, status: &Status, show_chips: bool, sensor_hz: f32) -> Action {
        let mut action = Action::None;

        // Cabecera
        ui.horizontal(|ui| {
            ui.vertical(|ui| match status {
                Status::Connected { pc_name, mode, slot, rtt_ms } => {
                    let title = if *slot > 0 {
                        format!("{pc_name} · Jugador {}", slot + 1)
                    } else {
                        pc_name.clone()
                    };
                    ui.label(RichText::new(title).size(17.0).strong().color(theme::TEXT));
                    let mut line = if mode == "dolphin" {
                        "Dolphin".to_owned()
                    } else if *slot > 0 {
                        "Puntero: apunta el Jugador 1".to_owned()
                    } else {
                        "Puntero".to_owned()
                    };
                    if let Some(r) = rtt_ms {
                        line.push_str(&format!(" · {r:.0} ms"));
                    }
                    if sensor_hz > 0.0 {
                        line.push_str(&format!(" · {sensor_hz:.0} Hz"));
                    }
                    ui.label(RichText::new(line).size(13.0).color(theme::TEXT_DIM));
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

        if show_chips {
            if let Status::Connected { mode, .. } = status {
                ui.horizontal(|ui| {
                    if ui.selectable_label(mode != "dolphin", RichText::new("  Puntero  ").size(14.0)).clicked() {
                        action = Action::Mode("pointer");
                    }
                    if ui.selectable_label(mode == "dolphin", RichText::new("  Dolphin  ").size(14.0)).clicked() {
                        action = Action::Mode("dolphin");
                    }
                });
            }
        }

        // Cuerpo del mando: un lienzo con hit-test propio
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
        let cx = rect.center().x - 14.0 * s; // sitio para la tira de scroll
        let mut y = rect.top() + 6.0 * s;

        // Cruceta
        let arm = 56.0 * s;
        let pad_c = Pos2::new(cx, y + arm * 1.5);
        let arms = [
            (Vec2::new(0.0, -arm), "▲", pmp::BTN_DPAD_UP),
            (Vec2::new(0.0, arm), "▼", pmp::BTN_DPAD_DOWN),
            (Vec2::new(-arm, 0.0), "◀", pmp::BTN_DPAD_LEFT),
            (Vec2::new(arm, 0.0), "▶", pmp::BTN_DPAD_RIGHT),
        ];
        for (off, label, bit) in arms {
            let r = Rect::from_center_size(pad_c + off, Vec2::splat(arm));
            let down = pressed & bit != 0;
            painter.rect(
                r.shrink(2.0),
                Rounding::same(10.0),
                if down { theme::GLOW } else { theme::CARD },
                Stroke::new(1.0_f32, theme::CARD_BORDER),
            );
            painter.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.0 * s), theme::TEXT_DIM);
            self.hits.push((Shape::Rect(r), Target::Button(bit)));
        }
        painter.rect(
            Rect::from_center_size(pad_c, Vec2::splat(arm)).shrink(2.0),
            Rounding::same(6.0),
            theme::CARD,
            Stroke::NONE,
        );
        y += arm * 3.0 + 14.0 * s;

        // − ◎ +
        let row_y = y + 32.0 * s;
        self.circle(painter, Pos2::new(cx - 74.0 * s, row_y), 27.0 * s, "−", 20.0 * s, pmp::BTN_MINUS, pressed, false);
        self.circle(painter, Pos2::new(cx + 74.0 * s, row_y), 27.0 * s, "+", 20.0 * s, pmp::BTN_PLUS, pressed, false);
        // diana de recentrado
        let rc = Pos2::new(cx, row_y);
        let holding = self.touches.values().any(|t| t.target == Target::Recenter);
        painter.circle(rc, 32.0 * s, if holding { theme::GLOW } else { theme::CARD }, Stroke::new(1.0_f32, theme::CARD_BORDER));
        painter.circle_filled(rc, 13.0 * s, theme::BACKGROUND);
        painter.circle_filled(rc, 5.0 * s, theme::BLUE);
        self.hits.push((Shape::Circle { c: rc, r: 32.0 * s }, Target::Recenter));
        y += 64.0 * s + 14.0 * s;

        // A
        let a_r = 72.0 * s;
        self.circle(painter, Pos2::new(cx, y + a_r), a_r, "A", 44.0 * s, pmp::BTN_A, pressed, true);
        y += a_r * 2.0 + 12.0 * s;

        // 1 2
        let r12 = 26.0 * s;
        self.circle(painter, Pos2::new(cx - 36.0 * s, y + r12), r12, "1", 18.0 * s, pmp::BTN_ONE, pressed, false);
        self.circle(painter, Pos2::new(cx + 36.0 * s, y + r12), r12, "2", 18.0 * s, pmp::BTN_TWO, pressed, false);
        y += r12 * 2.0 + 8.0 * s;

        // Multimedia (plegable)
        let toggle = Rect::from_center_size(Pos2::new(cx, y + 14.0 * s), Vec2::new(150.0 * s, 26.0 * s));
        let toggle_resp = ui.interact(toggle, ui.id().with("media"), Sense::click());
        if toggle_resp.clicked() {
            self.show_media = !self.show_media;
        }
        let painter = ui.painter();
        painter.text(
            toggle.center(),
            Align2::CENTER_CENTER,
            if self.show_media { "Multimedia ▲" } else { "Multimedia ▼" },
            FontId::proportional(13.0 * s),
            theme::TEXT_DIM,
        );
        y += 28.0 * s;
        if self.show_media {
            let items = [
                ("⏮", pmp::BTN_MEDIA_PREV),
                ("🔉", pmp::BTN_MEDIA_VOL_DOWN),
                ("⏯", pmp::BTN_MEDIA_PLAY_PAUSE),
                ("🔇", pmp::BTN_MEDIA_MUTE),
                ("🔊", pmp::BTN_MEDIA_VOL_UP),
                ("⏭", pmp::BTN_MEDIA_NEXT),
            ];
            let r = 22.0 * s;
            let step = r * 2.0 + 8.0 * s;
            let x0 = cx - step * 2.5;
            for (i, (label, bit)) in items.iter().enumerate() {
                self.circle(painter, Pos2::new(x0 + step * i as f32, y + r), r, label, 15.0 * s, *bit, pressed, false);
            }
            y += r * 2.0 + 8.0 * s;
        }

        // Gatillo B: banda inferior
        let b_h = 84.0 * s;
        let b_rect = Rect::from_min_max(
            Pos2::new(rect.left(), (rect.bottom() - b_h - 8.0 * s).max(y + 4.0)),
            Pos2::new(rect.right() - 34.0 * s, (rect.bottom() - 8.0 * s).max(y + 4.0 + b_h)),
        );
        let b_down = pressed & pmp::BTN_B != 0;
        painter.rect(
            b_rect,
            Rounding::same(22.0 * s),
            if b_down { theme::BLUE_HOVER } else { theme::BLUE },
            Stroke::NONE,
        );
        painter.text(b_rect.center(), Align2::CENTER_CENTER, "B", FontId::proportional(30.0 * s), theme::CARD);
        self.hits.push((Shape::Rect(b_rect), Target::Button(pmp::BTN_B)));

        // Tira de scroll: borde derecho
        let strip = Rect::from_min_max(
            Pos2::new(rect.right() - 28.0 * s, rect.top() + rect.height() * 0.25),
            Pos2::new(rect.right() - 2.0, rect.top() + rect.height() * 0.72),
        );
        let scrolling = self.touches.values().any(|t| t.target == Target::Scroll);
        painter.rect(
            strip,
            Rounding::same(13.0 * s),
            if scrolling { theme::GLOW } else { theme::CARD_BORDER },
            Stroke::NONE,
        );
        for i in -1..=1 {
            painter.circle_filled(strip.center() + Vec2::new(0.0, 10.0 * s * i as f32), 2.5 * s, theme::TEXT_DIM);
        }
        self.hits.push((Shape::Rect(strip), Target::Scroll));
    }

    #[allow(clippy::too_many_arguments)]
    fn circle(&mut self, painter: &egui::Painter, c: Pos2, r: f32, label: &str, font: f32, bit: u32, pressed: u32, primary: bool) {
        let down = pressed & bit != 0;
        let (fill, text): (Color32, Color32) = if primary {
            (if down { theme::BLUE_HOVER } else { theme::BLUE }, theme::CARD)
        } else {
            (if down { theme::GLOW } else { theme::CARD }, theme::TEXT)
        };
        painter.circle(c, if down { r * 0.94 } else { r }, fill, Stroke::new(1.0_f32, theme::CARD_BORDER));
        painter.text(c, Align2::CENTER_CENTER, label, FontId::proportional(font), text);
        self.hits.push((Shape::Circle { c, r }, Target::Button(bit)));
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
        // Diana: mantener 150 ms → recentrar (una vez por toque)
        for t in self.touches.values_mut() {
            if t.target == Target::Recenter && !t.recentered && t.start.elapsed() >= RECENTER_HOLD {
                t.recentered = true;
                buttons.bump_recenter();
            }
        }
        if !self.touches.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn begin(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        let Some(target) = self.hit(pos) else { return };
        if let Target::Button(bit) = target {
            buttons.set(bit, true);
        }
        self.touches.insert(
            key,
            Touch {
                target,
                start: Instant::now(),
                last: pos,
                recentered: false,
            },
        );
    }

    fn moved(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        if let Some(t) = self.touches.get_mut(&key) {
            if t.target == Target::Scroll {
                // dedo hacia arriba (dy negativo) = scroll up = positivo
                buttons.add_scroll((t.last.y - pos.y).round() as i32);
            }
            t.last = pos;
        }
    }

    fn end(&mut self, key: u64, buttons: &Buttons) {
        if let Some(t) = self.touches.remove(&key) {
            if let Target::Button(bit) = t.target {
                buttons.set(bit, false);
            }
        }
    }
}
