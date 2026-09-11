//! El GamePad de Wii U (modo Cemu), apaisado: L/ZL y R/ZR en las esquinas,
//! dos sticks con su L3/R3, cruceta, A/B/X/Y en rombo, − Home +, «TV/Pad»,
//! «Soplar» y la pantalla táctil 16:9 en el centro. Se pinta a mano en un
//! espacio virtual apaisado: si la ventana ya es apaisada, recto; si es
//! vertical (Phosh sin rotación) se gira 90° según el ajuste «Giro» para
//! sostener el móvil tumbado, y los toques se destransforman. Un Pro
//! Controller (jugadores 2-4) es lo mismo sin táctil, Mic ni TV/Pad. Mismo
//! multitouch real que el mando; un dedo por stick y uno en la táctil.
//!
//! La pantalla aparece AL INSTANTE al pedir Wii U (optimista, incluso
//! conectando): hasta que el receptor confirma `cemu`, los controles se ven
//! atenuados e inertes. Bajo la cabecera, el selector «En Cemu soy:
//! GamePad / Mando de Wii» (el mismo que en el layout Wii dentro de Wii U).
//!
//! Doble pantalla: si la app tiene abierto el canal de la pantalla
//! (`screen::Client`), la zona táctil pinta la imagen del GamePad de Cemu
//! (textura con filtro lineal, escalada exacta a la zona y girada con ella);
//! sin imagen, el fondo y la etiqueta de siempre más la línea de estado. El
//! mapeo táctil no cambia: sigue siendo la fracción de la zona.

