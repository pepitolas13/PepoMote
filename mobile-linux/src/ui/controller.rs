//! El mando (vertical, estilo Wiimote): cruceta, −/diana/+, A, 1/2,
//! multimedia, gatillo B y tira de scroll. Multitouch REAL: cada dedo se
//! sigue por su id de toque (ui/touch.rs) con hit-test propio, así se puede
//! mantener B mientras se pulsa A. Con ratón (pruebas en PC) se usa el
//! puntero; en cuanto aparece el primer toque, el puntero sintetizado del
//! primer dedo se ignora para no contar doble.

use crate::buttons::Buttons;
use crate::link::Status;
use crate::theme;
use crate::ui::dpad;
use crate::ui::touch::{self, Canvas, Input, Phase, Press, Shape, Transform};
use egui::{Align2, FontId, Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::tr;

pub enum Action {
    None,
    Exit,
    Mode(&'static str),
    /// Pedir al receptor otro tipo de mando en modo Wii U (`"gamepad"` o
    /// `"wiimote"`, desde el selector «En Cemu soy») o en RetroArch
    /// (`"retropad"`, `"nes"`, `"gun"`).
    Pad(&'static str),
    /// RetroArch: abrir el selector de mando de consola.
    LayoutPicker,
    /// Abrir el teclado para el teclado en pantalla de Cemu (modo Wii U).
    Keyboard,
    /// RetroArch: tecla rápida de un toque (nombre del protocolo).
    Hotkey(&'static str),
    /// RetroArch: tecla de mantener, pulsada (true) o soltada.
    Hold(&'static str, bool),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Button(u32),
    /// Un dedo que no lleva nada: con «pulsar deslizando» pulsará el botón en
    /// el que entre. Solo se registra con ese ajuste encendido.
    Free,
    /// La tira de precisión / acercar: es «mantener», no un botón. No se
    /// suelta al salirse el dedo ni se pulsa deslizando; solo al levantar.
    Hold(u32),
    /// La cruceta de una pieza: el dedo lleva sus bits mientras está apoyado.
    Dpad,
    Recenter,
    Scroll,
}

struct Touch {
    target: Target,
    start: Instant,
    last: Pos2,
    recentered: bool,
    /// Bits de cruceta que lleva este dedo.
    held: u32,
}

const RECENTER_HOLD: Duration = Duration::from_millis(150);

pub struct ControllerUi {
    hits: Vec<(Shape, Target)>,
    touches: HashMap<u64, Touch>,
    input: Input,
    show_media: bool,
    /// Centro y media anchura de la cruceta del último frame.
    dpad: (Pos2, f32),
    /// Modo Dolphin: la tira izquierda es «Acercar» (bit 30) en vez de la
    /// precisión, que ahí no hace nada.
    dolphin: bool,
    /// Los dos ajustes de pulsación (deslizar / mantener al salir).
    press: Press,
    /// RetroArch confirmado: etiquetas B/A/X/Menú en vez de 1/2/A/Home.
    retro: bool,
    /// RetroArch: rebobinar estaba pulsado en el frame anterior.
    rewind_held: bool,
}

impl Default for ControllerUi {
    fn default() -> Self {
        Self::new()
    }
}

/// Nombre del modo del receptor para la cabecera.
pub fn mode_label(mode: &str) -> String {
    match mode {
        "pointer" => tr!("common.mode_pointer").to_owned(),
        "dolphin" => tr!("common.mode_dolphin").to_owned(),
        "cemu" => tr!("common.mode_cemu").to_owned(),
        "switch" => tr!("common.mode_switch").to_owned(),
        "retroarch" => tr!("common.mode_retroarch").to_owned(),
        other => other.to_owned(),
    }
}

/// Teclas rápidas de RetroArch que caben en el mando: (nombre del protocolo,
/// de mantener). El avance rápido y el menú van en el propio mando apaisado
/// (Rápido y Menú); en el vertical, Home es el menú.
pub const RETRO_HOTKEYS: [(&str, bool); 8] = [
    ("save_state", false),
    ("load_state", false),
    ("slot_minus", false),
    ("slot_plus", false),
    ("rewind", true),
    ("pause", false),
    ("screenshot", false),
    ("reset", false),
];

/// Etiqueta de cada tecla rápida (claves literales: el test de i18n las ve).
pub fn hotkey_label(name: &str) -> &'static str {
    match name {
        "save_state" => tr!("hk.save"),
        "load_state" => tr!("hk.load"),
        "slot_minus" => tr!("hk.slot_minus"),
        "slot_plus" => tr!("hk.slot_plus"),
        "rewind" => tr!("hk.rewind"),
        "pause" => tr!("hk.pause"),
        "screenshot" => tr!("hk.screenshot"),
        _ => tr!("hk.reset"),
    }
}

/// Selector segmentado «En RetroArch soy: [<consola>] [Wii de lado] [Pistola]».
/// El primer segmento lleva el nombre de la plantilla que toca (`layout_name`);
/// tocarlo estando ya elegido abre el selector de consola.
pub fn retro_pad_selector(ui: &mut egui::Ui, pad: &str, pending: Option<&str>, layout_name: &str, receiver: &crate::link::ReceiverCapabilities) -> Option<Action> {
    let mut out = None;
    ui.label(RichText::new(tr!("common.in_retroarch")).size(13.0).color(theme::text_dim()));
    ui.horizontal(|ui| {
        let pads = receiver.retro_pads();
        let w = ((ui.available_width() - 8.0 * (pads.len() - 1) as f32) / pads.len() as f32).max(70.0);
        for &p in pads {
            let label = match p { "nes" => tr!("common.nes_pad"), "gun" => tr!("common.light_gun"), _ => layout_name };
            let on = pad == p && pending.is_none();
            let pend = pending == Some(p);
            let (fill, color) = if on {
                (theme::blue(), theme::ON_ACCENT)
            } else if pend {
                (theme::glow(), theme::text())
            } else {
                (theme::card(), theme::text_dim())
            };
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
            ui.painter().rect(rect, egui::Rounding::same(15.0), fill, Stroke::new(1.0_f32, theme::card_border()));
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(13.0), color);
            if resp.clicked() {
                out = if pad != p {
                    Some(Action::Pad(p))
                } else if on && p == "retropad" {
                    Some(Action::LayoutPicker)
                } else {
                    None
                };
            }
        }
    });
    out
}

/// Teclas rápidas de RetroArch como botones: las de un toque disparan al
/// hacer clic; la de mantener (rebobinar) da un flanco al pulsar y otro al
/// soltar (`held` guarda si estaba pulsada en el frame anterior).
pub fn retro_hotkeys(ui: &mut egui::Ui, held: &mut bool) -> Action {
    let mut action = Action::None;
    ui.horizontal_wrapped(|ui| {
        for (name, hold) in RETRO_HOTKEYS {
            let resp = ui.button(RichText::new(hotkey_label(name)).size(13.0).color(theme::text()));
            if hold {
                let down = resp.is_pointer_button_down_on();
                if down != *held {
                    *held = down;
                    action = Action::Hold(name, down);
                }
            } else if resp.clicked() {
                action = Action::Hotkey(name);
            }
        }
    });
    action
}

/// Selector segmentado «En Cemu soy: [GamePad|Pro Controller] [Mando de
/// Wii]» (modo Wii U). El activo va en azul; el pedido y sin eco, a medio
/// tono (`pending`). Devuelve el `pad` a pedir al tocar el otro segmento.
pub fn pad_selector(ui: &mut egui::Ui, player: u8, pad: &str, pending: Option<&str>) -> Option<&'static str> {
    let mut out = None;
    let wii = pad == "wiimote";
    ui.label(RichText::new(tr!("common.in_cemu")).size(13.0).color(theme::text_dim()));
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
        let first = if player == 1 { tr!("common.gamepad") } else { tr!("common.pro") };
        if seg(ui, first, !wii && pending.is_none(), pending == Some("gamepad")) && wii {
            out = Some("gamepad");
        }
        if seg(ui, tr!("common.wiimote"), wii && pending.is_none(), pending == Some("wiimote")) && !wii {
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
            dpad: (Pos2::ZERO, 1.0),
            dolphin: false,
            retro: false,
            rewind_held: false,
            press: Press::default(),
        }
    }

    /// Suelta todos los dedos (al tapar la pantalla con el teclado: los
    /// toques que sigan no llegarán aquí).
    pub fn release(&mut self, buttons: &Buttons) {
        self.touches.clear();
        buttons.release_all();
    }

    /// `pad_pending`: tipo de mando pedido al receptor y aún sin eco (el
    /// selector lo pinta a medio tono). `press`: los ajustes de pulsación.
    /// `media_everywhere`: el ajuste «Multimedia en todos los modos».
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        buttons: &Buttons,
        status: &Status,
        show_chips: bool,
        pad_pending: Option<&str>,
        sensor_hz: f32,
        press: Press,
        layout_name: &str,
        media_everywhere: bool,
    ) -> Action {
        let mut action = Action::None;
        self.press = press;

        // Cabecera
        ui.horizontal(|ui| {
            ui.vertical(|ui| match status {
                Status::Connected { pc_name, mode, slot, player, rtt_ms, .. } => {
                    let title = if *slot > 0 {
                        tr!("ctl.title_player", pc_name, player)
                    } else {
                        pc_name.clone()
                    };
                    ui.label(RichText::new(title).size(17.0).strong().color(theme::text()));
                    let mut line = match mode.as_str() {
                        "cemu" => tr!("ctl.wiiu_wiimote").to_owned(),
                        "pointer" if *slot > 0 => tr!("ctl.pointer_p1").to_owned(),
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
                // Mando de Wii dentro de Wii U: el teclado para el teclado en
                // pantalla de Cemu, como en el GamePad; en modo puntero, para
                // el campo del PC donde apuntas
                if status.has_keyboard()
                    && ui.button(RichText::new(tr!("common.keyboard")).size(14.0).color(theme::text())).clicked()
                {
                    action = Action::Keyboard;
                }
            });
        });

        if let Status::Connected { mode, supports_cemu, supports_switch, supports_retroarch, player, pad, receiver, .. } = status {
            let wiiu = mode == "cemu";
            // RetroArch: 1 y 2 son B y A del RetroPad, la A grande es X y Home el menú
            self.retro = mode == "retroarch" && *supports_retroarch;
            if show_chips {
                ui.horizontal_wrapped(|ui| {
                    // selección por igualdad exacta del modo
                    if receiver.supports_pointer() && ui.selectable_label(mode == "pointer", RichText::new(format!("  {}  ", tr!("common.mode_pointer"))).size(14.0)).clicked() {
                        action = Action::Mode("pointer");
                    }
                    if ui.selectable_label(mode == "dolphin", RichText::new(format!("  {}  ", tr!("common.mode_dolphin"))).size(14.0)).clicked() {
                        action = Action::Mode("dolphin");
                    }
                    if *supports_cemu && ui.selectable_label(wiiu, RichText::new(format!("  {}  ", tr!("common.mode_cemu"))).size(14.0)).clicked() {
                        action = Action::Mode("cemu");
                    }
                    if *supports_switch && ui.selectable_label(mode == "switch", RichText::new(format!("  {}  ", tr!("common.mode_switch"))).size(14.0)).clicked() {
                        action = Action::Mode("switch");
                    }
                    if *supports_retroarch && ui.selectable_label(mode == "retroarch", RichText::new(format!("  {}  ", tr!("common.mode_retroarch"))).size(14.0)).clicked() {
                        action = Action::Mode("retroarch");
                    }
                });
            }
            if wiiu {
                // Mando de Wii dentro de Wii U: el mismo selector que en el GamePad
                if let Some(p) = pad_selector(ui, *player, pad, pad_pending) {
                    action = Action::Pad(p);
                }
                ui.label(
                    RichText::new(tr!("ctl.pad_help"))
                        .size(11.0)
                        .color(theme::text_dim()),
                );
            } else if mode == "retroarch" && *supports_retroarch {
                // RetroArch: qué mando soy (RetroPad / NES / pistola) y las teclas rápidas
                if let Some(a) = retro_pad_selector(ui, pad, pad_pending, layout_name, receiver) {
                    action = a;
                }
                ui.label(RichText::new(tr!("ctl.retro_pad_help")).size(11.0).color(theme::text_dim()));
                if let hk @ (Action::Hotkey(_) | Action::Hold(..)) = retro_hotkeys(ui, &mut self.rewind_held) {
                    action = hk;
                }
            }
        }
        notice_banner(ui, status);

        // Cuerpo del mando: un lienzo con hit-test propio
        let avail = ui.available_size();
        let (rect, _) = ui.allocate_exact_size(avail, Sense::hover());
        self.hits.clear();
        // Home solo conectado y fuera del modo puntero (el PC no le da uso)
        let home = matches!(status, Status::Connected { mode, .. } if mode != "pointer");
        self.dolphin = matches!(status, Status::Connected { mode, .. } if mode == "dolphin");
        let connected_mode = match status {
            Status::Connected { mode, .. } => Some(mode.as_str()),
            _ => None,
        };
        let media = shows_media(connected_mode, media_everywhere);
        self.layout(ui, rect, buttons, home, media);
        self.process_events(ui.ctx(), buttons);
        action
    }

    /// `home`: pintar el botón Home entre 1 y 2 (Dolphin: menú HOME de la Wii;
    /// Cemu: Mario Party 10 lo pide para dar por emparejado cada Mando de Wii).
    fn layout(&mut self, ui: &mut egui::Ui, rect: Rect, buttons: &Buttons, home: bool, media: bool) {
        let painter = ui.painter();
        let cv = Canvas::new(painter, rect, Transform::Straight);
        let pressed = buttons.physical();
        // Escala para que quepa en pantallas bajas (referencia: 668 pt de alto)
        let s = (rect.height() / 668.0).clamp(0.7, 1.05);
        let cx = rect.center().x; // tiras simétricas: scroll a la derecha, precisión a la izquierda
        // Hueco de sobra entre los chips de modo y la cruceta: que ir a por ↑ no toque un chip
        let mut y = rect.top() + 14.0 * s;

        // Cruceta de una pieza (como la del Mando de Wii): el cuadrado entero es su hit-test
        let arm = 56.0 * s;
        let pad_c = Pos2::new(cx, y + arm * 1.5);
        let shape = dpad::draw(&cv, pad_c, arm * 1.5, s, pressed);
        self.dpad = (pad_c, arm * 1.5);
        self.hits.push((shape, Target::Dpad));
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

        // A (en RetroArch, X: la A grande del Mando Wii es el tercer botón del RetroPad)
        let a_r = 72.0 * s;
        let retro = self.retro;
        self.circle(&cv, Pos2::new(cx, y + a_r), a_r, if retro { "X" } else { "A" }, 44.0 * s, pmp::BTN_A, pressed, true);
        y += a_r * 2.0 + 12.0 * s;

        // 1 · Home · 2 (como en el mando + Nunchuk; sin Home, 1 y 2 juntos como siempre)
        let r12 = 26.0 * s;
        let dx = if home { 72.0 * s } else { 36.0 * s };
        self.circle(&cv, Pos2::new(cx - dx, y + r12), r12, if retro { "B" } else { "1" }, 18.0 * s, pmp::BTN_ONE, pressed, false);
        if home {
            let (label, font) = if retro { (tr!("gp.menu"), 10.0 * s) } else { ("Home", 12.0 * s) };
            self.circle(&cv, Pos2::new(cx, y + r12), r12, label, font, pmp::BTN_HOME, pressed, false);
        }
        self.circle(&cv, Pos2::new(cx + dx, y + r12), r12, if retro { "A" } else { "2" }, 18.0 * s, pmp::BTN_TWO, pressed, false);
        y += r12 * 2.0 + 8.0 * s;

        // Multimedia (plegable): solo en modo puntero, o en todos con el ajuste
        if media {
            let toggle = Rect::from_center_size(Pos2::new(cx, y + 14.0 * s), Vec2::new(150.0 * s, 26.0 * s));
            let toggle_resp = ui.interact(toggle, ui.id().with("media"), Sense::click());
            if toggle_resp.clicked() {
                self.show_media = !self.show_media;
            }
            painter.text(
                toggle.center(),
                Align2::CENTER_CENTER,
                if self.show_media { tr!("ctl.media_open") } else { tr!("ctl.media_closed") },
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

        // Tira de precisión: borde izquierdo, simétrica a la de scroll pero
        // algo más ancha. Mantener = el puntero va al 40 % (bit 29, solo en
        // modo puntero); sigue aunque el dedo se salga (el toque es pegajoso).
        // En Dolphin la precisión no hace nada: la tira es «Acercar» (bit 30,
        // el Mando de Wii emulado se acerca a la pantalla)
        let strip = Rect::from_min_max(
            Pos2::new(rect.left() + 2.0, rect.top() + rect.height() * 0.25),
            Pos2::new(rect.left() + 36.0 * s, rect.top() + rect.height() * 0.72),
        );
        let bit = if self.dolphin { pmp::BTN_NEAR } else { pmp::BTN_PRECISION };
        let held = pressed & bit != 0;
        painter.rect(
            strip,
            Rounding::same(13.0 * s),
            if held { theme::glow() } else { theme::card_border() },
            Stroke::NONE,
        );
        let c = strip.center();
        let dim = Stroke::new(1.8 * s, theme::text_dim());
        if self.dolphin {
            // pantalla (barra arriba) y flecha que se le acerca
            painter.line_segment([c + Vec2::new(-5.5 * s, -6.0 * s), c + Vec2::new(5.5 * s, -6.0 * s)], dim);
            painter.line_segment([c + Vec2::new(0.0, 6.5 * s), c + Vec2::new(0.0, -2.5 * s)], dim);
            painter.line_segment([c + Vec2::new(-3.5 * s, 1.0 * s), c + Vec2::new(0.0, -2.8 * s)], dim);
            painter.line_segment([c + Vec2::new(3.5 * s, 1.0 * s), c + Vec2::new(0.0, -2.8 * s)], dim);
        } else {
            // mirilla de francotirador: aro, cuatro marcas que lo cruzan y punto central
            painter.circle_stroke(c, 4.8 * s, dim);
            for d in [Vec2::new(0.0, -1.0), Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)] {
                painter.line_segment([c + d * (3.0 * s), c + d * (7.5 * s)], dim);
            }
            painter.circle_filled(c, 1.0 * s, theme::text_dim());
        }
        self.hits.push((Shape::Rect(strip), Target::Hold(bit)));
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

    /// Forma del objetivo en el trazado de este frame: para saber si el dedo
    /// sigue dentro sin volver a resolver el hit-test entero.
    fn shape_of(&self, target: Target) -> Option<Shape> {
        self.hits.iter().find(|(_, t)| *t == target).map(|(s, _)| *s)
    }

    /// ¿Lleva OTRO dedo ese mismo bit? Ni se le quita deslizando ni se suelta
    /// al levantar este: mientras alguien lo apriete, sigue apretado (igual que
    /// `SlideTracker` en Android).
    fn taken(&self, key: u64, bit: u32) -> bool {
        self.touches
            .iter()
            .any(|(k, t)| *k != key && matches!(t.target, Target::Button(b) | Target::Hold(b) if b == bit))
    }

    fn begin(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        let target = match touch::hit_test(&self.hits, pos) {
            Some(t) => t,
            // deslizando, un dedo que nace en el vacío pulsa al entrar
            None if self.press.slide => Target::Free,
            None => return,
        };
        let mut held = 0;
        match target {
            Target::Button(bit) | Target::Hold(bit) => buttons.set(bit, true),
            Target::Dpad => {
                // un solo dedo lleva la cruceta; el segundo se ignora
                if self.touches.values().any(|t| t.target == Target::Dpad) {
                    return;
                }
                let (dc, half) = self.dpad;
                dpad::apply(buttons, &mut held, dpad::bits(pos - dc, half));
            }
            _ => {}
        }
        self.touches.insert(
            key,
            Touch {
                target,
                start: Instant::now(),
                last: pos,
                recentered: false,
                held,
            },
        );
    }

    fn moved(&mut self, key: u64, pos: Pos2, buttons: &Buttons) {
        let (dc, half) = self.dpad;
        let Some(t) = self.touches.get(&key) else { return };
        let (target, last) = (t.target, t.last);
        match target {
            Target::Scroll => {
                // dedo hacia arriba (dy negativo) = scroll up = positivo
                buttons.add_scroll((last.y - pos.y).round() as i32);
            }
            // deslizar por la cruceta cambia de dirección sin levantar el dedo
            Target::Dpad => {
                if let Some(t) = self.touches.get_mut(&key) {
                    dpad::apply(buttons, &mut t.held, dpad::bits(pos - dc, half));
                }
            }
            // un botón (o un dedo libre): lo que diga el modo de pulsación.
            // La cruceta, la diana, el scroll y la tira de mantener quedan
            // fuera: deslizar no los engancha ni los suelta.
            Target::Button(_) | Target::Free => {
                let held = if let Target::Button(bit) = target { Some(bit) } else { None };
                let over = if self.press.slide {
                    // deslizando manda lo que haya bajo el dedo (solo botones),
                    // salvo el que ya lleve otro dedo: ese no se le quita
                    match touch::hit_test(&self.hits, pos) {
                        Some(Target::Button(bit)) if !self.taken(key, bit) => Some(bit),
                        _ => None,
                    }
                } else {
                    // si no, solo importa si el dedo sigue dentro de su forma
                    held.filter(|_| self.shape_of(target).is_some_and(|s| s.hit(pos)))
                };
                if let touch::Move::To(next) = touch::on_move(self.press, held, over) {
                    if let Some(bit) = held {
                        buttons.set(bit, false);
                    }
                    if let Some(bit) = next {
                        buttons.set(bit, true);
                    }
                    if let Some(t) = self.touches.get_mut(&key) {
                        t.target = next.map_or(Target::Free, Target::Button);
                    }
                }
            }
            _ => {}
        }
        if let Some(t) = self.touches.get_mut(&key) {
            t.last = pos;
        }
    }

    fn end(&mut self, key: u64, buttons: &Buttons) {
        if let Some(mut t) = self.touches.remove(&key) {
            match t.target {
                // la tira de mantener se suelta igual que un botón, salvo que
                // otro dedo siga apretando ese mismo bit
                Target::Button(bit) | Target::Hold(bit) => {
                    if !self.taken(key, bit) {
                        buttons.set(bit, false);
                    }
                }
                Target::Dpad => {
                    dpad::apply(buttons, &mut t.held, 0);
                }
                // un dedo libre no lleva nada que soltar
                _ => {}
            }
        }
    }
}

/// ¿Se enseña la fila multimedia (⏯, volumen…)? Siempre en modo puntero, el
/// único en el que el PC atiende esas teclas (en Dolphin, Wii U y Switch los
/// paquetes van al DSU y en RetroArch solo se aplican los bits de la pistola);
/// en los demás modos nunca, salvo con el ajuste «Multimedia en todos los
/// modos». Sin enlace confirmado (`None`: conectando, reconectando) se ve: el
/// trazado sin modo es el de puntero, como con el botón Home. La misma regla
/// que en Android e iOS.
pub fn shows_media(connected_mode: Option<&str>, everywhere: bool) -> bool {
    everywhere || connected_mode.is_none_or(|mode| mode == "pointer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::touch::Press;

    /// Mismos casos que RouteTest (Android) y RouteTests (iOS).
    #[test]
    fn la_fila_multimedia_solo_en_puntero_salvo_ajuste() {
        assert!(shows_media(Some("pointer"), false));
        for mode in ["dolphin", "cemu", "retroarch"] {
            assert!(!shows_media(Some(mode), false), "{mode}");
            assert!(shows_media(Some(mode), true), "{mode}");
        }
        // Sin enlace confirmado: trazado de puntero
        assert!(shows_media(None, false));
    }

    fn connected(mode: &str) -> Status {
        Status::Connected {
            pc_name: "PC".into(), mode: mode.into(), mode_by_pc: false,
            slot: 0, player: 1, role: crate::link::Role::Wiimote, rtt_ms: None,
            supports_cemu: true, supports_switch: true, supports_retroarch: true, receiver: crate::link::ReceiverCapabilities::default(), pad: "wiimote".into(),
            notice: None, mode_seq: 1, pad_seq: 1, own_nunchuk: false, screen_only: None, game: None,
        }
    }

    /// Pinta un frame (sin eventos) para tener el trazado y sus formas.
    fn render(ctl: &mut ControllerUi, buttons: &Buttons, status: &Status, press: Press) {
        render_with(ctl, buttons, status, press, false);
    }

    /// Como [render], eligiendo el ajuste «Multimedia en todos los modos».
    fn render_with(ctl: &mut ControllerUi, buttons: &Buttons, status: &Status, press: Press, media_everywhere: bool) {
        let ctx = egui::Context::default();
        let size = Vec2::new(400.0, 900.0);
        let _ = ctx.run(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), ..Default::default() }, |ctx| {
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
                ctl.show(ui, buttons, status, false, None, 0.0, press, "RetroPad", media_everywhere);
            });
        });
    }

    /// El trazado de verdad: con la fila desplegada, sus botones existen en
    /// modo puntero y no en Dolphin; con el ajuste, también en Dolphin.
    #[test]
    fn la_fila_multimedia_no_se_pinta_fuera_del_puntero() {
        let mut ctl = ControllerUi::new();
        ctl.show_media = true;
        let b = Buttons::new();
        let press = Press { slide: false, sticky: true };
        let has_media = |ctl: &ControllerUi| {
            ctl.hits.iter().any(|(_, t)| matches!(t, Target::Button(bit) if *bit == pmp::BTN_MEDIA_PLAY_PAUSE))
        };
        render_with(&mut ctl, &b, &connected("pointer"), press, false);
        assert!(has_media(&ctl), "en puntero la fila se pinta");
        render_with(&mut ctl, &b, &connected("dolphin"), press, false);
        assert!(!has_media(&ctl), "en Dolphin no");
        render_with(&mut ctl, &b, &connected("dolphin"), press, true);
        assert!(has_media(&ctl), "con el ajuste, también en Dolphin");
        render_with(&mut ctl, &b, &Status::Connecting, press, false);
        assert!(has_media(&ctl), "sin enlace confirmado, trazado de puntero");
    }

    fn position(ctl: &ControllerUi, target: Target) -> Pos2 {
        match ctl.hits.iter().find(|(_, t)| *t == target).unwrap_or_else(|| panic!("sin {target:?}")).0 {
            Shape::Circle { c, .. } => c,
            Shape::Rect(r) => r.center(),
        }
    }

    /// Un punto sin nada: el hueco entre 1 y 2 (sin Home están a 36·s del
    /// centro y su radio es 26·s).
    fn empty(ctl: &ControllerUi) -> Pos2 {
        let one = position(ctl, Target::Button(pmp::BTN_ONE));
        let two = position(ctl, Target::Button(pmp::BTN_TWO));
        let mid = one + (two - one) * 0.5;
        assert!(touch::hit_test(&ctl.hits, mid).is_none(), "el hueco entre 1 y 2 debería estar vacío: {mid:?}");
        mid
    }

    fn setup(press: Press) -> (ControllerUi, Buttons) {
        let mut ctl = ControllerUi::new();
        let b = Buttons::new();
        // modo puntero: sin Home entre 1 y 2, y la tira izquierda es Precisión
        render(&mut ctl, &b, &connected("pointer"), press);
        (ctl, b)
    }

    #[test]
    fn deslizar_cambia_de_boton_sin_levantar_el_dedo() {
        let (mut ctl, b) = setup(Press { slide: true, sticky: true });
        let (one, two) = (position(&ctl, Target::Button(pmp::BTN_ONE)), position(&ctl, Target::Button(pmp::BTN_TWO)));
        ctl.begin(1, one, &b);
        assert_eq!(b.physical(), pmp::BTN_ONE);
        ctl.moved(1, empty(&ctl), &b);
        assert_eq!(b.physical(), 0, "al salir al vacío se suelta");
        ctl.moved(1, two, &b);
        assert_eq!(b.physical(), pmp::BTN_TWO, "y al entrar en el 2 se pulsa el 2");
        ctl.end(1, &b);
        assert_eq!(b.physical(), 0);
        assert!(ctl.touches.is_empty());
    }

    #[test]
    fn un_boton_que_lleva_otro_dedo_no_se_le_quita() {
        let (mut ctl, b) = setup(Press { slide: true, sticky: true });
        let (one, two) = (position(&ctl, Target::Button(pmp::BTN_ONE)), position(&ctl, Target::Button(pmp::BTN_TWO)));
        ctl.begin(1, one, &b);
        ctl.begin(2, two, &b);
        assert_eq!(b.physical(), pmp::BTN_ONE | pmp::BTN_TWO);
        // el segundo dedo se arrastra al 1: ya lo lleva el primero, así que
        // suelta el 2 y se queda sin nada
        ctl.moved(2, one, &b);
        assert_eq!(b.physical(), pmp::BTN_ONE, "el 1 sigue siendo del primer dedo");
        assert_eq!(ctl.touches[&2].target, Target::Free);
        ctl.end(2, &b);
        assert_eq!(b.physical(), pmp::BTN_ONE, "al levantar el segundo, el 1 sigue pulsado");
        ctl.end(1, &b);
        assert_eq!(b.physical(), 0);
        // y dos dedos que caen a la vez en el mismo botón: lo suelta el último
        let (mut ctl, b) = setup(Press::default());
        let one = position(&ctl, Target::Button(pmp::BTN_ONE));
        ctl.begin(3, one, &b);
        ctl.begin(4, one, &b);
        ctl.end(3, &b);
        assert_eq!(b.physical(), pmp::BTN_ONE, "el otro dedo lo sigue apretando");
        ctl.end(4, &b);
        assert_eq!(b.physical(), 0);
    }

    #[test]
    fn un_dedo_que_nace_en_el_vacio_pulsa_al_entrar() {
        let (mut ctl, b) = setup(Press { slide: true, sticky: true });
        let hueco = empty(&ctl);
        ctl.begin(1, hueco, &b);
        assert_eq!(b.physical(), 0, "en el vacío no se pulsa nada todavía");
        assert_eq!(ctl.touches[&1].target, Target::Free);
        ctl.moved(1, position(&ctl, Target::Button(pmp::BTN_A)), &b);
        assert_eq!(b.physical(), pmp::BTN_A);
        ctl.end(1, &b);
        assert_eq!(b.physical(), 0);
        // sin deslizar, ese dedo ni siquiera se registra
        let (mut ctl, b) = setup(Press::default());
        let hueco = empty(&ctl);
        ctl.begin(2, hueco, &b);
        assert!(ctl.touches.is_empty());
        ctl.moved(2, position(&ctl, Target::Button(pmp::BTN_A)), &b);
        assert_eq!(b.physical(), 0);
    }

    #[test]
    fn de_serie_el_boton_se_queda_pulsado_al_salirse() {
        let (mut ctl, b) = setup(Press::default());
        let one = position(&ctl, Target::Button(pmp::BTN_ONE));
        ctl.begin(1, one, &b);
        ctl.moved(1, empty(&ctl), &b);
        assert_eq!(b.physical(), pmp::BTN_ONE, "pegajoso: sigue pulsado fuera del botón");
        ctl.moved(1, position(&ctl, Target::Button(pmp::BTN_TWO)), &b);
        assert_eq!(b.physical(), pmp::BTN_ONE, "y no coge el 2 al pasar por encima");
        ctl.end(1, &b);
        assert_eq!(b.physical(), 0);
    }

    #[test]
    fn sin_pegajoso_se_suelta_al_salir_y_no_vuelve() {
        let (mut ctl, b) = setup(Press { slide: false, sticky: false });
        let one = position(&ctl, Target::Button(pmp::BTN_ONE));
        ctl.begin(1, one, &b);
        assert_eq!(b.physical(), pmp::BTN_ONE);
        ctl.moved(1, one + Vec2::new(2.0, 0.0), &b);
        assert_eq!(b.physical(), pmp::BTN_ONE, "dentro del botón no pasa nada");
        ctl.moved(1, empty(&ctl), &b);
        assert_eq!(b.physical(), 0, "al salirse se suelta");
        ctl.moved(1, one, &b);
        assert_eq!(b.physical(), 0, "y el dedo ya no vuelve a coger nada");
        ctl.end(1, &b);
        assert_eq!(b.physical(), 0);
    }

    #[test]
    fn la_tira_de_precision_no_se_suelta_al_salir_ni_se_pulsa_deslizando() {
        // es «mantener»: aunque se apague lo pegajoso, sigue hasta levantar
        let (mut ctl, b) = setup(Press { slide: false, sticky: false });
        let strip = position(&ctl, Target::Hold(pmp::BTN_PRECISION));
        ctl.begin(1, strip, &b);
        assert_eq!(b.physical(), pmp::BTN_PRECISION);
        ctl.moved(1, position(&ctl, Target::Button(pmp::BTN_A)), &b);
        assert_eq!(b.physical(), pmp::BTN_PRECISION, "ni se suelta ni pulsa la A");
        ctl.end(1, &b);
        assert_eq!(b.physical(), 0);
        // deslizando: un dedo que pasa por encima no la pulsa
        let (mut ctl, b) = setup(Press { slide: true, sticky: true });
        let strip = position(&ctl, Target::Hold(pmp::BTN_PRECISION));
        ctl.begin(2, position(&ctl, Target::Button(pmp::BTN_A)), &b);
        assert_eq!(b.physical(), pmp::BTN_A);
        ctl.moved(2, strip, &b);
        assert_eq!(b.physical(), 0, "suelta la A y la tira no se pulsa");
        ctl.end(2, &b);
        // y si el dedo nace en la tira, deslizar tampoco se la quita
        ctl.begin(3, strip, &b);
        assert_eq!(b.physical(), pmp::BTN_PRECISION);
        ctl.moved(3, position(&ctl, Target::Button(pmp::BTN_A)), &b);
        assert_eq!(b.physical(), pmp::BTN_PRECISION);
        ctl.end(3, &b);
        assert_eq!(b.physical(), 0);
    }

    #[test]
    fn deslizar_no_engancha_la_cruceta_la_diana_ni_el_scroll() {
        let (mut ctl, b) = setup(Press { slide: true, sticky: true });
        ctl.begin(1, position(&ctl, Target::Button(pmp::BTN_A)), &b);
        assert_eq!(b.physical(), pmp::BTN_A);
        for target in [Target::Dpad, Target::Recenter, Target::Scroll] {
            ctl.moved(1, position(&ctl, target), &b);
            assert_eq!(b.physical(), 0, "{target:?}: se suelta la A y no se coge nada");
            assert_eq!(b.recenter_count(), 0);
            assert_eq!(b.drain_scroll(), 0);
        }
        ctl.end(1, &b);
        assert_eq!(b.physical(), 0);
    }
}
