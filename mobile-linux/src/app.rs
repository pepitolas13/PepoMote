//! Pantallas: Inicio, Conectar (descubrimiento), IP a mano, Código, Mando,
//! Nunchuk y GamePad de Wii U. Misma lógica de navegación que MainActivity
//! en Android: la pantalla del mando la decide lo que diga el receptor
//! (modo y tipo de mando), no solo lo que se tocó en Inicio.

use crate::buttons::Buttons;
use crate::calib::{self, Axes};
use crate::discovery::{self, Receiver, ScanEvent};
use crate::inhibit::Inhibit;
use crate::link::{self, Link, Role, Status};
use crate::screen;
use crate::sensor;
use crate::store::{self, Pairing, Settings};
use crate::theme;
use crate::ui::controller::{Action, ControllerUi};
use crate::ui::gamepad::{Action as GamePadAction, GamePadUi, Inputs as GamePadInputs};
use crate::ui::keypad::{keypad, Key};
use crate::ui::nunchuk::{Action as NunchukAction, NunchukUi};
use crate::ui::text::{effect, TextDialog};
use crate::ui::touch::Press;
use egui::{RichText, Vec2};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::i18n;
use crate::tr;

/// Resultado del hilo de versiones, a la espera de que la UI lo guarde en
/// settings.json (solo la UI escribe ese archivo: sin carreras).
static UPDATE_RESULT: Mutex<Option<(u64, crate::update::Version)>> = Mutex::new(None);

pub fn push_update_result(now: u64, latest: crate::update::Version) {
    *UPDATE_RESULT.lock().unwrap_or_else(|e| e.into_inner()) = Some((now, latest));
}

/// Log de diagnóstico: ~/.config/pepomote/mobile.log (tamaño de
/// pantalla, escala, toques…). Para saber qué ve la app en un móvil real.
pub fn log_line(msg: &str) {
    let Some(d) = directories::ProjectDirs::from("dev", "pepotech", "PepoMote") else { return };
    let dir = d.config_dir();
    let _ = std::fs::create_dir_all(dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("mobile.log")) {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "{t} {msg}");
    }
    eprintln!("[pepomote] {msg}");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Home,
    Pair,
    Manual,
    Code,
    Controller,
    /// La otra mano: stick, C y Z (el móvil que hace de mando decide el modo).
    Nunchuk,
    /// GamePad (o Pro Controller) de Wii U para Cemu: apaisado, 80 bytes.
    GamePad,
    Calibrate,
}

/// Intención Wii U pendiente (tarjeta del inicio o chip Wii U): el GamePad
/// se muestra ya, optimista, y la resuelve el primer eco/difusión de `mode`
/// posterior a la petición (`seq` = `mode_seq` al pedirlo); `since` acota la
/// espera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Intent {
    None,
    WiiU { seq: u32, since: Instant },
    Switch { seq: u32, since: Instant },
    RetroArch { seq: u32, since: Instant },
}

impl Intent {
    fn wanted(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::WiiU { .. } => Some("cemu"),
            Self::Switch { .. } => Some("switch"),
            Self::RetroArch { .. } => Some("retroarch"),
        }
    }

    fn for_mode(mode: &str, seq: u32, since: Instant) -> Intent {
        match mode {
            "cemu" => Intent::WiiU { seq, since },
            "switch" => Intent::Switch { seq, since },
            "retroarch" => Intent::RetroArch { seq, since },
            _ => Intent::None,
        }
    }
}

/// The effective Pro of Cemu player 2 must not replace their Switch choice;
/// RetroArch keeps its own pad (retropad / nes / gun).
struct RequestedPads { cemu: &'static str, switch: &'static str, retro: &'static str }

impl Default for RequestedPads {
    fn default() -> Self { Self { cemu: "gamepad", switch: "pro", retro: "retropad" } }
}

impl RequestedPads {
    fn get(&self, mode: &str) -> Option<&'static str> {
        match mode { "cemu" => Some(self.cemu), "switch" => Some(self.switch), "retroarch" => Some(self.retro), _ => None }
    }

    fn for_receiver(&self, mode: &str, receiver: &link::ReceiverCapabilities) -> Option<&'static str> {
        self.get(mode).map(|pad| receiver.restored_pad(pad))
    }

    fn set(&mut self, mode: &str, pad: &'static str) {
        match mode {
            "cemu" if matches!(pad, "gamepad" | "wiimote") => self.cemu = pad,
            "switch" => self.switch = "pro",
            "retroarch" if link::is_retro_pad(pad) => self.retro = pad,
            _ => {}
        }
    }
}

/// Petición de tipo de mando (`pad`) sin eco todavía: el selector la pinta a
/// medio tono hasta el eco (`pad_seq` avanza) o hasta que caduque.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PadRequest {
    pad: &'static str,
    seq: u32,
    since: Instant,
}

/// Códigos con los que el PC rechaza el emparejamiento guardado: solo
/// emparejar de nuevo (el código bajo su QR) lo arregla, así que en vez de
/// un error suelto en el inicio se abre Conectar con la explicación.
fn needs_new_pairing(code: &str) -> bool {
    matches!(code, "bad_token" | "bad_code")
}

/// Explicación para la pantalla Conectar cuando el PC ya no reconoce el móvil.
fn re_pair_reason(pc_name: Option<&str>) -> String {
    let pc = pc_name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or(tr!("pair.your_pc"));
    tr!("pair.re_pair", pc)
}

fn notice_old_pc() -> &'static str {
    tr!("notice.old_pc")
}

fn notice_only_p1() -> &'static str {
    tr!("notice.only_p1")
}
/// Sin eco de `mode` en este tiempo, la intención Wii U se da por fallida.
const INTENT_TIMEOUT: Duration = Duration::from_secs(3);
/// Sin eco de `pad` en este tiempo (PC antiguo), el selector vuelve a lo que hay.
const PAD_TIMEOUT: Duration = Duration::from_secs(2);

/// (`mode_seq`, `pad_seq`) de la sesión; (0, 0) si no está conectada.
fn seqs(status: &Status) -> (u32, u32) {
    match status {
        Status::Connected { mode_seq, pad_seq, .. } => (*mode_seq, *pad_seq),
        _ => (0, 0),
    }
}

/// Pantalla de juego que toca (addendum UX v2), pura:
/// 1. GamePad si la sesión está en Wii U como mando y no es Mando de Wii;
/// 2. GamePad si hay intención Wii U pendiente (conectando o conectados);
/// 3. Nunchuk si ese es el papel (el del `ok`, o el pedido mientras conecta);
/// 4. si no, el layout Wii.
fn route(status: &Status, link_role: Role, intent: Intent) -> Screen {
    if let Status::Connected { mode, role, pad, receiver, .. } = status {
        // RetroArch: solo el RetroPad es el apaisado de dos sticks; el mando
        // de NES y la pistola son el layout Wii de siempre
        if *role == Role::Wiimote && (mode == "switch" || (mode == "cemu" && pad != "wiimote") || (mode == "retroarch" && receiver.restored_pad(pad) == "retropad")) {
            return Screen::GamePad;
        }
    }
    if intent != Intent::None {
        return Screen::GamePad;
    }
    let role = match status {
        Status::Connected { role, .. } => *role,
        _ => link_role,
    };
    match role {
        Role::Nunchuk => Screen::Nunchuk,
        Role::Wiimote => Screen::Controller,
    }
}

/// La intención Wii U ante el estado actual: sigue pendiente, o se resuelve
/// (con el aviso que toque si el receptor no ha ido a Wii U): eco/difusión
/// posterior que no es `cemu`, `ok` sin `cemu` en `modes` (PC antiguo, que
/// no hace falta esperar) o silencio más allá de `INTENT_TIMEOUT`.
fn settle(status: &Status, intent: Intent, now: Instant) -> (Intent, Option<&'static str>) {
    let (seq, since) = match intent {
        Intent::WiiU { seq, since } | Intent::Switch { seq, since } | Intent::RetroArch { seq, since } => (seq, since),
        Intent::None => return (Intent::None, None),
    };
    let Status::Connected { mode, mode_by_pc, mode_seq, supports_cemu, supports_switch, supports_retroarch, slot, .. } = status else {
        return (intent, None); // aún conectando
    };
    let wanted = intent.wanted().unwrap();
    let supported = match wanted { "switch" => *supports_switch, "retroarch" => *supports_retroarch, _ => *supports_cemu };
    if mode == wanted && supported {
        return (Intent::None, None);
    }
    if *mode_seq > seq && *mode_by_pc {
        // el PC cambió de modo por su cuenta (abrió o cerró un emulador): la
        // intención se descarta sin aviso, el suyo ya lo explica
        return (Intent::None, None);
    }
    let failed = *mode_seq > seq || !supported || now.saturating_duration_since(since) > INTENT_TIMEOUT;
    if !failed {
        return (intent, None);
    }
    let notice = if supported && *slot != 0 {
        notice_only_p1()
    } else {
        match wanted {
            "switch" => tr!("notice.old_pc_switch"),
            "retroarch" => tr!("notice.old_pc_retroarch"),
            _ => notice_old_pc(),
        }
    };
    (Intent::None, Some(notice))
}

/// Petición de `pad` pendiente: se cierra con el eco (`pad_seq` avanza) o
/// pasado `PAD_TIMEOUT` (PC antiguo: no contesta).
fn settle_pad(status: &Status, req: Option<PadRequest>, now: Instant) -> Option<PadRequest> {
    let r = req?;
    if !status.accepts_pad(r.pad) { return None; }
    match status {
        Status::Connected { pad_seq, .. } if *pad_seq > r.seq => None,
        Status::Connected { .. } if now.saturating_duration_since(r.since) > PAD_TIMEOUT => None,
        Status::Connected { .. } => Some(r),
        _ => None,
    }
}

/// Calibración de ejes en curso: fuente cruda en su hilo, paso actual y lo
/// deducido hasta ahora.
struct Calib {
    rx: mpsc::Receiver<sensor::Sample>,
    stop: Arc<AtomicBool>,
    step: usize,
    /// Captura en curso: (inicio, duración, muestras)
    capture: Option<(Instant, Duration, Vec<sensor::Sample>)>,
    accel: [Option<i8>; 3],
    gyro: [Option<i8>; 3],
    msg: Option<String>,
    done: Option<Axes>,
    last: Option<sensor::Sample>,
}

