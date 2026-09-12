//! El mando (vertical, estilo Wiimote): cruceta, −/diana/+, A, 1/2,
//! multimedia, gatillo B y tira de scroll. Multitouch REAL: cada dedo se
//! sigue por su id de toque (ui/touch.rs) con hit-test propio, así se puede
//! mantener B mientras se pulsa A. Con ratón (pruebas en PC) se usa el
//! puntero; en cuanto aparece el primer toque, el puntero sintetizado del
//! primer dedo se ignora para no contar doble.

use crate::buttons::Buttons;
use crate::link::Status;
use crate::theme;
use crate::ui::touch::{self, Canvas, Input, Phase, Shape, Transform};
use egui::{Align2, FontId, Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub enum Action {
    None,
    Exit,
    Mode(&'static str),
    /// Pedir al receptor otro tipo de mando en modo Wii U (`"gamepad"` o
    /// `"wiimote"`, desde el selector «En Cemu soy»).
    Pad(&'static str),
    /// Abrir el teclado para el teclado en pantalla de Cemu (modo Wii U).
    Keyboard,
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

const RECENTER_HOLD: Duration = Duration::from_millis(150);

pub struct ControllerUi {
    hits: Vec<(Shape, Target)>,
    touches: HashMap<u64, Touch>,
    input: Input,
    show_media: bool,
}

impl Default for ControllerUi {
    fn default() -> Self {
        Self::new()
    }
}

/// Nombre del modo del receptor para la cabecera.
pub fn mode_label(mode: &str) -> String {
    match mode {
        "pointer" => "Puntero".to_owned(),
        "dolphin" => "Dolphin".to_owned(),
        "cemu" => "Wii U".to_owned(),
        other => other.to_owned(),
    }
}

/// Selector segmentado «En Cemu soy: [GamePad|Pro Controller] [Mando de
/// Wii]» (modo Wii U). El activo va en azul; el pedido y sin eco, a medio
/// tono (`pending`). Devuelve el `pad` a pedir al tocar el otro segmento.
pub fn pad_selector(ui: &mut egui::Ui, player: u8, pad: &str, pending: Option<&str>) -> Option<&'static str> {
    let mut out = None;
    let wii = pad == "wiimote";
    ui.label(RichText::new("En Cemu soy:").size(13.0).color(theme::text_dim()));
    ui.horizontal(|ui| {
        let w = ((ui.available_width() - 8.0) / 2.0).max(90.0);
        let seg = |ui: &mut egui::Ui, label: &str, on: bool, pend: bool| -> bool {
            let (fill, color) = if on {
                (theme::blue(), theme::ON_ACCENT)
            } else if pend {
                (theme::glow(), theme::text())
            } else {
                (theme::card(), theme::text())
            };
            ui.add_sized(
                Vec2::new(w, 44.0),
                egui::Button::new(RichText::new(label).size(15.0).color(color))
                    .fill(fill)
                    .stroke(Stroke::new(1.0_f32, theme::card_border())),
            )
            .clicked()
        };
        let first = if player == 1 { "GamePad" } else { "Pro Controller" };
        if seg(ui, first, !wii && pending.is_none(), pending == Some("gamepad")) && wii {
            out = Some("gamepad");
        }
        if seg(ui, "Mando de Wii", wii && pending.is_none(), pending == Some("wiimote")) && !wii {
            out = Some("wiimote");
        }
    });
    out
}

/// Banner del aviso transitorio (`notice`) bajo la cabecera.
pub fn notice_banner(ui: &mut egui::Ui, status: &Status) {
    if let Some(n) = status.live_notice() {
        egui::Frame::none()
            .fill(theme::card())
            .stroke(Stroke::new(1.5_f32, theme::warn()))
            .rounding(Rounding::same(12.0))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(n).size(13.0).color(theme::text()));
            });
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
}

impl ControllerUi {
    pub fn new() -> Self {
        Self {
            hits: Vec::new(),
            touches: HashMap::new(),
            input: Input::default(),
            show_media: false,
        }
    }

    /// Suelta todos los dedos (al tapar la pantalla con el teclado: los
    /// toques que sigan no llegarán aquí).
    pub fn release(&mut self, buttons: &Buttons) {
        self.touches.clear();
        buttons.release_all();
    }

    /// `pad_pending`: tipo de mando pedido al receptor y aún sin eco (el
    /// selector lo pinta a medio tono).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        buttons: &Buttons,
        status: &Status,
        show_chips: bool,
        pad_pending: Option<&str>,
        sensor_hz: f32,
    ) -> Action {
        let mut action = Action::None;

        // Cabecera
        ui.horizontal(|ui| {
            ui.vertical(|ui| match status {
                Status::Connected { pc_name, mode, slot, player, rtt_ms, .. } => {
                    let title = if *slot > 0 {
                        format!("{pc_name} · Jugador {player}")
                    } else {
                        pc_name.clone()
                    };
                    ui.label(RichText::new(title).size(17.0).strong().color(theme::text()));
                    let mut line = match mode.as_str() {
                        "cemu" => "Wii U · Mando de Wii".to_owned(),
                        "pointer" if *slot > 0 => "Puntero: apunta el Jugador 1".to_owned(),
                        m => mode_label(m),
                    };
                    if let Some(r) = rtt_ms {
                        line.push_str(&format!(" · {r:.0} ms"));
                    }
                    if sensor_hz > 0.0 {
                        line.push_str(&format!(" · {sensor_hz:.0} Hz"));
                    }
                    ui.label(RichText::new(line).size(13.0).color(theme::text_dim()));
                }
                Status::Connecting => {
                    ui.label(RichText::new("Conectando…").size(17.0).strong().color(theme::text()));
                }
                _ => {
                    ui.label(RichText::new("Sin conexión").size(17.0).strong().color(theme::text()));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("Salir").size(14.0).color(theme::error()))
                    .clicked()
                {
                    action = Action::Exit;
                }
                // Mando de Wii dentro de Wii U: el teclado para el teclado en
                // pantalla de Cemu, como en el GamePad
                if matches!(status, Status::Connected { mode, .. } if mode == "cemu")
                    && ui.button(RichText::new("Teclado").size(14.0).color(theme::text())).clicked()
                {
                    action = Action::Keyboard;
                }
            });
        });

        if let Status::Connected { mode, supports_cemu, player, pad, .. } = status {
            let wiiu = mode == "cemu";
            if show_chips {
                ui.horizontal(|ui| {
                    // selección por igualdad exacta del modo
                    if ui.selectable_label(mode == "pointer", RichText::new("  Puntero  ").size(14.0)).clicked() {
                        action = Action::Mode("pointer");
                    }
                    if ui.selectable_label(mode == "dolphin", RichText::new("  Dolphin  ").size(14.0)).clicked() {
                        action = Action::Mode("dolphin");
                    }
                    if *supports_cemu && ui.selectable_label(wiiu, RichText::new("  Wii U  ").size(14.0)).clicked() {
                        action = Action::Mode("cemu");
                    }
                });
            }
            if wiiu {
                // Mando de Wii dentro de Wii U: el mismo selector que en el GamePad
                if let Some(p) = pad_selector(ui, *player, pad, pad_pending) {
                    action = Action::Pad(p);
                }
                ui.label(
                    RichText::new("Para juegos de Wii U que se juegan con el mando de Wii (Wii Sports Club, Wii Party U…)")
                        .size(11.0)
                        .color(theme::text_dim()),
                );
            }
        }
        notice_banner(ui, status);

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
        let cv = Canvas::new(painter, rect, Transform::Straight);
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
                if down { theme::glow() } else { theme::card() },
                Stroke::new(1.0_f32, theme::card_border()),
            );
            painter.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.0 * s), theme::text_dim());
            self.hits.push((Shape::Rect(r), Target::Button(bit)));
        }
        painter.rect(
            Rect::from_center_size(pad_c, Vec2::splat(arm)).shrink(2.0),
            Rounding::same(6.0),
            theme::card(),
            Stroke::NONE,
        );
        y += arm * 3.0 + 14.0 * s;

        // − ◎ +
        let row_y = y + 32.0 * s;
        self.circle(&cv, Pos2::new(cx - 74.0 * s, row_y), 27.0 * s, "−", 20.0 * s, pmp::BTN_MINUS, pressed, false);
        self.circle(&cv, Pos2::new(cx + 74.0 * s, row_y), 27.0 * s, "+", 20.0 * s, pmp::BTN_PLUS, pressed, false);
        // diana de recentrado
        let rc = Pos2::new(cx, row_y);
        let holding = self.touches.values().any(|t| t.target == Target::Recenter);
        painter.circle(rc, 32.0 * s, if holding { theme::glow() } else { theme::card() }, Stroke::new(1.0_f32, theme::card_border()));
        painter.circle_filled(rc, 13.0 * s, theme::background());
        painter.circle_filled(rc, 5.0 * s, theme::blue());
        self.hits.push((Shape::Circle { c: rc, r: 32.0 * s }, Target::Recenter));
        y += 64.0 * s + 14.0 * s;

        // A
        let a_r = 72.0 * s;
        self.circle(&cv, Pos2::new(cx, y + a_r), a_r, "A", 44.0 * s, pmp::BTN_A, pressed, true);
        y += a_r * 2.0 + 12.0 * s;

        // 1 2
        let r12 = 26.0 * s;
        self.circle(&cv, Pos2::new(cx - 36.0 * s, y + r12), r12, "1", 18.0 * s, pmp::BTN_ONE, pressed, false);
        self.circle(&cv, Pos2::new(cx + 36.0 * s, y + r12), r12, "2", 18.0 * s, pmp::BTN_TWO, pressed, false);
        y += r12 * 2.0 + 8.0 * s;

        // Multimedia (plegable)
        let toggle = Rect::from_center_size(Pos2::new(cx, y + 14.0 * s), Vec2::new(150.0 * s, 26.0 * s));
        let toggle_resp = ui.interact(toggle, ui.id().with("media"), Sense::click());
        if toggle_resp.clicked() {
            self.show_media = !self.show_media;
        }
        painter.text(
            toggle.center(),
            Align2::CENTER_CENTER,
            if self.show_media { "Multimedia ▲" } else { "Multimedia ▼" },
            FontId::proportional(13.0 * s),
            theme::text_dim(),
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
                self.circle(&cv, Pos2::new(x0 + step * i as f32, y + r), r, label, 15.0 * s, *bit, pressed, false);
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
            if b_down { theme::blue_hover() } else { theme::blue() },
            Stroke::NONE,
        );
        painter.text(b_rect.center(), Align2::CENTER_CENTER, "B", FontId::proportional(30.0 * s), theme::ON_ACCENT);
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
            if scrolling { theme::glow() } else { theme::card_border() },
            Stroke::NONE,
        );
        for i in -1..=1 {
            painter.circle_filled(strip.center() + Vec2::new(0.0, 10.0 * s * i as f32), 2.5 * s, theme::text_dim());
        }
        self.hits.push((Shape::Rect(strip), Target::Scroll));
    }

    #[allow(clippy::too_many_arguments)]
    fn circle(&mut self, cv: &Canvas, c: Pos2, r: f32, label: &str, font: f32, bit: u32, pressed: u32, primary: bool) {
        let shape = touch::circle_button(cv, c, r, label, font, pressed & bit != 0, primary);
        self.hits.push((shape, Target::Button(bit)));
    }

    fn process_events(&mut self, ctx: &egui::Context, buttons: &Buttons) {
        for ev in self.input.events(ctx) {
            match ev.phase {
                Phase::Begin => self.begin(ev.key, ev.pos, buttons),
                Phase::Move => self.moved(ev.key, ev.pos, buttons),
                Phase::End => self.end(ev.key, buttons),
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
        let Some(target) = touch::hit_test(&self.hits, pos) else { return };
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