use crate::buttons::Buttons;
use crate::frame::Rotation;
use crate::link::Status;
use crate::screen;
use crate::theme;
use crate::ui::nunchuk::{knob_pos, stick_value};
use crate::ui::touch::{self, fit_rect, Canvas, Input, Phase, Seg, Shape, Transform};
use egui::{Align2, Color32, FontId, ImageData, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::time::Duration;

pub enum Action {
    None,
    Exit,
    Mode(&'static str),
    /// Pedir al receptor otro tipo de mando (`"wiimote"` = ser Mando de Wii).
    Pad(&'static str),
    /// Nuevo giro del móvil apaisado (ajuste + atómico).
    Rotation(Rotation),
}

/// Lo que la app pasa cada frame.
pub struct Inputs<'a> {
    pub status: &'a Status,
    pub rotation: Rotation,
    /// Chips de modo (solo el jugador 1, y no en «solo Dolphin»).
    pub show_chips: bool,
    /// Intención Wii U pendiente: pantalla optimista (Wii U marcado,
    /// controles inertes hasta el eco).
    pub optimistic: bool,
    /// Petición de `pad` sin eco: su segmento a medio tono.
    pub pad_pending: Option<&'a str>,
    pub sensor_hz: f32,
    /// Canal de la pantalla del GamePad (doble pantalla), si está abierto.
    pub screen: Option<&'a screen::Client>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left = 0,
    Right = 1,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Chip {
    Exit,
    Mode(&'static str),
    Pad(&'static str),
    Rotate,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Button(u32),
    Stick(Side),
    Touch,
    /// Los chips y segmentos disparan al levantar el dedo encima.
    Chip(Chip),
}

struct StickUi {
    /// Centro y recorrido (virtuales) del último frame.
    c: Pos2,
    travel: f32,
    /// Pomo que se pinta (−1..1, +Y abajo); al centro al soltar.
    knob: Vec2,
}

/// Lo que se pinta de la sesión.
struct View<'a> {
    pc_name: &'a str,
    player: u8,
    /// `pad == "pro"`: sin táctil, Mic ni TV/Pad.
    pro: bool,
    /// Tipo de mando que dice el receptor ("gamepad", "pro" o "wiimote").
    pad: &'a str,
    mode: &'a str,
    show_chips: bool,
    supports_cemu: bool,
    /// El receptor ha confirmado Wii U y somos GamePad/Pro: se juega.
    active: bool,
    optimistic: bool,
    pending: Option<&'a str>,
    notice: Option<&'a str>,
    rtt_ms: Option<f32>,
    /// Canal de la pantalla del GamePad, si la app lo tiene abierto.
    screen: Option<&'a screen::Client>,
}

pub struct GamePadUi {
    hits: Vec<(Shape, Target)>,
    touches: HashMap<u64, Target>,
    input: Input,
    transform: Transform,
    screen: Rect,
    sticks: [StickUi; 2],
    touch_rect: Rect,
    active: bool,
    fired: Option<Chip>,
    /// La pantalla del GamePad de Cemu (se crea con la primera imagen y se
    /// actualiza con cada una; se suelta sin canal).
    texture: Option<TextureHandle>,
}

impl Default for GamePadUi {
    fn default() -> Self {
        Self::new()
    }
}

/// Posición dentro de la pantalla táctil → fracción 0..65535 por eje
/// (recortada al rectángulo).
pub fn touch_fraction(r: Rect, p: Pos2) -> (u16, u16) {
    let f = |v: f32, lo: f32, len: f32| -> u16 {
        if len <= 0.0 {
            return 0;
        }
        (((v - lo) / len).clamp(0.0, 1.0) * 65535.0).round() as u16
    };
    (f(p.x, r.left(), r.width()), f(p.y, r.top(), r.height()))
}

/// Velo sobre los controles cuando aún no se juega (conectando o esperando
/// el eco de Wii U).
const VEIL: Color32 = Color32::from_rgba_premultiplied(0xF4, 0xF6, 0xF7, 0xA0);

impl GamePadUi {
    pub fn new() -> Self {
        let stick = || StickUi {
            c: Pos2::ZERO,
            travel: 1.0,
            knob: Vec2::ZERO,
        };
        Self {
            hits: Vec::new(),
            touches: HashMap::new(),
            input: Input::default(),
            transform: Transform::Straight,
            screen: Rect::NOTHING,
            sticks: [stick(), stick()],
            touch_rect: Rect::NOTHING,
            active: false,
            fired: None,
            texture: None,
        }
    }

    /// Zona táctil en píxeles físicos (ancho, alto) tal como se maquetó la
    /// última vez; `None` antes del primer frame. Es lo que se pide al
    /// receptor como tamaño máximo de la pantalla.
    pub fn touch_size_px(&self, pixels_per_point: f32) -> Option<(f32, f32)> {
        self.touch_rect
            .is_positive()
            .then(|| (self.touch_rect.width() * pixels_per_point, self.touch_rect.height() * pixels_per_point))
    }

    /// Sube a la textura la imagen que haya dejado el hilo de la pantalla
    /// (`load_texture` la primera vez, `set` después; filtro lineal). Sin
    /// canal, la textura se suelta.
    fn upload(&mut self, ctx: &egui::Context, screen: Option<&screen::Client>) {
        let Some(c) = screen else {
            self.texture = None;
            return;
        };
        if let Some(img) = c.take_image() {
            let data = ImageData::Color(img);
            match &mut self.texture {
                Some(t) => t.set(data, TextureOptions::LINEAR),
                None => self.texture = Some(ctx.load_texture("pantalla-gamepad", data, TextureOptions::LINEAR)),
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, buttons: &Buttons, inp: &Inputs) -> Action {
        let avail = ui.available_size();
        let (rect, _) = ui.allocate_exact_size(avail, Sense::hover());
        self.screen = rect;
        self.transform = Transform::for_screen(rect, inp.rotation);
        self.upload(ui.ctx(), inp.screen);
        let notice = inp.status.live_notice();
        let view = match inp.status {
            Status::Connected {
                pc_name,
                mode,
                player,
                rtt_ms,
                supports_cemu,
                pad,
                ..
            } => View {
                pc_name,
                player: *player,
                pro: pad == "pro",
                pad,
                mode,
                show_chips: inp.show_chips,
                supports_cemu: *supports_cemu,
                active: mode == "cemu" && pad != "wiimote",
                optimistic: inp.optimistic,
                pending: inp.pad_pending,
                notice,
                rtt_ms: *rtt_ms,
                screen: inp.screen,
            },
            other => View {
                pc_name: if matches!(other, Status::Connecting) { "Conectando…" } else { "Sin conexión" },
                player: 1,
                pro: false,
                pad: "gamepad",
                mode: "",
                show_chips: false,
                supports_cemu: false,
                active: false,
                optimistic: inp.optimistic,
                pending: None,
                notice,
                rtt_ms: None,
                screen: None,
            },
        };
        self.active = view.active;
        self.hits.clear();
        {
            let cv = Canvas::new(ui.painter(), rect, self.transform);
            self.layout(&cv, buttons, &view, inp.rotation, inp.sensor_hz);
        }
        self.process_events(ui.ctx(), buttons);
        if !self.active {
            // que el velo y la cabecera se refresquen en cuanto llegue el eco
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let current_pad = if view.pad == "wiimote" { "wiimote" } else { "gamepad" };
        match self.fired.take() {
            Some(Chip::Exit) => Action::Exit,
            Some(Chip::Mode(m)) => Action::Mode(m),
            Some(Chip::Pad(p)) if p != current_pad => Action::Pad(p),
            Some(Chip::Pad(_)) => Action::None,
            Some(Chip::Rotate) => Action::Rotation(inp.rotation.toggled()),
            None => Action::None,
        }
    }

    fn layout(&mut self, cv: &Canvas, buttons: &Buttons, v: &View, rotation: Rotation, sensor_hz: f32) {
        let pressed = buttons.physical();
        let r = cv.rect();
        let (vw, vh) = (r.width(), r.height());
        // Escala: referencia 700×370 pt virtuales (un móvil apaisado)
        let s = (vh / 370.0).min(vw / 700.0).clamp(0.5, 1.6);
        let (x0, y0) = (r.left(), r.top());
        let cx = r.center().x;

        // ---- Cabecera: chips desde la derecha, el nombre con lo que quede ----
        let hh = 30.0 * s;
        let hy = y0 + hh / 2.0;
        let chip_h = 24.0 * s;
        let chip_font = 12.0 * s;
        let gap = 6.0 * s;
        let mut right = r.right() - 4.0 * s;
        let mut place = |w: f32| -> Rect {
            let rc = Rect::from_min_max(Pos2::new(right - w, hy - chip_h / 2.0), Pos2::new(right, hy + chip_h / 2.0));
            right -= w + gap;
            rc
        };
        self.chip(cv, place(52.0 * s), "Salir", chip_font, false, theme::ERROR, Chip::Exit);
        let giro = match rotation {
            Rotation::Left => "Giro ◀",
            Rotation::Right => "Giro ▶",
        };
        self.chip(cv, place(66.0 * s), giro, chip_font, false, theme::TEXT, Chip::Rotate);
        if v.show_chips {
            // esta ES la pantalla Wii U: su chip va marcado (también mientras
            // se espera el eco); los otros dos, por igualdad exacta
            if v.supports_cemu || v.optimistic {
                let on = v.mode == "cemu" || v.optimistic;
                self.chip(cv, place(58.0 * s), "Wii U", chip_font, on, theme::TEXT, Chip::Mode("cemu"));
            }
            self.chip(cv, place(66.0 * s), "Dolphin", chip_font, v.mode == "dolphin" && !v.optimistic, theme::TEXT, Chip::Mode("dolphin"));
            self.chip(cv, place(66.0 * s), "Puntero", chip_font, v.mode == "pointer" && !v.optimistic, theme::TEXT, Chip::Mode("pointer"));
        }
        let title_font = FontId::proportional(13.0 * s);
        let kind = if v.pro { "Pro Controller" } else { "GamePad" };
        let mut line = format!("{} · J{} · {kind}", v.pc_name, v.player);
        if let Some(ms) = v.rtt_ms {
            line.push_str(&format!(" · {ms:.0} ms"));
        }
        if sensor_hz > 0.0 {
            line.push_str(&format!(" · {sensor_hz:.0} Hz"));
        }
        if let Some(c) = v.screen {
            let fps = c.fps();
            if c.showing() && fps > 0.0 {
                line.push_str(&format!(" · pantalla · {fps:.0} fps"));
            }
        }
        let avail = right - x0 - 4.0 * s;
        cv.text(Pos2::new(x0 + 4.0 * s, hy), Align2::LEFT_CENTER, &cv.fit_text(&line, title_font.clone(), avail), title_font, theme::TEXT);

        // ---- Selector «En Cemu soy» bajo la cabecera, centrado ----
        let sel_h = 26.0 * s;
        let sel_y = y0 + hh + 2.0 * s;
        let label_font = FontId::proportional(12.0 * s);
        let label = "En Cemu soy:";
        let label_w = cv.text_width(label, label_font.clone());
        let seg_w = 104.0 * s;
        let total = label_w + 8.0 * s + seg_w * 2.0 + 4.0 * s;
        let mut x = cx - total / 2.0;
        cv.text(Pos2::new(x, sel_y + sel_h / 2.0), Align2::LEFT_CENTER, label, label_font, theme::TEXT_DIM);
        x += label_w + 8.0 * s;
        let wii = v.pad == "wiimote";
        let seg = |key: &str, on: bool| -> Seg {
            if v.pending == Some(key) {
                Seg::Pending
            } else if on && v.pending.is_none() {
                Seg::On
            } else {
                Seg::Off
            }
        };
        let r1 = Rect::from_min_size(Pos2::new(x, sel_y), Vec2::new(seg_w, sel_h));
        let r2 = Rect::from_min_size(Pos2::new(x + seg_w + 4.0 * s, sel_y), Vec2::new(seg_w, sel_h));
        let sh = touch::segment(cv, r1, kind, chip_font, seg("gamepad", !wii));
        self.hits.push((sh, Target::Chip(Chip::Pad("gamepad"))));
        let sh = touch::segment(cv, r2, "Mando de Wii", chip_font, seg("wiimote", wii));
        self.hits.push((sh, Target::Chip(Chip::Pad("wiimote"))));

        // ---- Cuerpo ----
        let by = sel_y + sel_h + 6.0 * s;
        let lx = x0 + (0.15 * vw).max(60.0 * s);
        let rx = r.right() - (lx - x0);

        // Hombros: L sobre ZL en la esquina izquierda, R sobre ZR en la derecha
        let (sw, sh_) = (96.0 * s, 24.0 * s);
        let l = Rect::from_min_size(Pos2::new(x0 + 6.0 * s, by), Vec2::new(sw, sh_));
        let zl = l.translate(Vec2::new(0.0, sh_ + 4.0 * s));
        let rr = Rect::from_min_size(Pos2::new(r.right() - 6.0 * s - sw, by), Vec2::new(sw, sh_));
        let zr = rr.translate(Vec2::new(0.0, sh_ + 4.0 * s));
        for (rc, label, bit) in [(l, "L", pmp::BTN_L), (zl, "ZL", pmp::BTN_ZL), (rr, "R", pmp::BTN_R), (zr, "ZR", pmp::BTN_ZR)] {
            let shape = touch::rect_button(cv, rc, 8.0 * s, label, 13.0 * s, pressed & bit != 0, false);
            self.hits.push((shape, Target::Button(bit)));
        }

        // Sticks (con su click L3/R3 pegado; van antes en los hits porque el
        // anillo tiene margen)
        let ring_r = 46.0 * s;
        let sy = by + 2.0 * sh_ + 14.0 * s + ring_r;
        let l3 = Pos2::new(lx + ring_r + 18.0 * s, sy + 20.0 * s);
        let r3 = Pos2::new(rx - ring_r - 18.0 * s, sy + 20.0 * s);
        self.button(cv, l3, 14.0 * s, "L3", 10.0 * s, pmp::BTN_STICK_L, pressed, false);
        self.button(cv, r3, 14.0 * s, "R3", 10.0 * s, pmp::BTN_STICK_R, pressed, false);
        self.stick(cv, Side::Left, Pos2::new(lx, sy), ring_r, s);
        self.stick(cv, Side::Right, Pos2::new(rx, sy), ring_r, s);

        // Cruceta bajo el stick izquierdo
        let arm = 30.0 * s;
        let dc = Pos2::new(lx, sy + ring_r + 10.0 * s + arm * 1.5);
        let arms = [
            (Vec2::new(0.0, -arm), "▲", pmp::BTN_DPAD_UP),
            (Vec2::new(0.0, arm), "▼", pmp::BTN_DPAD_DOWN),
            (Vec2::new(-arm, 0.0), "◀", pmp::BTN_DPAD_LEFT),
            (Vec2::new(arm, 0.0), "▶", pmp::BTN_DPAD_RIGHT),
        ];
        for (off, label, bit) in arms {
            let rc = Rect::from_center_size(dc + off, Vec2::splat(arm));
            let down = pressed & bit != 0;
            cv.rounded_rect(
                rc.shrink(2.0),
                8.0 * s,
                if down { theme::GLOW } else { theme::CARD },
                Stroke::new(1.0_f32, theme::CARD_BORDER),
            );
            cv.text(rc.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.0 * s), theme::TEXT_DIM);
            self.hits.push((Shape::Rect(rc), Target::Button(bit)));
        }
        cv.rounded_rect(Rect::from_center_size(dc, Vec2::splat(arm)).shrink(2.0), 5.0 * s, theme::CARD, Stroke::NONE);

        // A/B/X/Y en rombo bajo el stick derecho (A a la derecha, azul)
        let ac = Pos2::new(rx, dc.y);
        let (br, off) = (21.0 * s, 31.0 * s);
        self.button(cv, ac + Vec2::new(0.0, -off), br, "X", 16.0 * s, pmp::BTN_X, pressed, false);
        self.button(cv, ac + Vec2::new(-off, 0.0), br, "Y", 16.0 * s, pmp::BTN_Y, pressed, false);
        self.button(cv, ac + Vec2::new(0.0, off), br, "B", 16.0 * s, pmp::BTN_B, pressed, false);
        self.button(cv, ac + Vec2::new(off, 0.0), br, "A", 16.0 * s, pmp::BTN_A, pressed, true);

        // Centro: pantalla táctil 16:9 (solo GamePad) y las filas de sistema
        let zone_l = lx + ring_r + 36.0 * s;
        let zone_r = rx - ring_r - 36.0 * s;
        let zone_w = (zone_r - zone_l).max(60.0 * s);
        let rows_y = if v.pro {
            let y = by + (r.bottom() - by) / 2.0 - 10.0 * s;
            cv.text(Pos2::new(cx, y - 44.0 * s), Align2::CENTER_CENTER, "Pro Controller", FontId::proportional(12.0 * s), theme::TEXT_DIM);
            y
        } else {
            let tw = (zone_w - 8.0 * s).min(0.42 * vw);
            let th = tw * 9.0 / 16.0;
            let tr = Rect::from_min_size(Pos2::new(cx - tw / 2.0, by), Vec2::new(tw, th));
            self.touch_area(cv, tr, buttons, s, v.screen);
            tr.bottom() + 6.0 * s + 20.0 * s
        };
        let br2 = 20.0 * s;
        self.button(cv, Pos2::new(cx - 62.0 * s, rows_y), br2, "−", 18.0 * s, pmp::BTN_MINUS, pressed, false);
        self.button(cv, Pos2::new(cx, rows_y), br2, "Home", 10.0 * s, pmp::BTN_HOME, pressed, false);
        self.button(cv, Pos2::new(cx + 62.0 * s, rows_y), br2, "+", 18.0 * s, pmp::BTN_PLUS, pressed, false);
        if !v.pro {
            let ry = rows_y + br2 + 6.0 * s;
            let (bw, bh) = (76.0 * s, 24.0 * s);
            let tv = Rect::from_min_size(Pos2::new(cx - 44.0 * s - bw / 2.0, ry), Vec2::new(bw, bh));
            let mic = Rect::from_min_size(Pos2::new(cx + 44.0 * s - bw / 2.0, ry), Vec2::new(bw, bh));
            for (rc, label, bit) in [(tv, "TV/Pad", pmp::BTN_SCREEN), (mic, "Soplar", pmp::BTN_MIC)] {
                let shape = touch::rect_button(cv, rc, 8.0 * s, label, 11.0 * s, pressed & bit != 0, false);
                self.hits.push((shape, Target::Button(bit)));
            }
        }

        // Aún no se juega: velo sobre los controles y el porqué
        if !v.active {
            let body = Rect::from_min_max(Pos2::new(x0, by - 2.0 * s), r.max);
            cv.rounded_rect(body, 0.0, VEIL, Stroke::NONE);
            let why = if v.mode.is_empty() {
                "Conectando…"
            } else if v.pad == "wiimote" {
                "Ahora eres Mando de Wii"
            } else {
                "Cambiando el PC a Wii U…"
            };
            cv.text(Pos2::new(cx, by + (r.bottom() - by) * 0.42), Align2::CENTER_CENTER, why, FontId::proportional(15.0 * s), theme::TEXT_DIM);
        }

        // Aviso transitorio del receptor (o local)
        if let Some(n) = v.notice {
            let w = (0.5 * vw).min(360.0 * s).max(zone_w);
            let nr = Rect::from_center_size(Pos2::new(cx, r.bottom() - 22.0 * s), Vec2::new(w, 28.0 * s));
            cv.rounded_rect(nr, 14.0 * s, theme::CARD, Stroke::new(1.5_f32, theme::WARN));
            let font = FontId::proportional(12.0 * s);
            cv.text(nr.center(), Align2::CENTER_CENTER, &cv.fit_text(n, font.clone(), w - 16.0 * s), font, theme::TEXT);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn button(&mut self, cv: &Canvas, c: Pos2, r: f32, label: &str, font: f32, bit: u32, pressed: u32, primary: bool) {
        let shape = touch::circle_button(cv, c, r, label, font, pressed & bit != 0, primary);
        self.hits.push((shape, Target::Button(bit)));
    }

    #[allow(clippy::too_many_arguments)]
    fn chip(&mut self, cv: &Canvas, rc: Rect, label: &str, font: f32, selected: bool, color: Color32, target: Chip) {
        let shape = touch::chip(cv, rc, label, font, selected, color);
        self.hits.push((shape, Target::Chip(target)));
    }

    fn stick(&mut self, cv: &Canvas, side: Side, c: Pos2, ring_r: f32, s: f32) {
        let knob_r = ring_r * 0.40;
        // el borde del pomo llega justo al anillo
        let travel = ring_r - knob_r;
        let i = side as usize;
        self.sticks[i].c = c;
        self.sticks[i].travel = travel;
        let knob = self.sticks[i].knob;
        let held = self.touches.values().any(|t| *t == Target::Stick(side));
        cv.circle(c, ring_r, theme::CARD, Stroke::new(1.5_f32, if held { theme::BLUE } else { theme::CARD_BORDER }));
        cv.circle_filled(c, 3.0 * s, theme::CARD_BORDER);
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            cv.circle_filled(c + Vec2::new(dx, dy) * (ring_r - 8.0 * s), 2.5 * s, theme::CARD_BORDER);
        }
        cv.circle(
            c + knob * travel,
            knob_r,
            if held { theme::BLUE_HOVER } else { theme::BLUE },
            Stroke::new(1.0_f32, theme::CARD_BORDER),
        );
        self.hits.push((Shape::Circle { c, r: ring_r + 10.0 * s }, Target::Stick(side)));
    }

    /// La zona táctil 16:9: con la doble pantalla en marcha, la imagen del
    /// GamePad de Cemu escalada exacta a la zona (bandas si no es 16:9); si
    /// no, el fondo y la etiqueta de siempre más la línea de estado del canal.
    fn touch_area(&mut self, cv: &Canvas, tr: Rect, buttons: &Buttons, s: f32, screen: Option<&screen::Client>) {
        let (tx, ty, down) = buttons.touch();
        let border = Stroke::new(1.0_f32, if down { theme::BLUE } else { theme::CARD_BORDER });
        let live = screen.is_some_and(|c| c.showing());
        match (&self.texture, live) {
            (Some(tex), true) => {
                cv.rounded_rect(tr, 6.0 * s, Color32::BLACK, Stroke::NONE);
                cv.image(fit_rect(tr, tex.size()), tex.id());
                cv.rounded_rect(tr, 6.0 * s, Color32::TRANSPARENT, border);
            }
            _ => {
                cv.rounded_rect(tr, 6.0 * s, theme::CARD, border);
                cv.text(
                    Pos2::new(tr.center().x, tr.top() + 5.0 * s),
                    Align2::CENTER_TOP,
                    "Pantalla táctil",
                    FontId::proportional(11.0 * s),
                    theme::TEXT_DIM,
                );
                if let Some(c) = screen {
                    let font = FontId::proportional(12.0 * s);
                    let text = cv.fit_text(&c.placeholder(), font.clone(), tr.width() - 12.0 * s);
                    cv.text(tr.center(), Align2::CENTER_CENTER, &text, font, theme::TEXT_DIM);
                }
            }
        }
        if down {
            let p = Pos2::new(
                tr.left() + tx as f32 / 65535.0 * tr.width(),
                tr.top() + ty as f32 / 65535.0 * tr.height(),
            );
            cv.circle_filled(p, 6.0 * s, theme::BLUE);
        }
        self.hits.push((Shape::Rect(tr), Target::Touch));
        self.touch_rect = tr;
    }

    fn process_events(&mut self, ctx: &egui::Context, buttons: &Buttons) {
        for ev in self.input.events(ctx) {
            // los dedos llegan en pantalla; el layout vive en el espacio virtual
            let pos = self.transform.from_screen(self.screen, ev.pos);
            match ev.phase {
                Phase::Begin => self.begin(ev.key, pos, buttons),
                Phase::Move => self.moved(ev.key, pos, buttons),
                Phase::End => self.end(ev.key, pos, buttons),
            }
        }
        if !self.touches.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn begin(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        let Some(target) = touch::hit_test(&self.hits, pos) else { return };
        // sin Wii U confirmado solo responden la cabecera y el selector
        if !self.active && !matches!(target, Target::Chip(_)) {
            return;
        }
        match target {
            Target::Button(bit) => buttons.set(bit, true),
            Target::Stick(side) => {
                // un solo dedo lleva cada stick; el segundo se ignora
                if self.touches.values().any(|t| *t == target) {
                    return;
                }
                self.drag(side, pos, buttons);
            }
            Target::Touch => {
                if self.touches.values().any(|t| *t == Target::Touch) {
                    return;
                }
                self.touch_at(pos, buttons);
            }
            Target::Chip(_) => {}
        }
        self.touches.insert(key, target);
    }

    fn moved(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        match self.touches.get(&key).copied() {
            Some(Target::Stick(side)) => self.drag(side, pos, buttons),
            Some(Target::Touch) => self.touch_at(pos, buttons),
            _ => {}
        }
    }

    fn end(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        match self.touches.remove(&key) {
            Some(Target::Button(bit)) => buttons.set(bit, false),
            Some(Target::Stick(side)) => {
                // al soltar, al centro
                self.sticks[side as usize].knob = Vec2::ZERO;
                match side {
                    Side::Left => buttons.set_stick(0, 0),
                    Side::Right => buttons.set_stick2(0, 0),
                }
            }
            Some(Target::Touch) => {
                let (x, y, _) = buttons.touch();
                buttons.set_touch(x, y, false);
            }
            // un chip dispara solo si el dedo se levanta sobre el mismo chip
            Some(Target::Chip(c)) if touch::hit_test(&self.hits, pos) == Some(Target::Chip(c)) => {
                self.fired = Some(c);
            }
            _ => {}
        }
    }

    /// Dedo en `pos` (virtual) → pomo y valor en el cable del stick `side`.
    fn drag(&mut self, side: Side, pos: Pos2, buttons: &Buttons) {
        let st = &mut self.sticks[side as usize];
        let d = pos - st.c;
        st.knob = knob_pos(d.x, d.y, st.travel);
        let (x, y) = stick_value(st.knob);
        match side {
            Side::Left => buttons.set_stick(x, y),
            Side::Right => buttons.set_stick2(x, y),
        }
    }

    fn touch_at(&self, pos: Pos2, buttons: &Buttons) {
        let (x, y) = touch_fraction(self.touch_rect, pos);
        buttons.set_touch(x, y, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraccion_de_la_pantalla_tactil() {
        let r = Rect::from_min_max(Pos2::new(100.0, 50.0), Pos2::new(300.0, 162.5));
        assert_eq!(touch_fraction(r, r.min), (0, 0), "arriba a la izquierda");
        assert_eq!(touch_fraction(r, r.max), (65535, 65535), "abajo a la derecha");
        assert_eq!(touch_fraction(r, r.center()), (0x8000, 0x8000), "centro (como el vector dorado)");
        assert_eq!(touch_fraction(r, Pos2::new(200.0, 78.125)), (0x8000, 0x4000));
        assert_eq!(touch_fraction(r, Pos2::new(-5.0, 999.0)), (0, 65535), "fuera: recortado");
        assert_eq!(touch_fraction(Rect::NOTHING, Pos2::new(1.0, 1.0)), (0, 0), "sin rectángulo no hay fracción");
    }
}
