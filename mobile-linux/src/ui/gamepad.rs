//! GamePad de Wii U y Pro Controller de Switch, apaisados: L/ZL y R/ZR en las esquinas,
//! dos sticks con su L3/R3, cruceta, A/B/X/Y en rombo, − Home +, «TV/Pad»,
//! «Soplar» y la pantalla táctil 16:9 en el centro. Se pinta a mano en un
//! espacio virtual apaisado: si la ventana ya es apaisada, recto; si es
//! vertical (Phosh sin rotación) se gira 90° según el ajuste «Giro» para
//! sostener el móvil tumbado, y los toques se destransforman. Un Pro
//! Controller (jugadores 2-4) es lo mismo sin táctil, Mic ni TV/Pad; sin
//! táctil (Pro, o el ajuste «GamePad sin pantalla táctil») stick y cruceta
//! van en fila, tan grandes como quepan, y − Home + en columna en el centro.
//! Mismo multitouch real que el mando; un dedo por stick y uno en la táctil.
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
use crate::ui::dpad;
use crate::ui::nunchuk::{knob_pos, stick_value};
use crate::ui::touch::{self, fit_rect, Canvas, Input, Phase, Press, Seg, Shape, Transform};
use egui::{Align2, Color32, FontId, ImageData, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::time::Duration;
use crate::tr;

pub enum Action {
    None,
    Exit,
    Mode(&'static str),
    /// Pedir al receptor otro tipo de mando (`"wiimote"` = ser Mando de Wii;
    /// en RetroArch `"retropad"`, `"nes"` o `"gun"`).
    Pad(&'static str),
    /// Nuevo giro del móvil apaisado (ajuste + atómico).
    Rotation(Rotation),
    /// Abrir el teclado para el teclado en pantalla de Cemu.
    Keyboard,
    /// RetroArch: tecla rápida de un toque (nombre del protocolo). Las de
    /// mantener se leen con [`GamePadUi::held_hotkeys`].
    Hotkey(&'static str),
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
    pub wanted_mode: &'a str,
    /// Petición de `pad` sin eco: su segmento a medio tono.
    pub pad_pending: Option<&'a str>,
    pub sensor_hz: f32,
    /// Canal de la pantalla del GamePad (doble pantalla), si está abierto.
    pub screen: Option<&'a screen::Client>,
    /// Ajuste «GamePad sin pantalla táctil»: sin zona táctil, botones más grandes.
    pub no_screen: bool,
    /// Ajuste «pantalla completa»: solo la pantalla de Cemu y el táctil (con
    /// Wii U confirmado y como GamePad; si no, el trazado de siempre).
    pub full_screen: bool,
    /// En pantalla completa, botón de teclado arriba a la derecha.
    pub keyboard_button: bool,
    /// Los dos ajustes de pulsación (deslizar / mantener al salir).
    pub press: Press,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    Left = 0,
    Right = 1,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Chip {
    Exit,
    Mode(&'static str),
    Pad(&'static str),
    Rotate,
    Keyboard,
    /// RetroArch: tecla rápida de un toque (dispara al levantar el dedo).
    Hotkey(&'static str),
    /// RetroArch: tecla de mantener (pulsada mientras el dedo esté encima).
    Hold(&'static str),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Button(u32),
    /// Un dedo que no lleva nada: con «pulsar deslizando» pulsará el botón en
    /// el que entre. Solo se registra con ese ajuste encendido.
    Free,
    Stick(Side),
    Touch,
    /// La cruceta de una pieza (un dedo, que lleva sus bits mientras está apoyado).
    Dpad,
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
    /// Sin zona táctil: Pro Controller o el ajuste «GamePad sin pantalla».
    no_screen: bool,
    /// Tipo de mando que dice el receptor ("gamepad", "pro" o "wiimote").
    pad: &'a str,
    mode: &'a str,
    show_chips: bool,
    supports_cemu: bool,
    supports_switch: bool,
    supports_retroarch: bool,
    switch: bool,
    /// RetroArch: el mismo trazado que Switch con etiquetas de RetroPad
    /// (L2/R2, Select/Start, Menú, Rápido) y su selector de tres mandos.
    retro: bool,
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
    /// Centro y media anchura de la cruceta del último frame, y sus bits pulsados.
    dpad: (Pos2, f32),
    dpad_held: u32,
    touch_rect: Rect,
    /// Zona cuyo tamaño se pide al receptor: la táctil de siempre o, en
    /// pantalla completa, toda el área (la imagen se ajusta dentro).
    zone_rect: Rect,
    active: bool,
    fired: Option<Chip>,
    /// RetroArch: teclas de mantener bajo algún dedo (rebobinar).
    held: Vec<&'static str>,
    /// Los dos ajustes de pulsación (deslizar / mantener al salir).
    press: Press,
    /// La pantalla del GamePad de Cemu (se crea con la primera imagen y se
    /// actualiza con cada una; se suelta sin canal).
    texture: Option<TextureHandle>,
    layout_key: Option<String>,
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
/// el eco de Wii U): el fondo del tema, translúcido.
fn veil() -> Color32 {
    let b = theme::background();
    Color32::from_rgba_unmultiplied(b.r(), b.g(), b.b(), 0xA0)
}

/// Medidas del trazado en fila (sin pantalla táctil), en unidades virtuales.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowMetrics {
    /// Diámetro del stick = lado de la cruceta = lado del rombo.
    pub pad: f32,
    /// Radio de A/B/X/Y (el rombo cabe en `pad`).
    pub face_r: f32,
    /// Anchura de la columna central (−, Home, +; TV/Pad y Soplar si es GamePad).
    pub center_w: f32,
}

/// Cuatro pads en fila bajo los gatillos: tan grandes como dejen la altura
/// que queda (huecos de 6·s arriba y abajo) y la anchura (10·s entre stick y
/// cruceta/rombo, 6·s al centro y a los bordes). Misma idea que `PadMetrics`
/// en iOS/Android.
pub fn row_metrics(vw: f32, body_h: f32, s: f32, pro: bool) -> RowMetrics {
    let center_w = if pro { 40.0 * s } else { 76.0 * s };
    let ph = body_h - (2.0 * 24.0 * s + 4.0 * s) - 12.0 * s;
    let pw = (vw - center_w - 44.0 * s) / 4.0;
    let pad = ph.min(pw).max(40.0 * s);
    RowMetrics { pad, face_r: pad / 5.2, center_w }
}

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
            dpad: (Pos2::ZERO, 1.0),
            dpad_held: 0,
            input: Input::default(),
            transform: Transform::Straight,
            screen: Rect::NOTHING,
            sticks: [stick(), stick()],
            touch_rect: Rect::NOTHING,
            zone_rect: Rect::NOTHING,
            active: false,
            fired: None,
            held: Vec::new(),
            press: Press::default(),
            texture: None,
            layout_key: None,
        }
    }

    /// Zona táctil en píxeles físicos (ancho, alto) tal como se maquetó la
    /// última vez; `None` antes del primer frame. Es lo que se pide al
    /// receptor como tamaño máximo de la pantalla.
    pub fn touch_size_px(&self, pixels_per_point: f32) -> Option<(f32, f32)> {
        self.zone_rect
            .is_positive()
            .then(|| (self.zone_rect.width() * pixels_per_point, self.zone_rect.height() * pixels_per_point))
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
        let ext = matches!(inp.wanted_mode, "switch" | "retroarch");
        self.upload(ui.ctx(), if ext { None } else { inp.screen });
        let notice = inp.status.live_notice();
        let view = match inp.status {
            Status::Connected {
                pc_name,
                mode,
                player,
                rtt_ms,
                supports_cemu,
                supports_switch,
                supports_retroarch,
                pad,
                ..
            } => View {
                pc_name,
                player: *player,
                pro: pad == "pro",
                no_screen: ext || pad == "pro" || inp.no_screen,
                pad: if inp.wanted_mode == "switch" { "pro" } else if inp.wanted_mode == "retroarch" && !crate::link::is_retro_pad(pad) { "retropad" } else { pad },
                mode: if inp.optimistic { inp.wanted_mode } else { mode },
                show_chips: inp.show_chips,
                supports_cemu: *supports_cemu,
                supports_switch: *supports_switch,
                supports_retroarch: *supports_retroarch,
                switch: inp.wanted_mode == "switch",
                retro: inp.wanted_mode == "retroarch",
                active: inp.status.ext_confirmed() && mode == inp.wanted_mode,
                optimistic: inp.optimistic,
                pending: inp.pad_pending,
                notice,
                rtt_ms: *rtt_ms,
                screen: if ext { None } else { inp.screen },
            },
            other => View {
                pc_name: match other {
                    Status::Connecting => tr!("common.connecting"),
                    Status::Reconnecting { .. } => tr!("common.reconnecting"),
                    _ => tr!("common.no_connection"),
                },
                player: 1,
                pro: false,
                no_screen: ext || inp.no_screen,
                pad: match inp.wanted_mode { "switch" => "pro", "retroarch" => "retropad", _ => "gamepad" },
                mode: inp.wanted_mode,
                show_chips: false,
                supports_cemu: false,
                supports_switch: false,
                supports_retroarch: false,
                switch: inp.wanted_mode == "switch",
                retro: inp.wanted_mode == "retroarch",
                active: false,
                optimistic: inp.optimistic,
                pending: None,
                notice,
                rtt_ms: None,
                screen: None,
            },
        };
        self.transform = Transform::for_screen(rect, inp.rotation);
        let key = format!("{}/{}/{:?}/{}/{}/{}", view.mode, view.pad, self.transform, view.active, inp.full_screen, view.no_screen);
        if self.layout_key.as_ref() != Some(&key) {
            self.release(buttons);
            self.layout_key = Some(key);
        }
        self.active = view.active;
        self.press = inp.press;
        self.hits.clear();
        {
            let cv = Canvas::new(ui.painter(), rect, self.transform);
            // pantalla completa solo con Wii U confirmado y como GamePad con
            // pantalla; si no, el trazado de siempre (con cabecera y «Salir»)
            let full = inp.full_screen && view.active && !view.no_screen && view.pad == "gamepad";
            if full {
                self.layout_full(&cv, buttons, &view, inp.keyboard_button);
            } else {
                self.layout(&cv, buttons, &view, inp.rotation, inp.sensor_hz);
            }
        }
        self.process_events(ui.ctx(), buttons);
        if !self.active {
            // que el velo y la cabecera se refresquen en cuanto llegue el eco
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let current_pad = if view.switch || view.retro { view.pad } else if view.pad == "wiimote" { "wiimote" } else { "gamepad" };
        match self.fired.take() {
            Some(Chip::Exit) => Action::Exit,
            Some(Chip::Mode(m)) => Action::Mode(m),
            Some(Chip::Pad(p)) if p != current_pad => Action::Pad(p),
            Some(Chip::Pad(_)) => Action::None,
            Some(Chip::Rotate) => Action::Rotation(inp.rotation.toggled()),
            Some(Chip::Keyboard) => Action::Keyboard,
            Some(Chip::Hotkey(n)) => Action::Hotkey(n),
            Some(Chip::Hold(_)) => Action::None,
            None => Action::None,
        }
    }

    /// Suelta todos los dedos (al tapar la pantalla con el teclado: los
    /// toques que sigan no llegarán aquí).
    pub fn release(&mut self, buttons: &Buttons) {
        self.touches.clear();
        self.dpad_held = 0;
        self.fired = None;
        for st in &mut self.sticks {
            st.knob = Vec2::ZERO;
        }
        buttons.release_all();
    }

    fn layout(&mut self, cv: &Canvas, buttons: &Buttons, v: &View, rotation: Rotation, sensor_hz: f32) {
        let pressed = buttons.physical();
        let r = cv.rect();
        let (vw, vh) = (r.width(), r.height());
        // Escala: referencia 700×370 pt virtuales (un móvil apaisado)
        let s = (vh / 370.0).min(vw / 700.0).clamp(0.5, 1.6);
        let (x0, y0) = (r.left(), r.top());
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
        self.chip(cv, place(52.0 * s), tr!("common.exit"), chip_font, false, theme::error(), Chip::Exit);
        let giro = match rotation {
            Rotation::Left => tr!("gp.rotate_left"),
            Rotation::Right => tr!("gp.rotate_right"),
        };
        self.chip(cv, place(66.0 * s), giro, chip_font, false, theme::text(), Chip::Rotate);
        if matches!(v.mode, "cemu" | "switch" | "retroarch") {
            // teclado del móvil para el teclado en pantalla de Cemu (GamePad y Pro) o de RetroArch
            self.chip(cv, place(64.0 * s), tr!("common.keyboard"), chip_font, false, theme::text(), Chip::Keyboard);
        }
        if v.show_chips {
            // esta ES la pantalla Wii U: su chip va marcado (también mientras
            // se espera el eco); los otros dos, por igualdad exacta
            if v.supports_retroarch || v.retro {
                self.chip(cv, place(74.0 * s), tr!("common.mode_retroarch"), chip_font, v.mode == "retroarch", theme::text(), Chip::Mode("retroarch"));
            }
            if v.supports_switch || v.switch {
                self.chip(cv, place(62.0 * s), tr!("common.mode_switch"), chip_font, v.mode == "switch", theme::text(), Chip::Mode("switch"));
            }
            if v.supports_cemu || (!v.switch && v.optimistic) {
                let on = v.mode == "cemu";
                self.chip(cv, place(58.0 * s), tr!("common.mode_cemu"), chip_font, on, theme::text(), Chip::Mode("cemu"));
            }
            self.chip(cv, place(66.0 * s), tr!("common.mode_dolphin"), chip_font, v.mode == "dolphin" && !v.optimistic, theme::text(), Chip::Mode("dolphin"));
            self.chip(cv, place(66.0 * s), tr!("common.mode_pointer"), chip_font, v.mode == "pointer" && !v.optimistic, theme::text(), Chip::Mode("pointer"));
        }
        let title_font = FontId::proportional(13.0 * s);
        let kind = Self::kind(v);
        let mut line = tr!("gp.title", v.pc_name, v.player, kind);
        if let Some(ms) = v.rtt_ms {
            line.push_str(&format!(" · {ms:.0} ms"));
        }
        if sensor_hz > 0.0 {
            line.push_str(&format!(" · {sensor_hz:.0} Hz"));
        }
        if let Some(c) = v.screen {
            let fps = c.fps();
            if c.showing() && fps > 0.0 {
                line.push_str(&tr!("gp.screen_fps", fps.round() as i64));
            }
        }
        let avail = right - x0 - 4.0 * s;
        cv.text(Pos2::new(x0 + 4.0 * s, hy), Align2::LEFT_CENTER, &cv.fit_text(&line, title_font.clone(), avail), title_font, theme::text());

        let sel_y = y0 + hh + 2.0 * s;
        let sel_h = if v.switch { 0.0 } else if v.retro { self.retro_selector(cv, v, sel_y, s) } else { self.selector(cv, v, sel_y, s) };
        // RetroArch: fila de teclas rápidas bajo el selector
        let hk_h = if v.retro { self.hotkeys_row(cv, sel_y + sel_h + 4.0 * s, s) } else { 0.0 };

        // ---- Cuerpo ----
        let by = sel_y + sel_h + hk_h + 6.0 * s;
        // sin zona táctil pintada no hay tamaño que pedir al receptor
        self.touch_rect = Rect::NOTHING;
        self.zone_rect = Rect::NOTHING;

        // Hombros: L sobre ZL en la esquina izquierda, R sobre ZR en la derecha
        let (sw, sh_) = (96.0 * s, 24.0 * s);
        let l = Rect::from_min_size(Pos2::new(x0 + 6.0 * s, by), Vec2::new(sw, sh_));
        let zl = l.translate(Vec2::new(0.0, sh_ + 4.0 * s));
        let rr = Rect::from_min_size(Pos2::new(r.right() - 6.0 * s - sw, by), Vec2::new(sw, sh_));
        let zr = rr.translate(Vec2::new(0.0, sh_ + 4.0 * s));
        let (zl_label, zr_label) = if v.retro { ("L2", "R2") } else { ("ZL", "ZR") };
        for (rc, label, bit) in [(l, "L", pmp::BTN_L), (zl, zl_label, pmp::BTN_ZL), (rr, "R", pmp::BTN_R), (zr, zr_label, pmp::BTN_ZR)] {
            let shape = touch::rect_button(cv, rc, 8.0 * s, label, 13.0 * s, pressed & bit != 0, false);
            self.hits.push((shape, Target::Button(bit)));
        }

        // Sin pantalla táctil (Pro Controller o el ajuste): stick y cruceta en
        // fila si así salen más grandes que apilados (en un móvil, siempre)
        let row = v.no_screen.then(|| row_metrics(vw, r.bottom() - by, s, v.pro && !v.switch && !v.retro)).filter(|m| m.pad > 92.0 * s);
        let zone_w = if let Some(m) = row {
            self.layout_row(cv, v, &m, s, pressed, r, by);
            m.center_w
        } else {
            self.layout_stacked(cv, buttons, v, s, pressed, r, by)
        };

        self.overlay(cv, v, r, by, s, zone_w);
    }

    fn overlay(&self, cv: &Canvas, v: &View, r: Rect, by: f32, s: f32, zone_w: f32) {
        let (x0, cx, vw) = (r.left(), r.center().x, r.width());
        // Aún no se juega: velo sobre los controles y el porqué
        if !v.active {
            let body = Rect::from_min_max(Pos2::new(x0, by - 2.0 * s), r.max);
            cv.rounded_rect(body, 0.0, veil(), Stroke::NONE);
            let why = if v.switch {
                tr!("gp.activating_switch")
            } else if v.retro {
                tr!("gp.activating_retroarch")
            } else if v.mode.is_empty() {
                tr!("common.connecting")
            } else if v.pad == "wiimote" {
                tr!("gp.veil_wiimote")
            } else {
                tr!("gp.veil_switching")
            };
            cv.text(Pos2::new(cx, by + (r.bottom() - by) * 0.42), Align2::CENTER_CENTER, why, FontId::proportional(15.0 * s), theme::text_dim());
        }

        // Aviso transitorio del receptor (o local)
        if let Some(n) = v.notice {
            let w = (0.5 * vw).min(360.0 * s).max(zone_w);
            let nr = Rect::from_center_size(Pos2::new(cx, r.bottom() - 22.0 * s), Vec2::new(w, 28.0 * s));
            cv.rounded_rect(nr, 14.0 * s, theme::card(), Stroke::new(1.5_f32, theme::warn()));
            let font = FontId::proportional(12.0 * s);
            cv.text(nr.center(), Align2::CENTER_CENTER, &cv.fit_text(n, font.clone(), w - 16.0 * s), font, theme::text());
        }
    }

    fn kind(v: &View) -> &'static str {
        if v.retro {
            match v.pad { "nes" => tr!("common.nes_pad"), "gun" => tr!("common.light_gun"), _ => tr!("common.retropad") }
        } else if v.switch || v.pro {
            tr!("common.pro")
        } else {
            tr!("common.gamepad")
        }
    }

    /// «En RetroArch soy: [RetroPad] [NES] [Pistola]».
    fn retro_selector(&mut self, cv: &Canvas, v: &View, y: f32, s: f32) -> f32 {
        let r = cv.rect();
        let h = 26.0 * s;
        let gap = 4.0 * s;
        let font = 12.0 * s;
        let label = tr!("common.in_retroarch");
        let choices = [(tr!("common.retropad"), "retropad"), (tr!("common.nes_pad"), "nes"), (tr!("common.light_gun"), "gun")];
        let label_w = cv.text_width(label, FontId::proportional(font)) + 8.0 * s;
        let available = r.width() - 12.0 * s - label_w;
        let w = ((available - 2.0 * gap) / 3.0).min(96.0 * s);
        let total = label_w + 3.0 * w + 2.0 * gap;
        let x = r.center().x - total / 2.0;
        cv.text(Pos2::new(x, y + h / 2.0), Align2::LEFT_CENTER, label, FontId::proportional(font), theme::text_dim());
        for (i, (label, pad)) in choices.iter().enumerate() {
            let rc = Rect::from_min_size(Pos2::new(x + label_w + i as f32 * (w + gap), y), Vec2::new(w, h));
            let seg = if v.pending == Some(*pad) { Seg::Pending } else if v.pad == *pad && v.pending.is_none() { Seg::On } else { Seg::Off };
            let size = font.min(font * (w - 10.0 * s) / cv.text_width(label, FontId::proportional(font)).max(1.0));
            let shape = touch::segment(cv, rc, label, size, seg);
            self.hits.push((shape, Target::Chip(Chip::Pad(pad))));
        }
        h
    }

    /// RetroArch: guardar/cargar estado, ranura, rebobinar (mantener), pausa,
    /// captura y reiniciar, en una fila de chips bajo el selector.
    fn hotkeys_row(&mut self, cv: &Canvas, y: f32, s: f32) -> f32 {
        let r = cv.rect();
        let h = 22.0 * s;
        let gap = 4.0 * s;
        let font = 11.0 * s;
        let n = crate::ui::controller::RETRO_HOTKEYS.len() as f32;
        let w = ((r.width() - 12.0 * s - (n - 1.0) * gap) / n).min(80.0 * s);
        let total = n * w + (n - 1.0) * gap;
        let mut x = r.center().x - total / 2.0;
        for (name, hold) in crate::ui::controller::RETRO_HOTKEYS {
            let rc = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
            let label = crate::ui::controller::hotkey_label(name);
            let size = font.min(font * (w - 8.0 * s) / cv.text_width(label, FontId::proportional(font)).max(1.0));
            let selected = hold && self.held.contains(&name);
            let target = if hold { Chip::Hold(name) } else { Chip::Hotkey(name) };
            let shape = touch::chip(cv, rc, label, size, selected, theme::text());
            self.hits.push((shape, Target::Chip(target)));
            x += w + gap;
        }
        h
    }

    /// RetroArch: las teclas de mantener bajo algún dedo ahora mismo.
    pub fn held_hotkeys(&self) -> Vec<&'static str> {
        self.held.clone()
    }

    /// Wii U retains GamePad / Wii Remote choices; Switch is always Pro.
    fn selector(&mut self, cv: &Canvas, v: &View, y: f32, s: f32) -> f32 {
        let r = cv.rect();
        let h = 26.0 * s;
        let gap = 4.0 * s;
        let font = 12.0 * s;
        let label = tr!("common.in_cemu");
        let choices = [(Self::kind(v), "gamepad"), (tr!("common.wiimote"), "wiimote")];
        let label_w = cv.text_width(label, FontId::proportional(font)) + 8.0 * s;
        let available = r.width() - 12.0 * s - label_w;
        let w = ((available - gap) / 2.0).min(104.0 * s);
        let total = label_w + 2.0 * w + gap;
        let x = r.center().x - total / 2.0;
        cv.text(Pos2::new(x, y + h / 2.0), Align2::LEFT_CENTER, label, FontId::proportional(font), theme::text_dim());
        let current = if v.pad == "wiimote" { "wiimote" } else { "gamepad" };
        for (i, (label, pad)) in choices.iter().enumerate() {
            let rc = Rect::from_min_size(Pos2::new(x + label_w + i as f32 * (w + gap), y), Vec2::new(w, h));
            let seg = if v.pending == Some(*pad) { Seg::Pending } else if current == *pad && v.pending.is_none() { Seg::On } else { Seg::Off };
            let size = font.min(font * (w - 10.0 * s) / cv.text_width(label, FontId::proportional(font)).max(1.0));
            let shape = touch::segment(cv, rc, label, size, seg);
            self.hits.push((shape, Target::Chip(Chip::Pad(pad))));
        }
        h
    }

    /// Etiqueta y tamaño de letra de −, Home y +: en RetroArch, Select, Menú y Start.
    fn center_labels(v: &View) -> ((&'static str, f32), (&'static str, f32), (&'static str, f32)) {
        if v.retro {
            ((tr!("gp.select"), 8.5), (tr!("gp.menu"), 9.0), (tr!("gp.start"), 8.5))
        } else {
            (("−", 18.0), ("Home", 10.0), ("+", 18.0))
        }
    }

    /// La pastilla bajo −/Home/+: Capturar en Switch, avance rápido en RetroArch.
    fn pill_label(v: &View) -> &'static str {
        if v.retro { tr!("gp.fast") } else { tr!("gp.capture") }
    }

    fn pill(&mut self, cv: &Canvas, r: Rect, label: &str, bit: u32, s: f32, pressed: u32) {
        let shape = touch::rect_button(cv, r, 9.0 * s, label, 13.0 * s, pressed & bit != 0, false);
        self.hits.push((shape, Target::Button(bit)));
    }

    fn layout_stacked(&mut self, cv: &Canvas, buttons: &Buttons, v: &View, s: f32, pressed: u32, r: Rect, by: f32) -> f32 {
        let (x0, cx, vw) = (r.left(), r.center().x, r.width());
        let sh_ = 24.0 * s;
        let lx = x0 + (0.15 * vw).max(60.0 * s);
        let rx = r.right() - (lx - x0);

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

        // Cruceta bajo el stick izquierdo; A/B/X/Y en rombo bajo el derecho
        let arm = 30.0 * s;
        let dc = Pos2::new(lx, sy + ring_r + 10.0 * s + arm * 1.5);
        self.dpad(cv, dc, arm, s, pressed);
        self.face(cv, Pos2::new(rx, dc.y), 21.0 * s, 31.0 * s, s, pressed);

        // Centro: pantalla táctil 16:9 (solo GamePad con pantalla) y las filas de sistema
        let zone_l = lx + ring_r + 36.0 * s;
        let zone_r = rx - ring_r - 36.0 * s;
        let zone_w = (zone_r - zone_l).max(60.0 * s);
        let rows_y = if v.no_screen {
            let y = by + (r.bottom() - by) / 2.0 - 10.0 * s;
            if v.pro {
                cv.text(Pos2::new(cx, y - 44.0 * s), Align2::CENTER_CENTER, tr!("common.pro"), FontId::proportional(12.0 * s), theme::text_dim());
            }
            y
        } else {
            let tw = (zone_w - 8.0 * s).min(0.42 * vw);
            let th = tw * 9.0 / 16.0;
            let tr = Rect::from_min_size(Pos2::new(cx - tw / 2.0, by), Vec2::new(tw, th));
            self.touch_area(cv, tr, buttons, s, v.screen);
            tr.bottom() + 6.0 * s + 20.0 * s
        };
        let br2 = 20.0 * s;
        let (minus, home, plus) = Self::center_labels(v);
        self.button(cv, Pos2::new(cx - 62.0 * s, rows_y), br2, minus.0, minus.1 * s, pmp::BTN_MINUS, pressed, false);
        self.button(cv, Pos2::new(cx, rows_y), br2, home.0, home.1 * s, pmp::BTN_HOME, pressed, false);
        self.button(cv, Pos2::new(cx + 62.0 * s, rows_y), br2, plus.0, plus.1 * s, pmp::BTN_PLUS, pressed, false);
        if v.switch || v.retro {
            self.pill(cv, Rect::from_center_size(Pos2::new(cx, rows_y + 50.0 * s), Vec2::new(76.0 * s, 24.0 * s)), Self::pill_label(v), pmp::BTN_SCREEN, s, pressed);
        } else if !v.pro {
            let ry = rows_y + br2 + 6.0 * s;
            let (bw, bh) = (76.0 * s, 24.0 * s);
            let tv = Rect::from_min_size(Pos2::new(cx - 44.0 * s - bw / 2.0, ry), Vec2::new(bw, bh));
            let mic = Rect::from_min_size(Pos2::new(cx + 44.0 * s - bw / 2.0, ry), Vec2::new(bw, bh));
            for (rc, label, bit) in [(tv, tr!("gp.tv_pad"), pmp::BTN_SCREEN), (mic, tr!("gp.blow"), pmp::BTN_MIC)] {
                let shape = touch::rect_button(cv, rc, 8.0 * s, label, 11.0 * s, pressed & bit != 0, false);
                self.hits.push((shape, Target::Button(bit)));
            }
        }
        zone_w
    }

    /// Trazado en fila (sin pantalla táctil): L3/R3 junto a los gatillos;
    /// [stick][cruceta] a la izquierda y [rombo][stick] a la derecha, pegados
    /// abajo y tan grandes como dejen la anchura y la altura; −, Home, + (y
    /// TV/Pad, Soplar) en columna en el centro.
    #[allow(clippy::too_many_arguments)]
    fn layout_row(&mut self, cv: &Canvas, v: &View, m: &RowMetrics, s: f32, pressed: u32, r: Rect, by: f32) {
        let (x0, cx) = (r.left(), r.center().x);
        let (sw, sh_) = (96.0 * s, 24.0 * s);
        // L3 a la derecha de L/ZL y R3 a la izquierda de R/ZR, a la altura de las dos filas
        let cy = by + sh_ + 2.0 * s;
        let l3 = Pos2::new(x0 + 6.0 * s + sw + 6.0 * s + 14.0 * s, cy);
        let r3 = Pos2::new(r.right() - 6.0 * s - sw - 6.0 * s - 14.0 * s, cy);
        self.button(cv, l3, 14.0 * s, "L3", 10.0 * s, pmp::BTN_STICK_L, pressed, false);
        self.button(cv, r3, 14.0 * s, "R3", 10.0 * s, pmp::BTN_STICK_R, pressed, false);
        // Pads, pegados abajo (los sticks antes en los hits: el anillo tiene margen)
        let half = m.pad / 2.0;
        let py = r.bottom() - 6.0 * s - half;
        let ls = Pos2::new(x0 + 6.0 * s + half, py);
        let dc = Pos2::new(ls.x + m.pad + 10.0 * s, py);
        let rs = Pos2::new(r.right() - 6.0 * s - half, py);
        let ac = Pos2::new(rs.x - m.pad - 10.0 * s, py);
        self.stick(cv, Side::Left, ls, half, s);
        self.stick(cv, Side::Right, rs, half, s);
        self.dpad(cv, dc, m.pad / 3.0, s, pressed);
        self.face(cv, ac, m.face_r, half - m.face_r, s, pressed);
        // Centro: columna −, Home, + (y TV/Pad, Soplar), centrada en el cuerpo
        let br2 = 20.0 * s;
        let (bw, bh) = (76.0 * s, 24.0 * s);
        let gap = 6.0 * s;
        let total = 6.0 * br2 + 2.0 * gap + if v.switch || v.retro { bh + gap } else if v.pro { 0.0 } else { 2.0 * (bh + gap) };
        let mut y = by + (r.bottom() - by - total) / 2.0;
        let (minus, home, plus) = Self::center_labels(v);
        for ((label, font), bit) in [(minus, pmp::BTN_MINUS), (home, pmp::BTN_HOME), (plus, pmp::BTN_PLUS)] {
            self.button(cv, Pos2::new(cx, y + br2), br2, label, font * s, bit, pressed, false);
            y += 2.0 * br2 + gap;
        }
        if v.switch || v.retro {
            self.pill(cv, Rect::from_min_size(Pos2::new(cx - bw / 2.0, y), Vec2::new(bw, bh)), Self::pill_label(v), pmp::BTN_SCREEN, s, pressed);
        } else if !v.pro {
            for (label, bit) in [(tr!("gp.tv_pad"), pmp::BTN_SCREEN), (tr!("gp.blow"), pmp::BTN_MIC)] {
                let rc = Rect::from_min_size(Pos2::new(cx - bw / 2.0, y), Vec2::new(bw, bh));
                let shape = touch::rect_button(cv, rc, 8.0 * s, label, 11.0 * s, pressed & bit != 0, false);
                self.hits.push((shape, Target::Button(bit)));
                y += bh + gap;
            }
        }
    }

    /// Cruceta de una pieza centrada en `dc` con brazos de lado `arm` (media
    /// anchura de la cruz = 1,5 brazos); el cuadrado entero es su hit-test.
    fn dpad(&mut self, cv: &Canvas, dc: Pos2, arm: f32, s: f32, pressed: u32) {
        let half = arm * 1.5;
        let shape = dpad::draw(cv, dc, half, s, pressed);
        self.dpad = (dc, half);
        self.hits.push((shape, Target::Dpad));
    }

    /// A/B/X/Y en rombo alrededor de `ac`: radio `br`, a `off` del centro (A a
    /// la derecha, azul; el texto crece con el botón).
    fn face(&mut self, cv: &Canvas, ac: Pos2, br: f32, off: f32, _s: f32, pressed: u32) {
        let font = br * (16.0 / 21.0);
        self.button(cv, ac + Vec2::new(0.0, -off), br, "X", font, pmp::BTN_X, pressed, false);
        self.button(cv, ac + Vec2::new(-off, 0.0), br, "Y", font, pmp::BTN_Y, pressed, false);
        self.button(cv, ac + Vec2::new(0.0, off), br, "B", font, pmp::BTN_B, pressed, false);
        self.button(cv, ac + Vec2::new(off, 0.0), br, "A", font, pmp::BTN_A, pressed, true);
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
        cv.circle(c, ring_r, theme::card(), Stroke::new(1.5_f32, if held { theme::blue() } else { theme::card_border() }));
        cv.circle_filled(c, 3.0 * s, theme::card_border());
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            cv.circle_filled(c + Vec2::new(dx, dy) * (ring_r - 8.0 * s), 2.5 * s, theme::card_border());
        }
        cv.circle(
            c + knob * travel,
            knob_r,
            if held { theme::blue_hover() } else { theme::blue() },
            Stroke::new(1.0_f32, theme::card_border()),
        );
        self.hits.push((Shape::Circle { c, r: ring_r + 10.0 * s }, Target::Stick(side)));
    }

    /// La zona táctil 16:9: con la doble pantalla en marcha, la imagen del
    /// GamePad de Cemu escalada exacta a la zona (bandas si no es 16:9); si
    /// no, el fondo y la etiqueta de siempre más la línea de estado del canal.
    fn touch_area(&mut self, cv: &Canvas, tr: Rect, buttons: &Buttons, s: f32, screen: Option<&screen::Client>) {
        let (tx, ty, down) = buttons.touch();
        let border = Stroke::new(1.0_f32, if down { theme::blue() } else { theme::card_border() });
        let live = screen.is_some_and(|c| c.showing());
        match (&self.texture, live) {
            (Some(tex), true) => {
                cv.rounded_rect(tr, 6.0 * s, Color32::BLACK, Stroke::NONE);
                cv.image(fit_rect(tr, tex.size()), tex.id());
                cv.rounded_rect(tr, 6.0 * s, Color32::TRANSPARENT, border);
            }
            _ => {
                cv.rounded_rect(tr, 6.0 * s, theme::card(), border);
                cv.text(
                    Pos2::new(tr.center().x, tr.top() + 5.0 * s),
                    Align2::CENTER_TOP,
                    tr!("gp.touch"),
                    FontId::proportional(11.0 * s),
                    theme::text_dim(),
                );
                if let Some(c) = screen {
                    let font = FontId::proportional(12.0 * s);
                    let text = cv.fit_text(&c.placeholder(), font.clone(), tr.width() - 12.0 * s);
                    cv.text(tr.center(), Align2::CENTER_CENTER, &text, font, theme::text_dim());
                }
            }
        }
        if down {
            let p = Pos2::new(
                tr.left() + tx as f32 / 65535.0 * tr.width(),
                tr.top() + ty as f32 / 65535.0 * tr.height(),
            );
            cv.circle_filled(p, 6.0 * s, theme::blue());
        }
        self.hits.push((Shape::Rect(tr), Target::Touch));
        self.touch_rect = tr;
        self.zone_rect = tr;
    }

    /// «Pantalla del GamePad a pantalla completa»: solo la pantalla de Cemu,
    /// ajustada a su proporción real sobre fondo negro, con el táctil sobre
    /// la imagen (las bandas negras no cuentan), una ✕ pequeña arriba a la
    /// izquierda para salir y, si el ajuste lo pide, el botón de teclado
    /// arriba a la derecha. Sin sticks ni botones: el mando real va en el PC.
    /// Los chips van ANTES que la zona táctil en `hits` (primer acierto).
    fn layout_full(&mut self, cv: &Canvas, buttons: &Buttons, v: &View, keyboard: bool) {
        let r = cv.rect();
        let (vw, vh) = (r.width(), r.height());
        let s = (vh / 370.0).min(vw / 700.0).clamp(0.5, 1.6);
        cv.rounded_rect(r, 0.0, Color32::BLACK, Stroke::NONE);
        self.touch_rect = Rect::NOTHING;
        self.zone_rect = r;

        // chips: ✕ arriba a la izquierda, Teclado arriba a la derecha
        let chip_h = 26.0 * s;
        let chip_font = 13.0 * s;
        let close = Rect::from_min_size(Pos2::new(r.left() + 8.0 * s, r.top() + 8.0 * s), Vec2::new(34.0 * s, chip_h));
        self.chip(cv, close, "✕", chip_font, false, theme::error(), Chip::Exit);
        if keyboard && v.mode == "cemu" {
            let kb = Rect::from_min_size(Pos2::new(r.right() - 8.0 * s - 64.0 * s, r.top() + 8.0 * s), Vec2::new(64.0 * s, chip_h));
            self.chip(cv, kb, tr!("common.keyboard"), chip_font, false, theme::text(), Chip::Keyboard);
        }

        // la imagen, ajustada a su proporción real (16:9 mientras no hay fotograma)
        let live = v.screen.is_some_and(|c| c.showing());
        let fitted = match (&self.texture, live) {
            (Some(tex), true) => {
                let f = fit_rect(r, tex.size());
                cv.image(f, tex.id());
                f
            }
            _ => {
                let f = fit_rect(r, [16, 9]);
                cv.text(
                    Pos2::new(f.center().x, f.center().y - 10.0 * s),
                    Align2::CENTER_CENTER,
                    tr!("gp.touch"),
                    FontId::proportional(13.0 * s),
                    theme::text_dim(),
                );
                if let Some(c) = v.screen {
                    let font = FontId::proportional(12.0 * s);
                    let text = cv.fit_text(&c.placeholder(), font.clone(), f.width() - 12.0 * s);
                    cv.text(Pos2::new(f.center().x, f.center().y + 10.0 * s), Align2::CENTER_CENTER, &text, font, theme::text_dim());
                }
                f
            }
        };
        let (tx, ty, down) = buttons.touch();
        if down {
            let p = Pos2::new(
                fitted.left() + tx as f32 / 65535.0 * fitted.width(),
                fitted.top() + ty as f32 / 65535.0 * fitted.height(),
            );
            cv.circle_filled(p, 6.0 * s, theme::blue());
        }
        self.hits.push((Shape::Rect(fitted), Target::Touch));
        self.touch_rect = fitted;

        // Aviso transitorio del receptor (o local), arriba
        if let Some(n) = v.notice {
            let w = (0.6 * vw).min(360.0 * s);
            let nr = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 22.0 * s), Vec2::new(w, 28.0 * s));
            cv.rounded_rect(nr, 14.0 * s, theme::card(), Stroke::new(1.5_f32, theme::warn()));
            let font = FontId::proportional(12.0 * s);
            cv.text(nr.center(), Align2::CENTER_CENTER, &cv.fit_text(n, font.clone(), w - 16.0 * s), font, theme::text());
        }
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
        let target = match touch::hit_test(&self.hits, pos) {
            Some(t) => t,
            // deslizando, un dedo que nace en el vacío pulsa al entrar
            None if self.press.slide => Target::Free,
            None => return,
        };
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
            Target::Dpad => {
                // un solo dedo lleva la cruceta; el segundo se ignora
                if self.touches.values().any(|t| *t == Target::Dpad) {
                    return;
                }
                self.dpad_at(pos, buttons);
            }
            Target::Chip(Chip::Hold(n)) => {
                if !self.held.contains(&n) {
                    self.held.push(n);
                }
            }
            Target::Chip(_) | Target::Free => {}
        }
        self.touches.insert(key, target);
    }

    fn moved(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        match self.touches.get(&key).copied() {
            Some(Target::Stick(side)) => self.drag(side, pos, buttons),
            Some(Target::Touch) => self.touch_at(pos, buttons),
            // deslizar por la cruceta cambia de dirección sin levantar el dedo
            Some(Target::Dpad) => self.dpad_at(pos, buttons),
            // un botón (o un dedo libre): lo que diga el modo de pulsación.
            // Sticks, táctil, cruceta y chips quedan fuera: deslizar no los
            // engancha ni los suelta.
            Some(target @ (Target::Button(_) | Target::Free)) => self.slide(key, target, pos, buttons),
            _ => {}
        }
    }

    /// ¿Lleva OTRO dedo ese mismo bit? Ni se le quita deslizando ni se suelta
    /// al levantar este: mientras alguien lo apriete, sigue apretado (igual que
    /// `SlideTracker` en Android).
    fn taken(&self, key: u64, bit: u32) -> bool {
        self.touches.iter().any(|(k, t)| *k != key && *t == Target::Button(bit))
    }

    /// Dedo que lleva un botón (o nada) y se mueve: suelta y pulsa lo que
    /// diga `touch::on_move`.
    fn slide(&mut self, key: u64, target: Target, pos: Pos2, buttons: &Buttons) {
        let held = if let Target::Button(bit) = target { Some(bit) } else { None };
        let over = if self.press.slide {
            // deslizando manda lo que haya bajo el dedo (solo botones), salvo
            // el que ya lleve otro dedo: ese no se le quita
            match touch::hit_test(&self.hits, pos) {
                Some(Target::Button(bit)) if !self.taken(key, bit) => Some(bit),
                _ => None,
            }
        } else {
            // si no, solo importa si el dedo sigue dentro de su forma
            let shape = self.hits.iter().find(|(_, t)| *t == target).map(|(s, _)| *s);
            held.filter(|_| shape.is_some_and(|s| s.hit(pos)))
        };
        if let touch::Move::To(next) = touch::on_move(self.press, held, over) {
            if let Some(bit) = held {
                buttons.set(bit, false);
            }
            if let Some(bit) = next {
                buttons.set(bit, true);
            }
            self.touches.insert(key, next.map_or(Target::Free, Target::Button));
        }
    }

    fn end(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        match self.touches.remove(&key) {
            // salvo que otro dedo siga apretando ese mismo bit
            Some(Target::Button(bit)) => {
                if !self.taken(key, bit) {
                    buttons.set(bit, false);
                }
            }
            Some(Target::Dpad) => {
                let mut held = self.dpad_held;
                dpad::apply(buttons, &mut held, 0);
                self.dpad_held = 0;
            }
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
            // una tecla de mantener se suelta al levantar el dedo, esté donde esté
            Some(Target::Chip(Chip::Hold(n))) => {
                self.held.retain(|h| *h != n);
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

    /// Dedo en `pos` (virtual) sobre la cruceta → sus bits (suelta y pulsa lo que cambie).
    fn dpad_at(&mut self, pos: Pos2, buttons: &Buttons) {
        let (dc, half) = self.dpad;
        let mut held = self.dpad_held;
        dpad::apply(buttons, &mut held, dpad::bits(pos - dc, half));
        self.dpad_held = held;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switch_status(pad: &str) -> Status {
        Status::Connected {
            pc_name: "PC".into(), mode: "switch".into(), mode_by_pc: false,
            slot: 0, player: 1, role: crate::link::Role::Wiimote, rtt_ms: None,
            supports_cemu: true, supports_switch: true, supports_retroarch: true, pad: pad.into(),
            notice: None, mode_seq: 1, pad_seq: 1, own_nunchuk: false, screen_only: None,
        }
    }

    fn retro_status(pad: &str) -> Status {
        match switch_status(pad) {
            Status::Connected { pc_name, mode_by_pc, slot, player, role, rtt_ms, supports_cemu, supports_switch, supports_retroarch, notice, mode_seq, pad_seq, own_nunchuk, screen_only, .. } => Status::Connected {
                pc_name, mode: "retroarch".into(), mode_by_pc, slot, player, role, rtt_ms, supports_cemu, supports_switch, supports_retroarch,
                pad: pad.into(), notice, mode_seq, pad_seq, own_nunchuk, screen_only,
            },
            other => other,
        }
    }

    fn render(gamepad: &mut GamePadUi, buttons: &Buttons, status: &Status, size: Vec2) {
        render_press(gamepad, buttons, status, size, Press::default());
    }

    fn render_mode(gamepad: &mut GamePadUi, buttons: &Buttons, status: &Status, size: Vec2, wanted_mode: &'static str) {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), ..Default::default() }, |ctx| {
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
                gamepad.show(ui, buttons, &Inputs {
                    status, rotation: Rotation::Left, show_chips: true, optimistic: false,
                    wanted_mode, pad_pending: None, sensor_hz: 200.0, screen: None,
                    no_screen: false, full_screen: true, keyboard_button: true, press: Press::default(),
                });
            });
        });
    }

    #[test]
    fn retroarch_retropad_has_every_button_its_three_way_selector_and_hotkeys() {
        let required = pmp::BTN_A | pmp::BTN_B | pmp::BTN_X | pmp::BTN_Y | pmp::BTN_L | pmp::BTN_R |
            pmp::BTN_ZL | pmp::BTN_ZR | pmp::BTN_STICK_L | pmp::BTN_STICK_R | pmp::BTN_MINUS | pmp::BTN_PLUS | pmp::BTN_HOME | pmp::BTN_SCREEN;
        for size in [Vec2::new(700.0, 370.0), Vec2::new(360.0, 640.0)] {
            let mut ui = GamePadUi::new();
            render_mode(&mut ui, &Buttons::new(), &retro_status("retropad"), size, "retroarch");
            assert!(ui.active, "RetroPad confirmado: se juega");
            assert_eq!(mask(&ui), required);
            assert!(ui.touch_size_px(1.0).is_none(), "sin pantalla táctil");
            for pad in ["retropad", "nes", "gun"] {
                assert!(ui.hits.iter().any(|(_, t)| *t == Target::Chip(Chip::Pad(pad))), "selector {pad}");
            }
            assert!(ui.hits.iter().any(|(_, t)| *t == Target::Chip(Chip::Hotkey("save_state"))));
            assert!(ui.hits.iter().any(|(_, t)| *t == Target::Chip(Chip::Hold("rewind"))));
            assert!(ui.hits.iter().any(|(_, t)| *t == Target::Chip(Chip::Mode("retroarch"))));
        }
        // rebobinar: pulsado mientras el dedo está encima, suelto al levantarlo
        let mut ui = GamePadUi::new();
        let b = Buttons::new();
        render_mode(&mut ui, &b, &retro_status("retropad"), Vec2::new(700.0, 370.0), "retroarch");
        let rewind = position(&ui, Target::Chip(Chip::Hold("rewind")));
        ui.begin(9, rewind, &b);
        assert_eq!(ui.held_hotkeys(), vec!["rewind"]);
        ui.end(9, rewind + Vec2::new(300.0, 0.0), &b);
        assert!(ui.held_hotkeys().is_empty());
        // guardar estado dispara al levantar el dedo encima
        let save = position(&ui, Target::Chip(Chip::Hotkey("save_state")));
        ui.begin(10, save, &b);
        ui.end(10, save, &b);
        assert_eq!(ui.fired, Some(Chip::Hotkey("save_state")));
        // el mando de NES no es esta pantalla (ni 80 bytes): inerte aquí
        let mut ui = GamePadUi::new();
        render_mode(&mut ui, &Buttons::new(), &retro_status("nes"), Vec2::new(700.0, 370.0), "retroarch");
        assert!(!ui.active);
    }

    fn render_press(gamepad: &mut GamePadUi, buttons: &Buttons, status: &Status, size: Vec2, press: Press) {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), ..Default::default() }, |ctx| {
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
                gamepad.show(ui, buttons, &Inputs {
                    status, rotation: Rotation::Left, show_chips: true, optimistic: false,
                    wanted_mode: "switch", pad_pending: None, sensor_hz: 200.0, screen: None,
                    no_screen: false, full_screen: true, keyboard_button: true, press,
                });
            });
        });
    }

    fn mask(gamepad: &GamePadUi) -> u32 {
        gamepad.hits.iter().fold(0, |bits, (_, target)| bits | if let Target::Button(bit) = target { *bit } else { 0 })
    }

    fn position(gamepad: &GamePadUi, target: Target) -> Pos2 {
        match gamepad.hits.iter().find(|(_, t)| *t == target).unwrap().0 {
            Shape::Circle { c, .. } => c,
            Shape::Rect(r) => r.center(),
        }
    }

    #[test]
    fn switch_always_has_complete_pro_controls_without_a_selector_or_screen() {
        let required = pmp::BTN_A | pmp::BTN_B | pmp::BTN_X | pmp::BTN_Y | pmp::BTN_L | pmp::BTN_R |
            pmp::BTN_ZL | pmp::BTN_ZR | pmp::BTN_STICK_L | pmp::BTN_STICK_R | pmp::BTN_MINUS | pmp::BTN_PLUS | pmp::BTN_HOME | pmp::BTN_SCREEN;
        for pad in ["pro", "joycons", "joycon_side", "joycon_r"] {
            for size in [Vec2::new(700.0, 370.0), Vec2::new(360.0, 640.0)] {
                let mut ui = GamePadUi::new();
                render(&mut ui, &Buttons::new(), &switch_status(pad), size);
                assert_eq!(mask(&ui), required, "{pad}");
                assert!(ui.touch_size_px(1.0).is_none());
                assert!(!ui.hits.iter().any(|(_, t)| matches!(t, Target::Touch | Target::Chip(Chip::Pad(_)))));
                assert!(ui.hits.iter().any(|(_, t)| *t == Target::Stick(Side::Left)));
                assert!(ui.hits.iter().any(|(_, t)| *t == Target::Stick(Side::Right)));
                assert!(ui.hits.iter().any(|(_, t)| *t == Target::Dpad));
                assert_eq!(ui.transform, Transform::for_screen(ui.screen, Rotation::Left));
                assert!(position(&ui, Target::Button(pmp::BTN_A)).x > position(&ui, Target::Button(pmp::BTN_Y)).x);
                assert!(position(&ui, Target::Button(pmp::BTN_X)).y < position(&ui, Target::Button(pmp::BTN_B)).y);
            }
        }
    }

    #[test]
    fn layout_changes_release_held_controls_and_pending_mode_is_inert() {
        let mut ui = GamePadUi::new();
        let b = Buttons::new();
        render(&mut ui, &b, &switch_status("pro"), Vec2::new(700.0, 370.0));
        ui.begin(1, position(&ui, Target::Button(pmp::BTN_A)), &b);
        let stick = position(&ui, Target::Stick(Side::Left));
        ui.begin(2, stick, &b);
        ui.moved(2, stick + Vec2::new(500.0, 0.0), &b);
        assert_ne!(b.physical(), 0);
        assert_ne!(b.stick(), (0, 0));
        render(&mut ui, &b, &switch_status("joycon_r"), Vec2::new(360.0, 640.0));
        assert_eq!(b.physical(), 0);
        assert_eq!(b.wire_at(std::time::Instant::now()), 0);
        assert_eq!(b.stick(), (0, 0));
        assert!(ui.touches.is_empty());
        let mut pending = switch_status("pro");
        if let Status::Connected { mode, .. } = &mut pending { *mode = "pointer".into(); }
        render(&mut ui, &b, &pending, Vec2::new(700.0, 370.0));
        ui.begin(3, position(&ui, Target::Button(pmp::BTN_A)), &b);
        assert_eq!(b.physical(), 0);
    }

    #[test]
    fn medidas_del_trazado_en_fila() {
        // Móvil apaisado de referencia (700×370, s = 1): cuerpo de 306 bajo cabecera y selector
        let m = row_metrics(700.0, 306.0, 1.0, false);
        assert!((m.pad - 145.0).abs() < 0.01, "lo limita la anchura: (700 − 76 − 44) / 4 = {}", m.pad);
        assert!(m.pad > 92.0, "más grande que el stick apilado (92)");
        assert!((m.face_r - 145.0 / 5.2).abs() < 0.01);
        assert_eq!(m.center_w, 76.0);
        let pro = row_metrics(700.0, 306.0, 1.0, true);
        assert!((pro.pad - 154.0).abs() < 0.01, "sin pastillas el centro es más estrecho: {}", pro.pad);
        assert_eq!(pro.center_w, 40.0);
        // Muy bajo: lo limita la altura (306 → 242 de sitio)
        let bajo = row_metrics(1400.0, 306.0, 1.0, false);
        assert!((bajo.pad - 242.0).abs() < 0.01, "{}", bajo.pad);
        // Escala 1,5: todo ×1,5
        let big = row_metrics(1050.0, 459.0, 1.5, false);
        assert!((big.pad - 145.0 * 1.5).abs() < 0.01, "{}", big.pad);
        assert!(row_metrics(100.0, 100.0, 1.0, false).pad >= 40.0, "nunca por debajo del mínimo");
    }

    #[test]
    fn pantalla_completa_ajusta_la_imagen_y_el_tactil_va_sobre_ella() {
        // móvil apaisado 852×393 con un fotograma 854×480: bandas a los lados
        let r = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(852.0, 393.0));
        let f = fit_rect(r, [854, 480]);
        assert!((f.width() - 699.2).abs() < 0.5 && (f.height() - 393.0).abs() < 0.5, "{f:?}");
        assert!((f.left() - 76.4).abs() < 0.5 && f.top().abs() < 0.5, "centrada: {f:?}");
        assert_eq!(touch_fraction(f, f.center()), (0x8000, 0x8000));
        assert!(!Shape::Rect(f).hit(Pos2::new(20.0, 100.0)), "las bandas negras no son la pantalla");
        assert!(Shape::Rect(f).hit(Pos2::new(426.0, 196.0)));
        // sin fotograma: 16:9
        let g = fit_rect(r, [16, 9]);
        assert!((g.width() - 698.7).abs() < 0.5, "{g:?}");
    }

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

    /// Un punto del espacio virtual donde no hay nada.
    fn empty(gp: &GamePadUi) -> Pos2 {
        let v = gp.transform.virtual_rect(gp.screen);
        let mut y = v.top() + 2.0;
        while y < v.bottom() {
            let mut x = v.left() + 2.0;
            while x < v.right() {
                let p = Pos2::new(x, y);
                if touch::hit_test(&gp.hits, p).is_none() {
                    return p;
                }
                x += 6.0;
            }
            y += 6.0;
        }
        panic!("no hay ni un hueco vacío en el trazado");
    }

    fn setup(press: Press) -> (GamePadUi, Buttons) {
        let mut gp = GamePadUi::new();
        let b = Buttons::new();
        render_press(&mut gp, &b, &switch_status("pro"), Vec2::new(700.0, 370.0), press);
        (gp, b)
    }

    #[test]
    fn deslizar_cambia_de_boton_sin_levantar_el_dedo() {
        let (mut gp, b) = setup(Press { slide: true, sticky: true });
        let (a, x) = (position(&gp, Target::Button(pmp::BTN_A)), position(&gp, Target::Button(pmp::BTN_X)));
        gp.begin(1, a, &b);
        assert_eq!(b.physical(), pmp::BTN_A);
        gp.moved(1, empty(&gp), &b);
        assert_eq!(b.physical(), 0, "al salir al vacío se suelta");
        gp.moved(1, x, &b);
        assert_eq!(b.physical(), pmp::BTN_X, "y al entrar en la X se pulsa la X");
        gp.end(1, x, &b);
        assert_eq!(b.physical(), 0);
        assert!(gp.touches.is_empty());
    }

    #[test]
    fn un_boton_que_lleva_otro_dedo_no_se_le_quita() {
        let (mut gp, b) = setup(Press { slide: true, sticky: true });
        let (a, x) = (position(&gp, Target::Button(pmp::BTN_A)), position(&gp, Target::Button(pmp::BTN_X)));
        gp.begin(1, a, &b);
        gp.begin(2, x, &b);
        assert_eq!(b.physical(), pmp::BTN_A | pmp::BTN_X);
        // el segundo dedo se arrastra a la A: ya la lleva el primero, así que
        // suelta la X y se queda sin nada
        gp.moved(2, a, &b);
        assert_eq!(b.physical(), pmp::BTN_A, "la A sigue siendo del primer dedo");
        assert_eq!(gp.touches[&2], Target::Free);
        gp.end(2, a, &b);
        assert_eq!(b.physical(), pmp::BTN_A, "al levantar el segundo, la A sigue pulsada");
        gp.end(1, a, &b);
        assert_eq!(b.physical(), 0);
        // y dos dedos que caen a la vez en el mismo botón: lo suelta el último
        let (mut gp, b) = setup(Press::default());
        let a = position(&gp, Target::Button(pmp::BTN_A));
        gp.begin(3, a, &b);
        gp.begin(4, a, &b);
        gp.end(3, a, &b);
        assert_eq!(b.physical(), pmp::BTN_A, "el otro dedo la sigue apretando");
        gp.end(4, a, &b);
        assert_eq!(b.physical(), 0);
    }

    #[test]
    fn un_dedo_que_nace_en_el_vacio_pulsa_al_entrar() {
        let (mut gp, b) = setup(Press { slide: true, sticky: true });
        let hueco = empty(&gp);
        gp.begin(1, hueco, &b);
        assert_eq!(b.physical(), 0);
        assert_eq!(gp.touches[&1], Target::Free);
        gp.moved(1, position(&gp, Target::Button(pmp::BTN_A)), &b);
        assert_eq!(b.physical(), pmp::BTN_A);
        gp.end(1, position(&gp, Target::Button(pmp::BTN_A)), &b);
        assert_eq!(b.physical(), 0);
        // sin deslizar, ese dedo ni se registra
        let (mut gp, b) = setup(Press::default());
        let hueco = empty(&gp);
        gp.begin(2, hueco, &b);
        assert!(gp.touches.is_empty());
        gp.moved(2, position(&gp, Target::Button(pmp::BTN_A)), &b);
        assert_eq!(b.physical(), 0);
    }

    #[test]
    fn sin_pegajoso_se_suelta_al_salir_y_no_vuelve() {
        let (mut gp, b) = setup(Press { slide: false, sticky: false });
        let a = position(&gp, Target::Button(pmp::BTN_A));
        gp.begin(1, a, &b);
        assert_eq!(b.physical(), pmp::BTN_A);
        gp.moved(1, empty(&gp), &b);
        assert_eq!(b.physical(), 0, "al salirse se suelta");
        gp.moved(1, a, &b);
        assert_eq!(b.physical(), 0, "y el dedo ya no vuelve a coger nada");
        gp.end(1, a, &b);
        assert_eq!(b.physical(), 0);
        // de serie sigue siendo pegajoso
        let (mut gp, b) = setup(Press::default());
        gp.begin(2, a, &b);
        gp.moved(2, empty(&gp), &b);
        assert_eq!(b.physical(), pmp::BTN_A, "pegajoso: sigue pulsado fuera del botón");
        gp.end(2, a, &b);
        assert_eq!(b.physical(), 0);
    }

    #[test]
    fn deslizar_no_engancha_sticks_cruceta_ni_chips() {
        let (mut gp, b) = setup(Press { slide: true, sticky: true });
        let chip = *gp.hits.iter().find_map(|(_, t)| if let Target::Chip(c) = t { Some(c) } else { None }).expect("algún chip");
        for target in [Target::Stick(Side::Left), Target::Stick(Side::Right), Target::Dpad, Target::Chip(chip)] {
            gp.begin(1, position(&gp, Target::Button(pmp::BTN_A)), &b);
            assert_eq!(b.physical(), pmp::BTN_A);
            let p = position(&gp, target);
            gp.moved(1, p, &b);
            assert_eq!(b.physical(), 0, "{target:?}: se suelta la A y no se coge nada");
            assert_eq!(b.stick(), (0, 0));
            assert_eq!(b.stick2(), (0, 0));
            gp.end(1, p, &b);
            assert!(gp.fired.take().is_none(), "{target:?}: un chip no dispara si el dedo no nació en él");
        }
    }

    #[test]
    fn con_wii_u_sin_eco_no_se_pulsa_nada_ni_deslizando() {
        let mut pending = switch_status("pro");
        if let Status::Connected { mode, .. } = &mut pending {
            *mode = "pointer".into();
        }
        let mut gp = GamePadUi::new();
        let b = Buttons::new();
        let press = Press { slide: true, sticky: true };
        render_press(&mut gp, &b, &pending, Vec2::new(700.0, 370.0), press);
        assert!(!gp.active);
        let a = position(&gp, Target::Button(pmp::BTN_A));
        gp.begin(1, a, &b);
        assert_eq!(b.physical(), 0);
        assert!(gp.touches.is_empty());
        // y un dedo que nace en el vacío tampoco se registra
        gp.begin(2, empty(&gp), &b);
        assert!(gp.touches.is_empty());
        gp.moved(2, a, &b);
        assert_eq!(b.physical(), 0);
    }
}