pub struct MobileApp {
    screen: Screen,
    pairing: Option<Pairing>,
    link: Option<Link>,
    buttons: Arc<Buttons>,
    fake: bool,
    // Conectar
    discovered: Vec<Receiver>,
    scan_rx: Option<mpsc::Receiver<ScanEvent>>,
    last_scan: Option<Instant>,
    // Código / IP a mano
    target: Option<Receiver>,
    code: String,
    manual: String,
    pair_rx: Option<mpsc::Receiver<Result<Pairing, String>>>,
    pair_error: Option<String>,
    /// Por qué está abierta Conectar: el PC rechazó el emparejamiento guardado
    /// (`bad_token`) y se explica ahí mismo. None = emparejamiento normal.
    pair_reason: Option<String>,
    // Mando / Nunchuk / GamePad
    controller: ControllerUi,
    nunchuk: NunchukUi,
    gamepad: GamePadUi,
    /// Giro apaisado (settings.json).
    settings: Settings,
    /// Intención Wii U pendiente (tarjeta o chip): GamePad optimista hasta
    /// el eco de `mode`.
    intent: Intent,
    /// Petición de tipo de mando (`pad`) sin eco todavía.
    pad_pending: Option<PadRequest>,
    requested_pads: RequestedPads,
    last_mode: Option<String>,
    last_layout: Option<String>,
    extended_mode: &'static str,
    after_pair_mode: Option<&'static str>,
    /// Papel con el que se abrió el enlace vigente (otro papel = reconectar).
    link_role: Role,
    dolphin_only: bool,
    recenter_at: Option<Instant>,
    was_connected: bool,
    error: Option<String>,
    /// Qué sensor hay (se mira al arrancar y al fallar una conexión, no por frame).
    sensor_desc: String,
    /// Diagnóstico: lo que la app ve de la pantalla y de la entrada táctil.
    diag: String,
    diag_touches: u32,
    diag_pointer: u32,
    calib: Option<Calib>,
    /// Pantalla encendida y sin suspensión mientras dura la conexión.
    inhibit: Option<Inhibit>,
    /// Canal de la pantalla del GamePad (doble pantalla): abierto solo
    /// mientras se juega en la pantalla GamePad como GamePad.
    pad_screen: Option<screen::Client>,
    /// Pantalla completa del GamePad: desde cuándo está activa (para avisar,
    /// una vez, de un receptor que no la conoce).
    full_screen_since: Option<Instant>,
    full_screen_warned: bool,
    /// Teclado para el teclado en pantalla de Cemu (modo Wii U): tapa la
    /// pantalla de juego sin cambiarla (los INPUT siguen saliendo).
    text_dialog: Option<TextDialog>,
    /// RetroArch: teclas de mantener (rebobinar) que el GamePad tiene bajo el
    /// dedo ahora mismo, ya avisadas al receptor.
    hotkey_holds: Vec<&'static str>,
    /// RetroArch: mando de consola elegido a mano (retro_layouts.json).
    retro_layouts: store::RetroLayouts,
    /// RetroArch: el selector de mando de consola está abierto (tapa la
    /// pantalla de juego como el teclado).
    layout_picker: bool,
    /// RetroArch: última plantilla avisada al receptor (`pad.layout`).
    last_sent_layout: Option<&'static str>,
}

fn describe_sensors(fake: bool) -> String {
    match sensor::open_corrected(fake) {
        Ok(s) => s.describe(),
        Err(e) => tr!("home.no_sensors", e),
    }
}

impl MobileApp {
    pub fn new(cc: &eframe::CreationContext<'_>, fake: bool, autoconnect: Option<String>) -> Self {
        theme::apply(&cc.egui_ctx);
        let mut app = Self::build(fake);
        theme::set_preference(&cc.egui_ctx, app.settings.theme);
        i18n::set(app.settings.lang.resolve());
        if let Some(m) = autoconnect {
            match m.as_str() {
                "nunchuk" => app.open_nunchuk(),
                "cemu" => app.open_gamepad(),
                "switch" => app.open_switch(),
                "dolphin" => app.open_controller(Some("dolphin"), true),
                _ => app.open_controller(Some("pointer"), false),
            }
        }
        app
    }

    fn build(fake: bool) -> Self {
        let settings = store::load_settings();
        let buttons = Arc::new(Buttons::new());
        buttons.set_rotation(settings.rotation.as_u8());
        Self {
            screen: Screen::Home,
            pairing: store::load(),
            link: None,
            buttons,
            fake,
            sensor_desc: describe_sensors(fake),
            diag: String::new(),
            diag_touches: 0,
            diag_pointer: 0,
            calib: None,
            inhibit: None,
            pad_screen: None,
            full_screen_since: None,
            full_screen_warned: false,
            text_dialog: None,
            discovered: Vec::new(),
            scan_rx: None,
            last_scan: None,
            target: None,
            code: String::new(),
            manual: String::new(),
            pair_rx: None,
            pair_error: None,
            pair_reason: None,
            controller: ControllerUi::new(),
            nunchuk: NunchukUi::new(),
            gamepad: GamePadUi::new(),
            settings,
            intent: Intent::None,
            requested_pads: RequestedPads::default(),
            hotkey_holds: Vec::new(),
            retro_layouts: store::load_retro_layouts(),
            layout_picker: false,
            last_sent_layout: None,
            last_mode: None,
            last_layout: None,
            extended_mode: "cemu",
            after_pair_mode: None,
            pad_pending: None,
            link_role: Role::Wiimote,
            dolphin_only: false,
            recenter_at: None,
            was_connected: false,
            error: None,
        }
    }

    fn link_alive(&self) -> bool {
        self.link
            .as_ref()
            .is_some_and(|l| matches!(l.status(), Status::Connected { .. } | Status::Connecting | Status::Reconnecting { .. }))
    }

    /// Hay sesión abierta ahora mismo (no arrancando ni rehaciéndose) con ese PC.
    fn link_connected_to(&self, name: &str) -> bool {
        self.link
            .as_ref()
            .is_some_and(|l| matches!(l.status(), Status::Connected { pc_name, .. } if pc_name == name))
    }

    /// Conecta (o cambia de modo si ya hay enlace) y va al mando.
    fn open_controller(&mut self, mode: Option<&'static str>, dolphin_only: bool) {
        self.after_pair_mode = None;
        self.dolphin_only = dolphin_only;
        self.open_link(Role::Wiimote, mode);
    }

    /// Conecta como Nunchuk (la otra mano) y va a su pantalla. Sin modo: lo
    /// decide el móvil que hace de mando.
    fn open_nunchuk(&mut self) {
        self.open_link(Role::Nunchuk, None);
    }

    /// Wii U: conecta como mando (si hace falta), pide `mode cemu` y muestra
    /// el GamePad AL INSTANTE (optimista, conectando incluido); el eco lo
    /// confirma o devuelve al layout Wii con aviso (`settle`).
    fn open_gamepad(&mut self) {
        self.open_extended("cemu");
    }

    fn open_switch(&mut self) {
        self.open_extended("switch");
    }

    fn open_retroarch(&mut self) {
        self.open_extended("retroarch");
    }

    fn open_extended(&mut self, mode: &'static str) {
        self.extended_mode = mode;
        if self.pairing.is_none() { self.after_pair_mode = Some(mode); }
        self.dolphin_only = false;
        let before = self.mode_seq();
        self.open_link(Role::Wiimote, Some(mode));
        if self.link.is_some() {
            let seq = before.min(self.mode_seq());
            let since = Instant::now();
            self.intent = Intent::for_mode(mode, seq, since);
            self.go_play();
        }
    }

    fn ext_confirmed(&self) -> bool {
        self.link.as_ref().is_some_and(|l| l.status().ext_confirmed())
    }

    fn request_mode(&mut self, mode: &'static str) {
        if matches!(mode, "cemu" | "switch" | "retroarch") { self.extended_mode = mode; }
        self.gamepad.release(&self.buttons);
        self.controller.release(&self.buttons);
        self.release_hotkeys();
        self.pad_pending = None;
        let seq = self.mode_seq();
        let since = Instant::now();
        self.intent = Intent::for_mode(mode, seq, since);
        if let Some(link) = &self.link { link.send_mode(mode); }
        self.go_play();
    }

    /// Modo RetroArch: el modo confirmado de la sesión (para el selector de
    /// mando del mando vertical, que solo sabe el `pad` pedido).
    fn session_mode(&self) -> Option<String> {
        self.link.as_ref().and_then(|l| match l.status() { Status::Connected { mode, .. } => Some(mode), _ => None })
    }

    /// Tecla rápida de RetroArch de un toque (o flanco de una de mantener).
    fn send_hotkey(&self, name: &str, down: bool) {
        if let Some(l) = &self.link {
            if matches!(l.status(), Status::Connected { mode, .. } if mode == "retroarch") { l.send_hotkey(name, down); }
        }
    }

    /// Las de mantener que el GamePad tenga bajo el dedo: flancos al receptor.
    fn sync_hotkey_holds(&mut self) {
        let held = self.gamepad.held_hotkeys();
        for n in held.iter().filter(|n| !self.hotkey_holds.contains(*n)) {
            self.send_hotkey(n, true);
        }
        for n in self.hotkey_holds.iter().filter(|n| !held.contains(*n)) {
            self.send_hotkey(n, false);
        }
        self.hotkey_holds = held;
    }

    /// Nada de mantener queda pulsado (cambio de pantalla, de modo o de enlace).
    fn release_hotkeys(&mut self) {
        for n in std::mem::take(&mut self.hotkey_holds) {
            self.send_hotkey(n, false);
        }
    }

    /// Lo que el receptor sabe del juego cargado en RetroArch.
    fn retro_game(&self) -> Option<link::GameInfo> {
        match self.link.as_ref().map(|l| l.status()) {
            Some(Status::Connected { game, .. }) => game,
            _ => None,
        }
    }

    /// Plantilla de consola que toca como RetroPad: la elegida a mano para
    /// este juego (o la global), si no la consola que anunció el PC, si no el
    /// RetroPad completo.
    fn effective_retro_layout(&self) -> &'static str {
        let game = self.retro_game();
        let chosen = self.retro_layouts.choice_for(game.as_ref().map(|g| g.path.as_str()));
        pmp::retro::effective_layout(game.as_ref().and_then(|g| g.console.as_deref()), chosen)
    }

    fn open_layout_picker(&mut self) {
        self.gamepad.release(&self.buttons);
        self.controller.release(&self.buttons);
        self.layout_picker = true;
    }

    /// El selector de mando de consola: la elección se guarda (por juego si
    /// hay uno cargado) y la pantalla de juego la recoge en el siguiente frame.
    fn ui_layout_picker(&mut self, ui: &mut egui::Ui) {
        let game = self.retro_game();
        let path = game.as_ref().map(|g| g.path.as_str()).filter(|p| !p.is_empty());
        let auto_id = pmp::retro::effective_layout(game.as_ref().and_then(|g| g.console.as_deref()), None);
        let auto_name = pmp::retro::layout(auto_id).map_or("RetroPad", |l| l.name);
        let chosen = self.retro_layouts.choice_for(path).map(str::to_owned);
        let title = game.as_ref().map(|g| g.title.as_str()).filter(|t| !t.is_empty());
        match crate::ui::layout_picker::show(ui, auto_name, chosen.as_deref(), title) {
            Some(crate::ui::layout_picker::Pick::Auto) => {
                self.retro_layouts.pick(path, None);
                store::save_retro_layouts(&self.retro_layouts);
                self.layout_picker = false;
            }
            Some(crate::ui::layout_picker::Pick::Layout(id)) => {
                self.retro_layouts.pick(path, Some(id));
                store::save_retro_layouts(&self.retro_layouts);
                self.layout_picker = false;
            }
            Some(crate::ui::layout_picker::Pick::Close) => self.layout_picker = false,
            None => {}
        }
    }

    fn request_pad(&mut self, mode: &str, pad: &'static str) {
        let Some(link) = &self.link else { return };
        let status = link.status();
        if !matches!(&status, Status::Connected { mode: current, .. } if current == mode) || !status.accepts_pad(pad) { return; }
        self.requested_pads.set(mode, pad);
        self.gamepad.release(&self.buttons);
        self.controller.release(&self.buttons);
        let seq = seqs(&status).1;
        link.send_pad(pad);
        self.pad_pending = Some(PadRequest { pad, seq, since: Instant::now() });
    }

    /// `mode_seq` de la sesión vigente (0 si no la hay o aún conecta).
    fn mode_seq(&self) -> u32 {
        self.link.as_ref().map(|l| seqs(&l.status()).0).unwrap_or(0)
    }

    /// A la pantalla de juego que toca ahora mismo (`route`).
    fn go_play(&mut self) {
        let status = self.link.as_ref().map(|l| l.status()).unwrap_or(Status::Connecting);
        self.screen = route(&status, self.link_role, self.intent);
    }

    fn open_link(&mut self, role: Role, mode: Option<&'static str>) {
        if self.link_alive() {
            if self.link_role == role {
                if let (Some(m), Some(l)) = (mode, &self.link) {
                    l.send_mode(m);
                }
                self.go_play();
                return;
            }
            // otro papel: el receptor asigna el slot según el hello, así
            // que toca reconectar
            self.close_link();
        }
        let Some(pairing) = self.pairing.clone() else {
            self.screen = Screen::Pair;
            return;
        };
        let source = match sensor::open_corrected(self.fake) {
            Ok(source) => source,
            Err(error) => {
                log_line(&format!("sensores no disponibles, siguen los botones: {error}"));
                self.sensor_desc = tr!("home.controls_only").to_owned();
                Box::new(sensor::Unavailable)
            }
        };
        self.buttons.release_all();
        self.link = Some(Link::connect(
            pairing, self.buttons.clone(), source, mode.map(|m| m.to_owned()), role,
            self.settings.own_nunchuk, self.settings.gamepad_full_screen, self.settings.receiver_notices,
        ));
        self.link_role = role;
        self.was_connected = false;
        self.go_play();
    }

    fn close_link(&mut self) {
        self.close_screen();
        self.full_screen_since = None;
        self.full_screen_warned = false;
        if let Some(l) = self.link.take() {
            l.disconnect();
        }
        self.buttons.release_all();
        self.inhibit = None;
        self.intent = Intent::None;
        self.pad_pending = None;
        self.last_mode = None;
        self.last_layout = None;
    }

    /// Cierra el canal de la pantalla del GamePad, si estaba abierto.
    fn close_screen(&mut self) {
        if let Some(c) = self.pad_screen.take() {
            c.stop();
        }
    }

    /// La sesión está en Wii U, Switch o RetroArch (el teclado solo tiene sentido ahí).
    fn mode_has_keyboard(&self) -> bool {
        self.link
            .as_ref()
            .is_some_and(|l| l.status().has_keyboard())
    }

    /// Abre el teclado tapando la pantalla de juego. Los dedos que hubiera se
    /// sueltan (sus toques ya no llegarán al mando), pero la pantalla no
    /// cambia, así los INPUT siguen saliendo igual.
    ///
    /// En modo puntero, además y en este orden: se congela la pose ANTES del
    /// clic (si el móvil se mueve entre los dos flancos, 70 ms, el PC ve un
    /// ARRASTRE y en un campo de texto eso selecciona en vez de dejar el
    /// cursor) y se hace el clic izquierdo con el bit A de siempre, que va
    /// coordinado con la posición porque viaja en el mismo paquete.
    fn open_text_dialog(&mut self) {
        if !self.mode_has_keyboard() { self.text_dialog = None; return; }
        self.gamepad.release(&self.buttons);
        self.controller.release(&self.buttons);
        let status = self.link.as_ref().map(|l| l.status());
        if status.as_ref().is_some_and(|s| s.holds_pointer()) {
            self.buttons.set_pointer_hold(true);
        }
        if status
            .as_ref()
            .is_some_and(|s| s.clicks_before_keyboard(self.settings.keyboard_click_first))
        {
            self.buttons.set(pmp::BTN_A, true);
            self.buttons.set(pmp::BTN_A, false);
        }
        self.text_dialog = Some(TextDialog::new());
    }

    /// El diálogo del teclado: cada botón manda lo que toque por el canal de
    /// control (`effect`, puro) y vacía el campo o cierra según diga.
    fn ui_text(&mut self, ui: &mut egui::Ui) {
        let (Some(dlg), Some(link)) = (self.text_dialog.as_mut(), self.link.as_ref()) else {
            self.text_dialog = None;
            return;
        };
        let mode = match link.status() { Status::Connected { mode, .. } => mode, _ => String::new() };
        if let Some(b) = dlg.show(ui, &mode) {
            let e = effect(b, &dlg.field);
            if let Some(t) = e.send {
                link.send_text(&t);
            }
            if e.clear_field {
                dlg.field.clear();
            }
            if e.close {
                self.text_dialog = None;
            }
        }
    }

    /// El receptor dice que somos el GamePad (el que tiene pantalla; un Pro
    /// Controller no).
    fn pad_is_gamepad(&self) -> bool {
        self.link
            .as_ref()
            .is_some_and(|l| matches!(l.status(), Status::Connected { mode, pad, .. } if mode == "cemu" && pad == "gamepad"))
    }

    /// Canal de la pantalla del GamePad (doble pantalla): abierto solo
    /// mientras `want` (pantalla GamePad operativa, Wii U confirmado y
    /// `pad == "gamepad"`); cerrado al salir de esa pantalla, al pasar a
    /// Pro/Mando de Wii o al perder el enlace. Idempotente: una sola
    /// instancia, que se reabre si cambia la sesión o el tamaño de la zona.
    fn sync_screen(&mut self, ctx: &egui::Context, want: bool, full: bool) {
        let endpoint = if want { self.link.as_ref().and_then(|l| l.screen_endpoint()) } else { None };
        let Some(endpoint) = endpoint else {
            self.close_screen();
            return;
        };
        // el tamaño se pide como la zona táctil real: hasta que esté
        // maquetada (siguiente frame) no se abre
        let Some(zone) = self.gamepad.touch_size_px(ctx.pixels_per_point()) else { return };
        let size = if full { screen::wanted_size_full(zone) } else { screen::wanted_size(zone) };
        if self
            .pad_screen
            .as_ref()
            .is_some_and(|c| *c.endpoint() == endpoint && !screen::size_differs(c.size(), size))
        {
            return;
        }
        // la instancia anterior (si la hay) se para antes de abrir otra
        self.close_screen();
        log_line(&format!(
            "pantalla del GamePad: abriendo el canal con {}:{} (sesión {}, {}×{})",
            endpoint.host, endpoint.port, endpoint.session_id, size.0, size.1
        ));
        self.pad_screen = Some(screen::Client::start(endpoint, size, ctx.clone()));
    }

    fn poll_link(&mut self) {
        let Some(l) = &self.link else { return };
        let status = l.status();
        let now = Instant::now();
        match &status {
            Status::Failed { code, msg } => {
                self.close_screen();
                self.link = None;
                self.inhibit = None;
                self.intent = Intent::None;
                self.pad_pending = None;
                if needs_new_pairing(code) {
                    if self.screen == Screen::GamePad { self.after_pair_mode = Some(self.extended_mode); }
                    // El PC ya no reconoce el emparejamiento: a Conectar con
                    // la explicación, en vez de un error suelto en el inicio
                    self.error = None;
                    self.pair_reason = Some(re_pair_reason(self.pairing.as_ref().map(|p| p.pc_name.as_str())));
                    self.discovered.clear();
                    self.scan_rx = None; // lo que quedara en cola de un sondeo viejo no vale
                    self.last_scan = None;
                    self.screen = Screen::Pair;
                } else if code == "io" && store::load_all().list.len() >= 2 {
                    // Con varios PCs guardados, uno que no responde no es el
                    // final: a Conectar a elegir otro
                    self.error = None;
                    self.pair_reason = Some(tr!(
                        "pair.not_responding",
                        self.pairing.as_ref().map(|p| p.pc_name.as_str()).unwrap_or(tr!("pair.your_pc"))
                    ));
                    self.discovered.clear();
                    self.scan_rx = None; // lo que quedara en cola de un sondeo viejo no vale
                    self.last_scan = None;
                    self.screen = Screen::Pair;
                } else {
                    self.error = Some(msg.clone());
                    if matches!(self.screen, Screen::Controller | Screen::Nunchuk | Screen::GamePad) {
                        self.screen = Screen::Home;
                    }
                }
            }
            Status::Connecting | Status::Reconnecting { .. } | Status::Connected { .. } => {
                let reconnecting = matches!(status, Status::Reconnecting { .. });
                if let Status::Connected { mode, role, pad, receiver, .. } = &status {
                    match mode.as_str() { "switch" => self.extended_mode = "switch", "cemu" => self.extended_mode = "cemu", "retroarch" => self.extended_mode = "retroarch", _ => {} }
                    let signature = format!("{mode}/{role:?}/{pad}");
                    if self.last_layout.as_ref() != Some(&signature) {
                        self.gamepad.release(&self.buttons);
                        self.controller.release(&self.buttons);
                        self.last_layout = Some(signature);
                    }
                    if self.last_mode.as_ref() != Some(mode) {
                        self.pad_pending = None;
                        if *role == Role::Wiimote {
                            if let Some(pad) = self.requested_pads.for_receiver(mode, receiver) { l.send_pad(pad); }
                        }
                        self.last_mode = Some(mode.clone());
                    }
                    // RetroArch como RetroPad: la plantilla de consola que se enseña
                    // (por el juego que anunció el PC o la elegida a mano) va al
                    // receptor en cuanto cambia, y se avisa en pantalla
                    if mode == "retroarch" && *role == Role::Wiimote && pad == "retropad" {
                        let eff = self.effective_retro_layout();
                        if self.last_sent_layout != Some(eff) {
                            if self.last_sent_layout.is_some() {
                                l.notify(&tr!("gp.layout_changed", pmp::retro::layout(eff).map_or("RetroPad", |x| x.name)));
                            }
                            l.send_pad_layout("retropad", eff);
                            self.last_sent_layout = Some(eff);
                        }
                    } else {
                        self.last_sent_layout = None;
                    }
                } else {
                    self.gamepad.release(&self.buttons);
                    self.controller.release(&self.buttons);
                    self.last_mode = None;
                    self.last_layout = None;
                }
                // ¿Ya contestó el receptor a la intención Wii U o al `pad`?
                let (intent, notice) = settle(&status, self.intent, now);
                if let Some(n) = notice {
                    l.notify(n);
                }
                self.intent = intent;
                self.pad_pending = settle_pad(&status, self.pad_pending, now);
                // en las pantallas de juego manda el receptor (eco o difusión);
                // reconectando, la pantalla se queda como estaba
                if !reconnecting && matches!(self.screen, Screen::Controller | Screen::Nunchuk | Screen::GamePad) {
                    self.screen = route(&status, self.link_role, self.intent);
                }
                if reconnecting {
                    // al volver se recentra otra vez
                    self.was_connected = false;
                } else if !matches!(status, Status::Connected { .. }) {
                    // conectando: nada más que hacer
                } else if !self.was_connected {
                    self.was_connected = true;
                    // "Pulsar la diana" al conectar: centra el cursor con el
                    // móvil ya en la mano (igual que en Android)
                    self.recenter_at = Some(Instant::now() + Duration::from_millis(400));
                    // y la pantalla no se apaga en mitad de la partida
                    if self.inhibit.is_none() {
                        self.inhibit = Inhibit::start();
                        log_line(&match &self.inhibit {
                            Some(i) => format!("pantalla encendida mientras dure la conexión ({})", i.tool),
                            None => "sin inhibidor de pantalla (ni gnome-session-inhibit ni systemd/elogind-inhibit): la pantalla se apaga con el tiempo del sistema".to_owned(),
                        });
                    }
                } else if self.inhibit.as_mut().is_some_and(|i| !i.alive()) {
                    // el inhibidor se ha ido (sesión cerrada, polkit…): que conste
                    let tool = self.inhibit.take().map(|i| i.tool).unwrap_or("?");
                    log_line(&format!("el inhibidor de pantalla ({tool}) ha terminado solo; la pantalla vuelve al tiempo del sistema"));
                }
            }
            Status::Disconnected => {
                self.close_screen();
                self.link = None;
                self.inhibit = None;
                self.was_connected = false;
                self.intent = Intent::None;
                self.pad_pending = None;
            }
        }
        if self.recenter_at.is_some_and(|t| Instant::now() >= t) {
            self.recenter_at = None;
            self.buttons.bump_recenter();
        }
    }

    fn poll_scan(&mut self) {
        // El sondeo avisa de cada receptor según contesta (se añade sobre lo
        // que ya se enseña) y al acabar manda la lista completa, que es la
        // que quita lo que ya no contesta.
        if let Some(rx) = self.scan_rx.take() {
            let mut open = true;
            loop {
                match rx.try_recv() {
                    Ok(ScanEvent::Found(seen)) => self.discovered = discovery::merge(&self.discovered, &seen),
                    Ok(ScanEvent::Done(list)) => {
                        self.discovered = list;
                        open = false;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        open = false;
                        break;
                    }
                }
            }
            if open {
                self.scan_rx = Some(rx);
            }
        }
        if self.scan_rx.is_none() && self.last_scan.is_none_or(|t| t.elapsed() > Duration::from_millis(2500)) {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                discovery::scan_into(Duration::from_millis(1500), Some(tx));
            });
            self.scan_rx = Some(rx);
            self.last_scan = Some(Instant::now());
        }
    }

    fn start_pairing(&mut self) {
        let Some(t) = self.target.clone() else { return };
        let code = self.code.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(link::pair(&t.host, t.port, &code, &t.name));
        });
        self.pair_rx = Some(rx);
        self.pair_error = None;
    }

    fn resume_after_pairing(&mut self) {
        match self.after_pair_mode.take() {
            Some("switch") => self.open_switch(),
            Some("retroarch") => self.open_retroarch(),
            Some("cemu") => self.open_gamepad(),
            _ => self.open_controller(Some("pointer"), false),
        }
    }

    fn poll_pairing(&mut self) {
        let Some(rx) = &self.pair_rx else { return };
        match rx.try_recv() {
            Ok(Ok(p)) => {
                store::save(&p);
                self.pairing = Some(p);
                self.pair_rx = None;
                self.pair_reason = None;
                self.code.clear();
                self.resume_after_pairing();
            }
            Ok(Err(e)) => {
                self.pair_error = Some(e);
                self.pair_rx = None;
                self.code.clear();
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pair_rx = None;
            }
        }
    }

    /// Tamaño lógico, escala y eventos táctiles/puntero vistos: en pantalla
    /// (Inicio) y en el log cuando cambian.
    fn update_diag(&mut self, ctx: &egui::Context) {
        let (touches, pointer) = ctx.input(|i| {
            let t = i.events.iter().filter(|e| matches!(e, egui::Event::Touch { .. })).count() as u32;
            let p = i.events.iter().filter(|e| matches!(e, egui::Event::PointerButton { .. })).count() as u32;
            (t, p)
        });
        self.diag_touches += touches;
        self.diag_pointer += pointer;
        let r = ctx.screen_rect();
        let line = format!(
            "Pantalla {:.0}×{:.0} pt · escala {:.2} · toques {} · clics {}",
            r.width(),
            r.height(),
            ctx.pixels_per_point(),
            self.diag_touches,
            self.diag_pointer
        );
        if line != self.diag {
            // solo cambios de geometría/escala al log (no cada toque)
            let geom = |s: &str| s.split(" · toques").next().unwrap_or("").to_owned();
            if geom(&line) != geom(&self.diag) {
                log_line(&line);
            }
            self.diag = line;
        }
    }

    // ---- pantallas ----

    fn ui_home(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.label(RichText::new("PepoMote").size(38.0).strong().color(theme::text()));
        ui.label(RichText::new(tr!("home.tagline")).size(14.0).color(theme::text_dim()));
        ui.add_space(6.0);
        let (dot, txt) = match self.link.as_ref().map(|l| l.status()) {
            Some(Status::Connected { pc_name, .. }) => (theme::ok(), tr!("home.connected", pc_name)),
            Some(Status::Connecting) => (theme::warn(), tr!("common.connecting").to_owned()),
            Some(Status::Reconnecting { pc_name, .. }) => (theme::warn(), tr!("home.reconnecting", pc_name)),
            _ => match &self.pairing {
                Some(p) => (theme::text_dim(), tr!("home.paired", p.pc_name)),
                None => (theme::text_dim(), tr!("home.unpaired").to_owned()),
            },
        };
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 5.0, dot);
            ui.label(RichText::new(txt).size(13.0).color(theme::text_dim()));
        });
        ui.add_space(18.0);

        // Aviso de versión nueva: tarjeta con el enlace a la release de GitHub
        if let Some(v) = crate::update::pending(
            &crate::update::Version::current(),
            self.settings.update_latest,
            self.settings.update_dismissed,
        ) {
            egui::Frame::none()
                .fill(theme::card())
                .stroke(egui::Stroke::new(1.5_f32, theme::blue()))
                .rounding(theme::RADIUS)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(tr!("upd.available", v)).size(15.0).strong().color(theme::text()));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.hyperlink_to(
                            RichText::new(tr!("upd.download")).size(14.0).color(theme::blue()),
                            crate::update::release_url(&v),
                        );
                        ui.add_space(8.0);
                        if ui.button(RichText::new(tr!("upd.dismiss")).size(13.0)).clicked() {
                            self.settings.update_dismissed = Some(v);
                            store::save_settings(&self.settings);
                        }
                    });
                });
            ui.add_space(12.0);
        }

        let w = ui.available_width();
        let half = Vec2::new((w - 12.0) / 2.0, 96.0);
        let card = |ui: &mut egui::Ui, size: Vec2, title: &str, sub: &str, accent: egui::Color32| -> bool {
            ui.add_sized(
                size,
                egui::Button::new(
                    RichText::new(format!("{title}\n{sub}")).size(16.0).color(theme::text()),
                )
                .fill(theme::card())
                .stroke(egui::Stroke::new(2.0_f32, accent)),
            )
            .clicked()
        };
        let mut go: Option<u8> = None;
        ui.horizontal(|ui| {
            if card(ui, half, tr!("home.card_connect"), tr!("home.card_connect_sub"), theme::blue()) {
                go = Some(0);
            }
            if card(ui, half, tr!("home.card_pad"), tr!("home.card_pad_sub"), theme::blue()) {
                go = Some(1);
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if card(ui, half, tr!("home.card_dolphin"), tr!("home.card_dolphin_sub"), theme::ok()) {
                go = Some(2);
            }
            if card(ui, half, tr!("home.card_nunchuk"), tr!("home.card_nunchuk_sub"), theme::ok()) {
                go = Some(4);
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if card(ui, half, tr!("home.card_wiiu"), tr!("home.card_wiiu_sub"), theme::ok()) {
                go = Some(5);
            }
            if card(ui, half, tr!("home.card_switch"), tr!("home.card_switch_sub"), theme::blue()) {
                go = Some(6);
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if card(ui, half, tr!("home.card_retroarch"), tr!("home.card_retroarch_sub"), theme::warn()) {
                go = Some(7);
            }
            if card(ui, half, tr!("home.card_pair"), tr!("home.card_pair_sub"), theme::text_dim()) {
                go = Some(3);
            }
        });
        match go {
            Some(0) => self.open_controller(Some("pointer"), false),
            Some(1) => self.open_controller(None, false),
            Some(2) => self.open_controller(Some("dolphin"), true),
            Some(7) => self.open_retroarch(),
            Some(3) => {
                self.after_pair_mode = None;
                self.discovered.clear();
                self.scan_rx = None; // lo que quedara en cola de un sondeo viejo no vale
                self.last_scan = None;
                self.pair_reason = None;
                self.screen = Screen::Pair;
            }
            Some(4) => self.open_nunchuk(),
            Some(5) => self.open_gamepad(),
            Some(6) => self.open_switch(),
            _ => {}
        }

        ui.add_space(16.0);
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).size(13.0).color(theme::error()));
            ui.add_space(6.0);
        }
        ui.label(RichText::new(tr!("home.sensors", self.sensor_desc)).size(11.0).color(theme::text_dim()));
        if ui
            .add(egui::Button::new(RichText::new(tr!("home.calibrate")).size(13.0).color(theme::text())).fill(theme::card()))
            .on_hover_text(tr!("home.calibrate_tip"))
            .clicked()
        {
            self.start_calibration();
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("home.theme")).size(12.0).color(theme::text_dim()));
            let mut pref = self.settings.theme;
            for p in theme::ThemePref::ALL {
                let label = match p {
                    theme::ThemePref::System => tr!("home.theme_system"),
                    theme::ThemePref::Light => tr!("home.theme_light"),
                    theme::ThemePref::Dark => tr!("home.theme_dark"),
                };
                if ui.selectable_label(pref == p, RichText::new(label).size(12.0)).clicked() {
                    pref = p;
                }
            }
            if pref != self.settings.theme {
                self.settings.theme = pref;
                store::save_settings(&self.settings);
                theme::set_preference(ui.ctx(), pref);
            }
        });
        if ui
            .checkbox(&mut self.settings.update_check, RichText::new(tr!("upd.toggle")).size(12.0))
            .on_hover_text(tr!("upd.toggle_help"))
            .changed()
        {
            store::save_settings(&self.settings);
        }
        // Avisos del receptor sobre el mando al cambiar de modo
        if ui
            .checkbox(&mut self.settings.receiver_notices, RichText::new(tr!("home.notices")).size(12.0))
            .on_hover_text(tr!("home.notices_help"))
            .changed()
        {
            store::save_settings(&self.settings);
        }
        // GamePad de Wii U sin pantalla táctil: botones más grandes
        if ui
            .checkbox(&mut self.settings.gamepad_no_screen, RichText::new(tr!("gp.no_screen")).size(12.0))
            .on_hover_text(tr!("gp.no_screen_help"))
            .changed()
        {
            if self.settings.gamepad_no_screen {
                self.settings.gamepad_full_screen = false;
            }
            store::save_settings(&self.settings);
        }
        // Pantalla del GamePad a pantalla completa (mando real en el PC), con
        // la subopción del botón de teclado; excluyente con «sin pantalla»
        if ui
            .checkbox(&mut self.settings.gamepad_full_screen, RichText::new(tr!("gp.full_screen")).size(12.0))
            .on_hover_text(tr!("gp.full_screen_help"))
            .changed()
        {
            if self.settings.gamepad_full_screen {
                self.settings.gamepad_no_screen = false;
            }
            store::save_settings(&self.settings);
            // con el enlace vivo se aplica ya (el receptor lo confirma con el eco)
            if let Some(l) = &self.link {
                l.send_screen_only(self.settings.gamepad_full_screen);
            }
        }
        ui.indent("gp_full_kb", |ui| {
            if ui
                .add_enabled(
                    self.settings.gamepad_full_screen,
                    egui::Checkbox::new(&mut self.settings.gamepad_full_screen_kb, RichText::new(tr!("gp.full_screen_kb")).size(12.0)),
                )
                .on_hover_text(tr!("gp.full_screen_kb_help"))
                .changed()
            {
                store::save_settings(&self.settings);
            }
        });
        // Modo puntero: el botón «Teclado» hace antes clic donde apuntas
        if ui
            .checkbox(&mut self.settings.keyboard_click_first, RichText::new(tr!("home.kb_click")).size(12.0))
            .on_hover_text(tr!("home.kb_click_help"))
            .changed()
        {
            store::save_settings(&self.settings);
        }
        // Cómo se pulsan los botones: deslizando (manda sobre el siguiente) o
        // pegajoso (lo de siempre). La subopción solo se elige con el primero
        // apagado; el valor guardado no se toca
        if ui
            .checkbox(&mut self.settings.slide_press, RichText::new(tr!("home.slide_press")).size(12.0))
            .on_hover_text(tr!("home.slide_press_help"))
            .changed()
        {
            store::save_settings(&self.settings);
        }
        ui.indent("sticky_press", |ui| {
            if ui
                .add_enabled(
                    !self.settings.slide_press,
                    egui::Checkbox::new(&mut self.settings.sticky_press, RichText::new(tr!("home.sticky_press")).size(12.0)),
                )
                .on_hover_text(tr!("home.sticky_press_help"))
                .changed()
            {
                store::save_settings(&self.settings);
            }
        });
        ui.label(RichText::new(&self.diag).size(11.0).color(theme::text_dim()));
        ui.label(
            RichText::new(tr!("home.version", env!("CARGO_PKG_VERSION")))
                .size(11.0)
                .color(theme::text_dim()),
        );
    }

    fn ui_pair(&mut self, ui: &mut egui::Ui) {
        self.poll_scan();
        ui.add_space(10.0);
        let title = if self.pair_reason.is_some() { tr!("pair.title_again") } else { tr!("pair.title") };
        ui.label(RichText::new(title).size(28.0).strong().color(theme::text()));
        ui.label(
            RichText::new(tr!("pair.subtitle"))
                .size(13.0)
                .color(theme::text_dim()),
        );
        if let Some(reason) = &self.pair_reason {
            ui.add_space(8.0);
            ui.label(RichText::new(reason).size(13.0).color(theme::error()));
        }
        // Tus PCs: tocar uno conecta con él; «Olvidar» lo quita
        let saved = store::load_all();
        if !saved.list.is_empty() {
            ui.add_space(12.0);
            ui.label(RichText::new(tr!("pair.your_pcs")).size(15.0).strong().color(theme::text()));
            ui.add_space(4.0);
            let mut choose: Option<Pairing> = None;
            let mut forget: Option<String> = None;
            for p in &saved.list {
                let current = saved.current.as_deref() == Some(p.token.as_str());
                // El PC de la sesión abierta está en la red seguro: sin esperar
                // al sondeo (por nombre además de actual: olvidado el PC del
                // enlace, el actual pasa a ser otro, y ese no tiene sesión)
                let online = (current && self.link_connected_to(&p.pc_name))
                    || self.discovered.iter().any(|r| r.name == p.pc_name || r.host == p.host);
                ui.horizontal(|ui| {
                    let w = ui.available_width() - 84.0;
                    let label = format!(
                        "{}{}\n{}:{}{}",
                        p.pc_name,
                        if current { tr!("pair.current") } else { "" },
                        p.host,
                        p.port,
                        if online { tr!("pair.online") } else { "" }
                    );
                    let mut b = egui::Button::new(RichText::new(label).size(14.0).color(theme::text())).fill(theme::card());
                    if current {
                        b = b.stroke(egui::Stroke::new(1.5_f32, theme::blue()));
                    }
                    if ui.add_sized(Vec2::new(w, 56.0), b).clicked() {
                        choose = Some(p.clone());
                    }
                    let f = egui::Button::new(RichText::new(tr!("pair.forget")).size(12.0).color(theme::error())).fill(theme::card());
                    if ui.add_sized(Vec2::new(74.0, 56.0), f).clicked() {
                        forget = Some(p.token.clone());
                    }
                });
            }
            if let Some(p) = choose {
                self.pairing = store::select(&p.token);
                self.pair_reason = None;
                self.resume_after_pairing();
            }
            if let Some(t) = forget {
                self.pairing = store::forget(&t);
            }
        }
        ui.add_space(12.0);
        let msg = if self.discovered.is_empty() {
            if self.scan_rx.is_some() {
                tr!("pair.scanning")
            } else {
                tr!("pair.none")
            }
        } else {
            tr!("pair.on_network")
        };
        ui.label(RichText::new(msg).size(13.0).color(theme::text_dim()));
        ui.add_space(6.0);
        let mut chosen: Option<Receiver> = None;
        let known = |r: &Receiver| saved.list.iter().any(|p| p.pc_name == r.name || p.host == r.host);
        for r in self.discovered.iter().filter(|r| !known(r)) {
            if ui
                .add_sized(
                    Vec2::new(ui.available_width(), 64.0),
                    egui::Button::new(
                        RichText::new(format!("{}\n{}:{}", r.name, r.host, r.port)).size(15.0).color(theme::text()),
                    )
                    .fill(theme::card()),
                )
                .clicked()
            {
                chosen = Some(r.clone());
            }
        }
        if let Some(r) = chosen {
            self.target = Some(r);
            self.code.clear();
            self.pair_error = None;
            self.screen = Screen::Code;
        }
        ui.add_space(14.0);
        if ui
            .add_sized(Vec2::new(ui.available_width(), 48.0), egui::Button::new(RichText::new(tr!("pair.manual_ip")).size(15.0)))
            .clicked()
        {
            self.manual.clear();
            self.screen = Screen::Manual;
        }
        ui.add_space(8.0);
        if ui.button(RichText::new(tr!("common.back")).size(14.0).color(theme::text_dim())).clicked() {
            self.pair_reason = None;
            self.screen = Screen::Home;
        }
    }

    fn ui_manual(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.label(RichText::new(tr!("manual.title")).size(26.0).strong().color(theme::text()));
        ui.label(RichText::new(tr!("manual.subtitle")).size(13.0).color(theme::text_dim()));
        ui.add_space(10.0);
        let shown = if self.manual.is_empty() { "…".to_owned() } else { self.manual.clone() };
        ui.label(RichText::new(shown).size(30.0).strong().color(theme::text()));
        ui.add_space(10.0);
        match keypad(ui, &['.', ':'], true) {
            Some(Key::Char(c)) => {
                if self.manual.len() < 21 {
                    self.manual.push(c);
                }
            }
            Some(Key::Backspace) => {
                self.manual.pop();
            }
            Some(Key::Ok) => {
                let (host, port) = store::split_host_port(&self.manual);
                if !host.is_empty() {
                    self.target = Some(Receiver {
                        name: host.clone(),
                        host,
                        port,
                    });
                    self.code.clear();
                    self.pair_error = None;
                    self.screen = Screen::Code;
                }
            }
            // el numérico no tiene Shift
            Some(Key::Shift) | None => {}
        }
        ui.add_space(8.0);
        if ui.button(RichText::new(tr!("common.back")).size(14.0).color(theme::text_dim())).clicked() {
            self.screen = Screen::Pair;
        }
    }

    fn ui_code(&mut self, ui: &mut egui::Ui) {
        self.poll_pairing();
        let name = self.target.as_ref().map(|t| t.name.clone()).unwrap_or_default();
        ui.add_space(10.0);
        ui.label(RichText::new(tr!("code.title")).size(26.0).strong().color(theme::text()));
        ui.label(
            RichText::new(tr!("code.subtitle", name))
                .size(13.0)
                .color(theme::text_dim()),
        );
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 4.0 * 58.0) / 2.0);
            for i in 0..4 {
                let (r, _) = ui.allocate_exact_size(Vec2::new(50.0, 60.0), egui::Sense::hover());
                ui.painter().rect(r, egui::Rounding::same(12.0), theme::card(), egui::Stroke::new(1.5_f32, theme::card_border()));
                if let Some(c) = self.code.chars().nth(i) {
                    ui.painter().text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        c,
                        egui::FontId::proportional(30.0),
                        theme::text(),
                    );
                }
            }
        });
        ui.add_space(10.0);
        if self.pair_rx.is_some() {
            ui.label(RichText::new(tr!("code.pairing")).size(14.0).color(theme::blue()));
        } else if let Some(e) = &self.pair_error {
            ui.label(RichText::new(e).size(13.0).color(theme::error()));
        }
        ui.add_space(8.0);
        if self.pair_rx.is_none() {
            match keypad(ui, &[], false) {
                Some(Key::Char(c)) if self.code.len() < 4 => {
                    self.code.push(c);
                    if self.code.len() == 4 {
                        self.start_pairing();
                    }
                }
                Some(Key::Backspace) => {
                    self.code.pop();
                }
                _ => {}
            }
        }
        ui.add_space(8.0);
        if ui.button(RichText::new(tr!("common.back")).size(14.0).color(theme::text_dim())).clicked() {
            self.pair_rx = None;
            self.screen = Screen::Pair;
        }
    }

    /// Los dos ajustes de pulsación tal como los ven las tres pantallas.
    fn press(&self) -> Press {
        Press { slide: self.settings.slide_press, sticky: self.settings.sticky_press }
    }

    fn ui_controller(&mut self, ui: &mut egui::Ui) {
        let Some(link) = &self.link else {
            ui.add_space(10.0);
            ui.label(RichText::new(tr!("common.no_connection")).size(20.0).strong().color(theme::text()));
            ui.add_space(8.0);
            if self.pairing.is_some() && ui.button(RichText::new(tr!("common.reconnect")).size(15.0).color(theme::blue())).clicked() {
                self.open_controller(Some("pointer"), self.dolphin_only);
            }
            if ui.button(RichText::new(tr!("common.back")).size(14.0).color(theme::text_dim())).clicked() {
                self.screen = Screen::Home;
            }
            return;
        };
        let status = link.status();
        let slot0 = matches!(&status, Status::Connected { slot: 0, .. });
        let hz = link.sensor_hz();
        let pending = self.pad_pending.map(|p| p.pad);
        let press = self.press();
        let layout_name = pmp::retro::layout(self.effective_retro_layout()).map_or("RetroPad", |l| l.name);
        match self.controller.show(ui, &self.buttons, &status, !self.dolphin_only && slot0, pending, hz, press, layout_name) {
            Action::Exit => {
                self.close_link();
                self.screen = Screen::Home;
            }
            Action::Mode(m) => self.request_mode(m),
            Action::Pad(p) => {
                let mode = self.session_mode().unwrap_or_default();
                let mode: &str = if mode == "retroarch" { "retroarch" } else { "cemu" };
                self.request_pad(mode, p);
            }
            Action::Keyboard => self.open_text_dialog(),
            Action::LayoutPicker => self.open_layout_picker(),
            Action::Hotkey(n) => self.send_hotkey(n, true),
            Action::Hold(n, down) => self.send_hotkey(n, down),
            Action::None => {}
        }
    }

    fn ui_gamepad(&mut self, ui: &mut egui::Ui) {
        let Some(link) = &self.link else {
            ui.add_space(10.0);
            ui.label(RichText::new(tr!("common.no_connection")).size(20.0).strong().color(theme::text()));
            ui.add_space(8.0);
            if self.pairing.is_some() && ui.button(RichText::new(tr!("common.reconnect")).size(15.0).color(theme::blue())).clicked() {
                match self.extended_mode { "switch" => self.open_switch(), "retroarch" => self.open_retroarch(), _ => self.open_gamepad() }
            }
            if ui.button(RichText::new(tr!("common.back")).size(14.0).color(theme::text_dim())).clicked() {
                self.screen = Screen::Home;
            }
            return;
        };
        let status = link.status();
        let slot0 = matches!(&status, Status::Connected { slot: 0, .. });
        let wanted_mode: &'static str = self.intent.wanted().unwrap_or_else(|| {
            match &status {
                Status::Connected { mode, .. } if mode == "switch" => "switch",
                Status::Connected { mode, .. } if mode == "cemu" => "cemu",
                Status::Connected { mode, .. } if mode == "retroarch" => "retroarch",
                _ => self.extended_mode,
            }
        });
        // RetroArch: la plantilla de consola que toca y el juego cargado
        let game = if wanted_mode == "retroarch" { self.retro_game() } else { None };
        let layout_id = if wanted_mode == "retroarch" { self.effective_retro_layout() } else { "retropad" };
        let layout = pmp::retro::layout(layout_id).unwrap_or(&pmp::retro::RETROPAD);
        let inputs = GamePadInputs {
            status: &status,
            rotation: self.settings.rotation,
            show_chips: !self.dolphin_only && slot0,
            optimistic: self.intent != Intent::None,
            wanted_mode,
            pad_pending: self.pad_pending.map(|p| p.pad),
            sensor_hz: link.sensor_hz(),
            screen: self.pad_screen.as_ref(),
            no_screen: self.settings.gamepad_no_screen,
            full_screen: self.settings.gamepad_full_screen,
            keyboard_button: self.settings.gamepad_full_screen_kb,
            press: self.press(),
            layout,
            game: game.as_ref().map(|g| g.title.as_str()).filter(|t| !t.is_empty()),
        };
        match self.gamepad.show(ui, &self.buttons, &inputs) {
            GamePadAction::Exit => {
                self.close_link();
                self.screen = Screen::Home;
            }
            GamePadAction::Mode(m) => self.request_mode(m),
            GamePadAction::Pad(p) => {
                self.request_pad(wanted_mode, p);
            }
            GamePadAction::Rotation(r) => {
                self.settings.rotation = r;
                store::save_settings(&self.settings);
                self.buttons.set_rotation(r.as_u8());
                log_line(&format!("giro apaisado: borde superior a la {}", r.label()));
            }
            GamePadAction::Keyboard => self.open_text_dialog(),
            GamePadAction::LayoutPicker => self.open_layout_picker(),
            GamePadAction::Hotkey(n) => self.send_hotkey(n, true),
            GamePadAction::None => {}
        }
        self.sync_hotkey_holds();
    }

    fn ui_nunchuk(&mut self, ui: &mut egui::Ui) {
        let Some(link) = &self.link else {
            ui.add_space(10.0);
            ui.label(RichText::new(tr!("common.no_connection")).size(20.0).strong().color(theme::text()));
            ui.add_space(8.0);
            if self.pairing.is_some() && ui.button(RichText::new(tr!("common.reconnect")).size(15.0).color(theme::blue())).clicked() {
                self.open_nunchuk();
            }
            if ui.button(RichText::new(tr!("common.back")).size(14.0).color(theme::text_dim())).clicked() {
                self.screen = Screen::Home;
            }
            return;
        };
        let status = link.status();
        let hz = link.sensor_hz();
        let press = self.press();
        if let NunchukAction::Exit = self.nunchuk.show(ui, &self.buttons, &status, hz, press) {
            self.close_link();
            self.screen = Screen::Home;
        }
    }
}

impl MobileApp {
    fn start_calibration(&mut self) {
        self.close_link();
        match sensor::open(self.fake) {
            Ok(src) => {
                let (tx, rx) = mpsc::channel::<sensor::Sample>();
                let stop = Arc::new(AtomicBool::new(false));
                let stop2 = stop.clone();
                let _ = std::thread::Builder::new()
                    .name("pepomote-calib".into())
                    .spawn(move || src.run(tx, stop2));
                self.calib = Some(Calib {
                    rx,
                    stop,
                    step: 0,
                    capture: None,
                    accel: [None; 3],
                    gyro: [None; 3],
                    msg: None,
                    done: None,
                    last: None,
                });
                self.error = None;
                self.screen = Screen::Calibrate;
            }
            Err(e) => {
                self.error = Some(tr!("cal.open_err", e));
                self.screen = Screen::Home;
            }
        }
    }

    fn stop_calibration(&mut self) {
        if let Some(c) = self.calib.take() {
            c.stop.store(true, Ordering::Relaxed);
        }
        self.sensor_desc = describe_sensors(self.fake);
        self.screen = Screen::Home;
    }

    fn ui_calibrate(&mut self, ui: &mut egui::Ui) {
        let Some(c) = self.calib.as_mut() else {
            self.screen = Screen::Home;
            return;
        };
        while let Ok(s) = c.rx.try_recv() {
            c.last = Some(s);
            if let Some((_, _, buf)) = c.capture.as_mut() {
                buf.push(s);
            }
        }
        if c.capture.as_ref().is_some_and(|(start, dur, _)| start.elapsed() >= *dur) {
            let (_, _, buf) = c.capture.take().unwrap();
            let step = &calib::steps()[c.step];
            let res = match step.kind {
                calib::Kind::Pose => calib::pose_sign(calib::mean_accel(&buf), step.axis),
                calib::Kind::Motion => calib::motion_sign(calib::integrate_deg(&buf), step.axis),
            };
            match res {
                Ok(sign) => {
                    match step.kind {
                        calib::Kind::Pose => c.accel[step.axis] = Some(sign),
                        calib::Kind::Motion => c.gyro[step.axis] = Some(sign),
                    }
                    c.msg = None;
                    c.step += 1;
                    if c.step >= calib::STEP_COUNT {
                        let axes = Axes {
                            accel: c.accel.map(|s| s.unwrap_or(1)),
                            gyro: c.gyro.map(|s| s.unwrap_or(1)),
                        };
                        store::save_axes(&axes);
                        log_line(&format!("calibración de ejes guardada: {}", axes.describe()));
                        c.done = Some(axes);
                    }
                }
                Err(e) => c.msg = Some(e),
            }
        }

        let mut leave = false;
        ui.add_space(8.0);
        ui.label(RichText::new(tr!("cal.title")).size(26.0).strong().color(theme::text()));
        ui.label(
            RichText::new(tr!("cal.subtitle"))
                .size(12.0)
                .color(theme::text_dim()),
        );
        if let Some(axes) = c.done {
            ui.add_space(12.0);
            ui.label(RichText::new(tr!("cal.done")).size(16.0).color(theme::text()));
            ui.label(RichText::new(axes.describe()).size(20.0).strong().color(theme::ok()));
            ui.label(
                RichText::new(if axes.is_identity() { tr!("cal.identity") } else { tr!("cal.inverted") })
                .size(13.0)
                .color(theme::text_dim()),
            );
            ui.add_space(16.0);
            if ui
                .add_sized(Vec2::new(ui.available_width(), 56.0), egui::Button::new(RichText::new(tr!("common.back")).size(18.0)).fill(theme::blue()))
                .clicked()
            {
                leave = true;
            }
        } else {
            let step = &calib::steps()[c.step];
            ui.label(RichText::new(tr!("cal.step_of", c.step + 1, calib::STEP_COUNT)).size(13.0).color(theme::text_dim()));
            ui.add_space(10.0);
            ui.label(RichText::new(step.title).size(20.0).strong().color(theme::text()));
            ui.add_space(6.0);
            ui.label(RichText::new(step.text).size(16.0).color(theme::text()));
            ui.add_space(14.0);
            let left = c
                .capture
                .as_ref()
                .map(|(s, d, _)| (d.as_secs_f32() - s.elapsed().as_secs_f32()).max(0.0));
            let label = match left {
                Some(t) => tr!(
                    "cal.countdown",
                    if step.kind == calib::Kind::Motion { tr!("cal.go") } else { tr!("cal.measuring") },
                    format!("{t:.1}")
                ),
                None => tr!("cal.ready").to_owned(),
            };
            let btn = ui.add_enabled(
                left.is_none() && c.last.is_some(),
                egui::Button::new(RichText::new(label).size(20.0).strong().color(theme::text()))
                    .min_size(Vec2::new(ui.available_width(), 64.0))
                    .fill(theme::blue()),
            );
            if btn.clicked() {
                let dur = match step.kind {
                    calib::Kind::Pose => Duration::from_millis(800),
                    calib::Kind::Motion => Duration::from_millis(2200),
                };
                c.capture = Some((Instant::now(), dur, Vec::new()));
                c.msg = None;
            }
            if let Some(m) = &c.msg {
                ui.add_space(8.0);
                ui.label(RichText::new(m).size(14.0).color(theme::error()));
            }
            ui.add_space(12.0);
            match c.last {
                Some(s) => ui.label(
                    RichText::new(format!(
                        "accel [{:.1} {:.1} {:.1}] m/s²   gyro [{:.2} {:.2} {:.2}] rad/s",
                        s.accel[0], s.accel[1], s.accel[2], s.gyro[0], s.gyro[1], s.gyro[2]
                    ))
                    .size(11.0)
                    .color(theme::text_dim()),
                ),
                None => ui.label(RichText::new(tr!("cal.waiting")).size(12.0).color(theme::warn())),
            };
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new(tr!("cal.cancel")).size(15.0)).clicked() {
                    leave = true;
                }
                if ui.button(RichText::new(tr!("cal.clear")).size(15.0)).clicked() {
                    store::clear_axes();
                    leave = true;
                }
            });
        }
        if leave {
            self.stop_calibration();
        }
    }
}

impl eframe::App for MobileApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::sync(ctx);
        ctx.request_repaint_after(Duration::from_millis(if self.screen == Screen::Calibrate { 30 } else { 100 }));
        self.poll_link();
        // La pose congelada se suelta SOLA: al cerrar el teclado, al cambiar
        // de modo (el PC lo cambia por su cuenta al abrir un emulador), al
        // reconectar o al salir. Una congelada olvidada dejaría el puntero
        // muerto.
        let hold = self.text_dialog.is_some()
            && self.link.as_ref().is_some_and(|l| l.status().holds_pointer());
        self.buttons.set_pointer_hold(hold);
        self.update_diag(ctx);
        if let Some((t, v)) = UPDATE_RESULT.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.settings.update_last_check = t;
            self.settings.update_latest = Some(v);
            store::save_settings(&self.settings);
        }

        // El hilo de paquetes manda 80 bytes (y remapea los sensores) solo
        // mientras se juega en la pantalla GamePad con Wii U CONFIRMADO por el
        // receptor (conectando u optimista: 72 bytes como siempre, y los
        // controles están inertes); al dejar de jugar, todo suelto
        let on_gamepad = self.screen == Screen::GamePad;
        let gamepad = on_gamepad && self.ext_confirmed();
        let status = self.link.as_ref().map(|l| l.status());
        // Switch y el RetroPad de RetroArch van en el mismo paquete (sin táctil ni micrófono)
        let switch = matches!(&status, Some(Status::Connected { mode, .. }) if mode == "switch" || mode == "retroarch");
        self.buttons.set_switch(switch);
        if !on_gamepad && !self.hotkey_holds.is_empty() {
            self.release_hotkeys();
        }
        self.buttons.set_rotation(self.settings.rotation.as_u8());
        if gamepad != self.buttons.is_gamepad() {
            self.buttons.set_gamepad(gamepad);
            if !gamepad {
                self.buttons.release_all();
            }
        }
        // Doble pantalla: el canal de la pantalla del GamePad solo mientras se
        // juega aquí como GamePad (un Pro Controller no tiene pantalla) y sin
        // el ajuste «GamePad sin pantalla táctil»
        let want_screen = gamepad && self.pad_is_gamepad() && !self.settings.gamepad_no_screen;
        // Pantalla completa: solo la pantalla de Cemu y el táctil (el mando
        // real va en el PC); el área entera se pide al receptor
        let full = want_screen && self.settings.gamepad_full_screen;
        self.sync_screen(ctx, want_screen, full);
        // Receptor anterior a 1.6: no confirma «solo pantalla» ni en el ok
        // ni con el eco; se avisa una vez (a los 2 s, por si el eco llega tarde)
        if full {
            let since = *self.full_screen_since.get_or_insert_with(Instant::now);
            if !self.full_screen_warned && since.elapsed() > Duration::from_secs(2) {
                self.full_screen_warned = true;
                if let Some(l) = &self.link {
                    if matches!(l.status(), Status::Connected { screen_only: None, .. }) {
                        l.notify(&tr!("gp.full_screen_old_pc"));
                    }
                }
            }
        } else {
            self.full_screen_since = None;
        }

        // El teclado para Cemu tapa la pantalla de juego sin cambiarla (los
        // INPUT siguen saliendo, el giro no cambia); se va solo si se pierde
        // el enlace, la pantalla de juego o el modo Wii U
        let text_open = self.text_dialog.is_some()
            && matches!(self.screen, Screen::Controller | Screen::GamePad)
            && self.mode_has_keyboard();
        if !text_open {
            self.text_dialog = None;
        }
        // El selector de mando de consola, igual: solo en RetroArch y en una pantalla de juego
        let picker_open = self.layout_picker
            && matches!(self.screen, Screen::Controller | Screen::GamePad)
            && self.session_mode().as_deref() == Some("retroarch");
        if !picker_open {
            self.layout_picker = false;
        }
        let text_open = text_open || picker_open;

        // Idioma: ES / EN arriba a la derecha del inicio (se guarda en settings.json)
        if self.screen == Screen::Home {
            egui::Area::new(egui::Id::new("lang"))
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 14.0))
                .show(ctx, |ui| {
                    let lang = i18n::current();
                    let button = egui::Button::new(RichText::new(lang.code().to_uppercase()).size(12.0).color(theme::text_dim()))
                        .fill(theme::card())
                        .stroke(egui::Stroke::new(1.0_f32, theme::card_border()));
                    if ui.add(button).on_hover_text(tr!("home.lang_switch", lang.other().name())).clicked() {
                        let next = i18n::toggle();
                        self.settings.lang = store::LangPref::from_lang(next);
                        store::save_settings(&self.settings);
                    }
                });
        }

        // en pantalla completa, sin margen y sobre negro
        let full_now = full && !text_open;
        egui::CentralPanel::default()
            // el GamePad se pinta a mano y aprovecha hasta el borde
            .frame(
                egui::Frame::default()
                    .fill(if full_now { egui::Color32::BLACK } else { theme::background() })
                    .inner_margin(if full_now { 0.0 } else if on_gamepad && !text_open { 6.0 } else { 16.0 }),
            )
            .show(ctx, |ui| {
                if picker_open {
                    return self.ui_layout_picker(ui);
                }
                if text_open {
                    return self.ui_text(ui);
                }
                match self.screen {
                    Screen::Home => { egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| self.ui_home(ui)); },
                    Screen::Pair => self.ui_pair(ui),
                    Screen::Manual => self.ui_manual(ui),
                    Screen::Code => self.ui_code(ui),
                    Screen::Controller => self.ui_controller(ui),
                    Screen::Nunchuk => self.ui_nunchuk(ui),
                    Screen::GamePad => self.ui_gamepad(ui),
                    Screen::Calibrate => self.ui_calibrate(ui),
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_intent_waits_for_its_own_mode_and_capability() {
        let now = Instant::now();
        let intent = Intent::Switch { seq: 0, since: now };
        assert_eq!(intent.wanted(), Some("switch"));
        assert_eq!(route(&Status::Connecting, Role::Wiimote, intent), Screen::GamePad);
        assert_eq!(settle(&Status::Connecting, intent, now), (intent, None));
        let mut status = conn("cemu", Role::Wiimote, "gamepad", 0, true, 0);
        assert_eq!(settle(&status, intent, now), (intent, None), "Cemu does not fulfil a Switch request");
        assert_eq!(settle(&status, intent, now + INTENT_TIMEOUT + Duration::from_millis(1)), (Intent::None, Some(tr!("notice.old_pc_switch"))));
        if let Status::Connected { supports_switch, .. } = &mut status { *supports_switch = false; }
        assert_eq!(settle(&status, intent, now), (Intent::None, Some(tr!("notice.old_pc_switch"))));
        assert_eq!(settle(&conn("pointer", Role::Wiimote, "pro", 1, true, 1), intent, now), (Intent::None, Some(notice_only_p1())));
        let mut confirmed = conn("switch", Role::Wiimote, "pro", 0, true, 1);
        assert_eq!(settle(&confirmed, intent, now), (Intent::None, None));
        if let Status::Connected { mode, mode_by_pc, .. } = &mut confirmed { *mode = "dolphin".into(); *mode_by_pc = true; }
        assert_eq!(settle(&confirmed, intent, now), (Intent::None, None), "the PC's automatic mode wins without a second warning");
    }

    #[test]
    fn pad_choices_do_not_leak_between_emulators() {
        let mut choices = RequestedPads::default();
        choices.set("cemu", "wiimote");
        choices.set("switch", "joycon_r");
        assert_eq!(choices.get("cemu"), Some("wiimote"));
        assert_eq!(choices.get("switch"), Some("pro"));
        choices.set("switch", "gamepad");
        choices.set("cemu", "joycons");
        assert_eq!(choices.get("switch"), Some("pro"));
        assert_eq!(choices.get("cemu"), Some("wiimote"));
        assert_eq!(choices.get("dolphin"), None);
        // RetroArch: sus tres mandos y nada más
        assert_eq!(choices.get("retroarch"), Some("retropad"));
        choices.set("retroarch", "gun");
        assert_eq!(choices.get("retroarch"), Some("gun"));
        choices.set("retroarch", "wiimote");
        assert_eq!(choices.get("retroarch"), Some("gun"));
        assert_eq!(choices.get("cemu"), Some("wiimote"));
    }

    #[test]
    fn retroarch_intent_routes_and_settles_like_the_others() {
        let now = Instant::now();
        let intent = Intent::for_mode("retroarch", 0, now);
        assert_eq!(intent.wanted(), Some("retroarch"));
        assert_eq!(Intent::for_mode("dolphin", 0, now), Intent::None);
        assert_eq!(route(&Status::Connecting, Role::Wiimote, intent), Screen::GamePad);
        assert_eq!(settle(&Status::Connecting, intent, now), (intent, None));
        // solo el RetroPad es el apaisado; el mando de NES y la pistola, el mando vertical
        assert_eq!(route(&conn("retroarch", Role::Wiimote, "retropad", 0, true, 1), Role::Wiimote, Intent::None), Screen::GamePad);
        assert_eq!(route(&conn("retroarch", Role::Wiimote, "nes", 0, true, 1), Role::Wiimote, Intent::None), Screen::Controller);
        assert_eq!(route(&conn("retroarch", Role::Wiimote, "gun", 1, true, 1), Role::Wiimote, Intent::None), Screen::Controller);
        assert_eq!(route(&conn("retroarch", Role::Nunchuk, "retropad", 1, true, 1), Role::Nunchuk, Intent::None), Screen::Nunchuk);
        // el eco de RetroArch cierra la intención; otro modo o un PC viejo, aviso
        assert_eq!(settle(&conn("retroarch", Role::Wiimote, "retropad", 0, true, 1), intent, now), (Intent::None, None));
        let mut old = conn("pointer", Role::Wiimote, "gamepad", 0, true, 1);
        if let Status::Connected { supports_retroarch, .. } = &mut old { *supports_retroarch = false; }
        assert_eq!(settle(&old, intent, now), (Intent::None, Some(tr!("notice.old_pc_retroarch"))));
        assert_eq!(settle(&conn("pointer", Role::Wiimote, "gamepad", 1, true, 1), intent, now), (Intent::None, Some(notice_only_p1())));
        let switch = conn("switch", Role::Wiimote, "pro", 0, true, 0);
        assert_eq!(settle(&switch, intent, now), (intent, None), "Switch no responde a una petición de RetroArch");
    }

    #[test]
    fn android_restaura_retropad_sin_borrar_la_pistola_del_pc() {
        let mut choices = RequestedPads::default();
        choices.set("retroarch", "gun");
        let android = link::ReceiverCapabilities::from_ok(&serde_json::json!({"platform":"android"}));
        assert_eq!(choices.for_receiver("retroarch", &android), Some("retropad"));
        assert_eq!(choices.get("retroarch"), Some("gun"), "el fallback no es una elección del usuario");
        assert_eq!(choices.for_receiver("switch", &android), Some("pro"));
        assert_eq!(choices.for_receiver("retroarch", &link::ReceiverCapabilities::default()), Some("gun"));
        let mut st = conn("retroarch", Role::Wiimote, "gun", 0, true, 1);
        if let Status::Connected { receiver, .. } = &mut st { *receiver = android; }
        let now = Instant::now();
        let req = Some(PadRequest { pad: "gun", seq: 0, since: now });
        assert_eq!(settle_pad(&st, req, now), None, "no queda petición imposible pendiente");
        assert_eq!(route(&st, Role::Wiimote, Intent::None), Screen::GamePad);
        assert_eq!(settle(&st, Intent::for_mode("retroarch", 0, now), now), (Intent::None, None));
    }

    #[test]
    fn switch_routes_every_controller_kind_to_gamepad() {
        for pad in ["pro", "joycons", "joycon_side", "joycon_r"] {
            let status = conn("switch", Role::Wiimote, pad, 0, true, 1);
            assert_eq!(route(&status, Role::Wiimote, Intent::None), Screen::GamePad, "{pad}");
        }
        assert_eq!(route(&conn("switch", Role::Nunchuk, "pro", 1, true, 1), Role::Nunchuk, Intent::None), Screen::Nunchuk);
    }

    fn conn(mode: &str, role: Role, pad: &str, slot: u8, supports_cemu: bool, mode_seq: u32) -> Status {
        Status::Connected {
            pc_name: "PC".into(),
            mode: mode.into(),
            mode_by_pc: false,
            slot,
            player: slot + 1,
            role,
            rtt_ms: None,
            supports_cemu,
            supports_switch: true,
            supports_retroarch: true,
            receiver: link::ReceiverCapabilities::default(),
            pad: pad.into(),
            notice: None,
            mode_seq,
            pad_seq: 0,
            own_nunchuk: false,
            screen_only: None,
            game: None,
        }
    }

    fn wiiu(now: Instant) -> Intent {
        Intent::WiiU { seq: 0, since: now }
    }

    #[test]
    fn un_token_rechazado_manda_a_emparejar() {
        assert!(needs_new_pairing("bad_token"));
        assert!(needs_new_pairing("bad_code"));
        for code in ["io", "busy", "bad_version", ""] {
            assert!(!needs_new_pairing(code), "{code}");
        }
        assert!(re_pair_reason(Some("SALON-PC")).starts_with("SALON-PC ya no reconoce este móvil"));
        assert!(re_pair_reason(Some(" SALON-PC ")).starts_with("SALON-PC ya no reconoce"));
        assert!(re_pair_reason(None).starts_with("Tu PC ya no reconoce"));
        assert!(re_pair_reason(Some("   ")).starts_with("Tu PC ya no reconoce"));
    }

    #[test]
    fn enrutado_del_addendum() {
        let now = Instant::now();
        // 1. Wii U confirmado como mando: GamePad (J1) o Pro (J2), sin intención
        assert_eq!(route(&conn("cemu", Role::Wiimote, "gamepad", 0, true, 1), Role::Wiimote, Intent::None), Screen::GamePad);
        assert_eq!(route(&conn("cemu", Role::Wiimote, "pro", 1, true, 1), Role::Wiimote, Intent::None), Screen::GamePad);
        // Mando de Wii dentro de Wii U: layout Wii
        assert_eq!(route(&conn("cemu", Role::Wiimote, "wiimote", 0, true, 1), Role::Wiimote, Intent::None), Screen::Controller);
        // 2. intención pendiente: GamePad optimista, conectando o conectados en otro modo
        assert_eq!(route(&Status::Connecting, Role::Wiimote, wiiu(now)), Screen::GamePad);
        assert_eq!(route(&conn("pointer", Role::Wiimote, "gamepad", 0, true, 0), Role::Wiimote, wiiu(now)), Screen::GamePad);
        assert_eq!(route(&conn("dolphin", Role::Wiimote, "gamepad", 0, false, 0), Role::Wiimote, wiiu(now)), Screen::GamePad);
        // 3. Nunchuk: su pantalla, esté el modo que esté
        assert_eq!(route(&conn("cemu", Role::Nunchuk, "pro", 3, true, 1), Role::Nunchuk, Intent::None), Screen::Nunchuk);
        assert_eq!(route(&conn("pointer", Role::Nunchuk, "pro", 3, true, 0), Role::Nunchuk, Intent::None), Screen::Nunchuk);
        assert_eq!(route(&Status::Connecting, Role::Nunchuk, Intent::None), Screen::Nunchuk);
        // 4. lo demás: layout Wii
        assert_eq!(route(&conn("pointer", Role::Wiimote, "gamepad", 0, true, 0), Role::Wiimote, Intent::None), Screen::Controller);
        assert_eq!(route(&conn("dolphin", Role::Wiimote, "gamepad", 1, false, 0), Role::Wiimote, Intent::None), Screen::Controller);
        assert_eq!(route(&Status::Connecting, Role::Wiimote, Intent::None), Screen::Controller);
        // un receptor antiguo degrada al Nunchuk a mando: manda lo que dice el ok
        assert_eq!(route(&conn("pointer", Role::Wiimote, "gamepad", 1, false, 0), Role::Nunchuk, Intent::None), Screen::Controller);
    }

    #[test]
    fn eco_pointer_con_intencion_pendiente_vuelve_a_wii_con_aviso() {
        let now = Instant::now();
        let intent = wiiu(now);
        // el ok aún no es respuesta (mode_seq 0): se sigue esperando en el GamePad
        let st = conn("pointer", Role::Wiimote, "gamepad", 0, true, 0);
        assert_eq!(settle(&st, intent, now), (intent, None));
        assert_eq!(route(&st, Role::Wiimote, intent), Screen::GamePad);
        // eco posterior que no es cemu: se resuelve, aviso de PC antiguo, layout Wii
        let st = conn("pointer", Role::Wiimote, "gamepad", 0, true, 1);
        let (after, notice) = settle(&st, intent, now);
        assert_eq!(after, Intent::None);
        assert_eq!(notice, Some(notice_old_pc()));
        assert_eq!(route(&st, Role::Wiimote, after), Screen::Controller);
        // jugador 2 con un receptor que sí sabe de Wii U: solo el jugador 1 cambia el modo
        let st = conn("pointer", Role::Wiimote, "pro", 1, true, 1);
        assert_eq!(settle(&st, intent, now), (Intent::None, Some(notice_only_p1())));
        // jugador 2 con receptor antiguo: el aviso es el de la versión
        let st = conn("pointer", Role::Wiimote, "pro", 1, false, 1);
        assert_eq!(settle(&st, intent, now), (Intent::None, Some(notice_old_pc())));
    }

    #[test]
    fn la_difusion_del_pc_resuelve_la_intencion_sin_aviso() {
        let now = Instant::now();
        let intent = wiiu(now);
        let by_pc = |mut st: Status| {
            if let Status::Connected { mode_by_pc, .. } = &mut st {
                *mode_by_pc = true;
            }
            st
        };
        // el PC abrió Dolphin (modo automático) mientras se esperaba Wii U: ni
        // «PC antiguo» ni «solo el Jugador 1», su aviso ya lo explica
        let st = by_pc(conn("dolphin", Role::Wiimote, "pro", 1, true, 1));
        assert_eq!(settle(&st, intent, now), (Intent::None, None));
        assert_eq!(route(&st, Role::Wiimote, Intent::None), Screen::Controller);
        // un `mode` del PC anterior a la petición no la resuelve: se sigue esperando
        let st = by_pc(conn("pointer", Role::Wiimote, "gamepad", 0, true, 0));
        assert_eq!(settle(&st, intent, now), (intent, None));
    }

    #[test]
    fn la_intencion_se_resuelve_con_cemu_sin_aviso_y_espera_mientras_conecta() {
        let now = Instant::now();
        let intent = wiiu(now);
        assert_eq!(settle(&Status::Connecting, intent, now), (intent, None), "conectando: se espera");
        assert_eq!(settle(&Status::Disconnected, intent, now), (intent, None));
        let st = conn("cemu", Role::Wiimote, "gamepad", 0, true, 1);
        assert_eq!(settle(&st, intent, now), (Intent::None, None), "eco cemu: confirmado");
        // ya estaba en cemu al conectar (lo puso el jugador 1): también confirmado
        let st = conn("cemu", Role::Wiimote, "pro", 1, true, 0);
        assert_eq!(settle(&st, intent, now), (Intent::None, None));
        assert_eq!(settle(&st, Intent::None, now), (Intent::None, None), "sin intención no hay nada que resolver");
    }

    #[test]
    fn pc_antiguo_o_silencio_no_dejan_el_gamepad_colgado() {
        let now = Instant::now();
        let intent = wiiu(now);
        // el ok no trae cemu en modes: no hace falta esperar al eco
        let st = conn("pointer", Role::Wiimote, "gamepad", 0, false, 0);
        assert_eq!(settle(&st, intent, now), (Intent::None, Some(notice_old_pc())));
        // sabe de Wii U pero no contesta: a los 3 s se da por perdido
        let st = conn("pointer", Role::Wiimote, "gamepad", 0, true, 0);
        assert_eq!(settle(&st, intent, now + INTENT_TIMEOUT), (intent, None), "justo en el límite aún se espera");
        assert_eq!(settle(&st, intent, now + INTENT_TIMEOUT + Duration::from_millis(1)), (Intent::None, Some(notice_old_pc())));
        let st = conn("pointer", Role::Wiimote, "pro", 2, true, 0);
        assert_eq!(settle(&st, intent, now + Duration::from_secs(4)), (Intent::None, Some(notice_only_p1())));
    }

    #[test]
    fn peticion_de_pad_pendiente_hasta_el_eco_o_dos_segundos() {
        let now = Instant::now();
        let req = PadRequest { pad: "wiimote", seq: 0, since: now };
        let mut st = conn("cemu", Role::Wiimote, "gamepad", 0, true, 1);
        assert_eq!(settle_pad(&st, Some(req), now), Some(req), "sin eco: sigue pendiente");
        assert_eq!(settle_pad(&st, Some(req), now + PAD_TIMEOUT), Some(req));
        assert_eq!(settle_pad(&st, Some(req), now + PAD_TIMEOUT + Duration::from_millis(1)), None, "PC antiguo: caduca");
        if let Status::Connected { pad_seq, pad, .. } = &mut st {
            *pad_seq = 1;
            *pad = "wiimote".into();
        }
        assert_eq!(settle_pad(&st, Some(req), now), None, "eco: resuelta");
        assert_eq!(settle_pad(&st, None, now), None);
        assert_eq!(settle_pad(&Status::Connecting, Some(req), now), None, "sin sesión no hay petición");
        assert_eq!(seqs(&st), (1, 1));
        assert_eq!(seqs(&Status::Connecting), (0, 0));
    }
}
