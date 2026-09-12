//! Pantallas: Inicio, Conectar (descubrimiento), IP a mano, Código, Mando,
//! Nunchuk y GamePad de Wii U. Misma lógica de navegación que MainActivity
//! en Android: la pantalla del mando la decide lo que diga el receptor
//! (modo y tipo de mando), no solo lo que se tocó en Inicio.

use crate::buttons::Buttons;
use crate::calib::{self, Axes};
use crate::discovery::{self, Receiver};
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
use egui::{RichText, Vec2};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    let pc = pc_name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Tu PC");
    format!(
        "{pc} ya no reconoce este móvil: su emparejamiento ha cambiado (PepoMote reinstalado o restablecido en el PC). \
         Elige tu PC y teclea el código nuevo que hay bajo su QR."
    )
}

const NOTICE_OLD_PC: &str = "El PC necesita PepoMote 1.3 para Wii U";
const NOTICE_ONLY_P1: &str = "Solo el Jugador 1 cambia el modo";
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
    if let Status::Connected { mode, role, pad, .. } = status {
        if mode == "cemu" && *role == Role::Wiimote && pad != "wiimote" {
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
    let Intent::WiiU { seq, since } = intent else {
        return (Intent::None, None);
    };
    let Status::Connected { mode, mode_by_pc, mode_seq, supports_cemu, slot, .. } = status else {
        return (intent, None); // aún conectando
    };
    if mode == "cemu" {
        return (Intent::None, None);
    }
    if *mode_seq > seq && *mode_by_pc {
        // el PC cambió de modo por su cuenta (abrió o cerró un emulador): la
        // intención se descarta sin aviso, el suyo ya lo explica
        return (Intent::None, None);
    }
    let failed = *mode_seq > seq || !supports_cemu || now.saturating_duration_since(since) > INTENT_TIMEOUT;
    if !failed {
        return (intent, None);
    }
    let notice = if *supports_cemu && *slot != 0 { NOTICE_ONLY_P1 } else { NOTICE_OLD_PC };
    (Intent::None, Some(notice))
}

/// Petición de `pad` pendiente: se cierra con el eco (`pad_seq` avanza) o
/// pasado `PAD_TIMEOUT` (PC antiguo: no contesta).
fn settle_pad(status: &Status, req: Option<PadRequest>, now: Instant) -> Option<PadRequest> {
    let r = req?;
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
    scan_rx: Option<mpsc::Receiver<Vec<Receiver>>>,
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
    /// Teclado para el teclado en pantalla de Cemu (modo Wii U): tapa la
    /// pantalla de juego sin cambiarla (los INPUT siguen saliendo).
    text_dialog: Option<TextDialog>,
}

fn describe_sensors(fake: bool) -> String {
    match sensor::open_corrected(fake) {
        Ok(s) => s.describe(),
        Err(e) => format!("sin sensores: {e}"),
    }
}

impl MobileApp {
    pub fn new(cc: &eframe::CreationContext<'_>, fake: bool, autoconnect: Option<String>) -> Self {
        theme::apply(&cc.egui_ctx);
        let mut app = Self::build(fake);
        theme::set_preference(&cc.egui_ctx, app.settings.theme);
        if let Some(m) = autoconnect {
            match m.as_str() {
                "nunchuk" => app.open_nunchuk(),
                "cemu" => app.open_gamepad(),
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

    /// Conecta (o cambia de modo si ya hay enlace) y va al mando.
    fn open_controller(&mut self, mode: Option<&'static str>, dolphin_only: bool) {
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
        self.dolphin_only = false;
        let before = self.mode_seq();
        self.open_link(Role::Wiimote, Some("cemu"));
        if self.link.is_some() {
            // el eco que resuelve la intención es el posterior a la petición
            // (una sesión nueva empieza en 0)
            let seq = before.min(self.mode_seq());
            self.intent = Intent::WiiU { seq, since: Instant::now() };
            self.go_play();
        }
    }

    /// El receptor ha confirmado Wii U y somos GamePad/Pro: se juega (y van
    /// paquetes de 80 bytes).
    fn wiiu_confirmed(&self) -> bool {
        self.link.as_ref().is_some_and(|l| {
            matches!(l.status(), Status::Connected { mode, role: Role::Wiimote, pad, .. } if mode == "cemu" && pad != "wiimote")
        })
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
        match sensor::open_corrected(self.fake) {
            Ok(source) => {
                self.buttons.release_all();
                self.link = Some(Link::connect(pairing, self.buttons.clone(), source, mode.map(|m| m.to_owned()), role));
                self.link_role = role;
                self.was_connected = false;
                self.go_play();
            }
            Err(e) => {
                // Sin sensores no hay mando: a Inicio, con el motivo y el
                // inventario de lo que el sistema expone (para saber qué falta)
                let inv = sensor::inventory();
                log_line(&format!("sensores: {e}\n{inv}"));
                self.sensor_desc = format!("sin sensores: {e}");
                self.error = Some(format!("{e}\n\n{inv}"));
                self.screen = Screen::Home;
            }
        }
    }

    fn close_link(&mut self) {
        self.close_screen();
        if let Some(l) = self.link.take() {
            l.disconnect();
        }
        self.buttons.release_all();
        self.inhibit = None;
        self.intent = Intent::None;
        self.pad_pending = None;
    }

    /// Cierra el canal de la pantalla del GamePad, si estaba abierto.
    fn close_screen(&mut self) {
        if let Some(c) = self.pad_screen.take() {
            c.stop();
        }
    }

    /// La sesión está en Wii U (el teclado solo tiene sentido ahí).
    fn mode_is_cemu(&self) -> bool {
        self.link
            .as_ref()
            .is_some_and(|l| matches!(l.status(), Status::Connected { mode, .. } if mode == "cemu"))
    }

    /// Abre el teclado para Cemu tapando la pantalla de juego: los dedos que
    /// hubiera se sueltan (sus toques ya no llegarán al mando), pero la
    /// pantalla no cambia, así los INPUT siguen saliendo igual.
    fn open_text_dialog(&mut self) {
        self.gamepad.release(&self.buttons);
        self.controller.release(&self.buttons);
        self.text_dialog = Some(TextDialog::new());
    }

    /// El diálogo del teclado: cada botón manda lo que toque por el canal de
    /// control (`effect`, puro) y vacía el campo o cierra según diga.
    fn ui_text(&mut self, ui: &mut egui::Ui) {
        let (Some(dlg), Some(link)) = (self.text_dialog.as_mut(), self.link.as_ref()) else {
            self.text_dialog = None;
            return;
        };
        if let Some(b) = dlg.show(ui) {
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
            .is_some_and(|l| matches!(l.status(), Status::Connected { pad, .. } if pad == "gamepad"))
    }

    /// Canal de la pantalla del GamePad (doble pantalla): abierto solo
    /// mientras `want` (pantalla GamePad operativa, Wii U confirmado y
    /// `pad == "gamepad"`); cerrado al salir de esa pantalla, al pasar a
    /// Pro/Mando de Wii o al perder el enlace. Idempotente: una sola
    /// instancia, que se reabre si cambia la sesión o el tamaño de la zona.
    fn sync_screen(&mut self, ctx: &egui::Context, want: bool) {
        let endpoint = if want { self.link.as_ref().and_then(|l| l.screen_endpoint()) } else { None };
        let Some(endpoint) = endpoint else {
            self.close_screen();
            return;
        };
        // el tamaño se pide como la zona táctil real: hasta que esté
        // maquetada (siguiente frame) no se abre
        let Some(zone) = self.gamepad.touch_size_px(ctx.pixels_per_point()) else { return };
        let size = screen::wanted_size(zone);
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
                    // El PC ya no reconoce el emparejamiento: a Conectar con
                    // la explicación, en vez de un error suelto en el inicio
                    self.error = None;
                    self.pair_reason = Some(re_pair_reason(self.pairing.as_ref().map(|p| p.pc_name.as_str())));
                    self.discovered.clear();
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
        if let Some(rx) = &self.scan_rx {
            if let Ok(list) = rx.try_recv() {
                self.discovered = list;
                self.scan_rx = None;
            }
        }
        if self.scan_rx.is_none() && self.last_scan.is_none_or(|t| t.elapsed() > Duration::from_millis(2500)) {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(discovery::scan(Duration::from_millis(1500)));
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

    fn poll_pairing(&mut self) {
        let Some(rx) = &self.pair_rx else { return };
        match rx.try_recv() {
            Ok(Ok(p)) => {
                store::save(&p);
                self.pairing = Some(p);
                self.pair_rx = None;
                self.pair_reason = None;
                self.code.clear();
                self.open_controller(Some("pointer"), false);
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
        ui.label(RichText::new("Apunta. Haz clic. Juega.").size(14.0).color(theme::text_dim()));
        ui.add_space(6.0);
        let (dot, txt) = match self.link.as_ref().map(|l| l.status()) {
            Some(Status::Connected { pc_name, .. }) => (theme::ok(), format!("Conectado a {pc_name}")),
            Some(Status::Connecting) => (theme::warn(), "Conectando…".to_owned()),
            Some(Status::Reconnecting { pc_name, .. }) => (theme::warn(), format!("Reconectando con {pc_name}…")),
            _ => match &self.pairing {
                Some(p) => (theme::text_dim(), format!("Emparejado con {} · sin conexión", p.pc_name)),
                None => (theme::text_dim(), "Sin emparejar: toca Conectar".to_owned()),
            },
        };
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 5.0, dot);
            ui.label(RichText::new(txt).size(13.0).color(theme::text_dim()));
        });
        ui.add_space(18.0);

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
            if card(ui, half, "Conectar", "apunta y haz clic", theme::blue()) {
                go = Some(0);
            }
            if card(ui, half, "Mando", "solo botones", theme::blue()) {
                go = Some(1);
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if card(ui, half, "Dolphin", "Wiimote virtual", theme::ok()) {
                go = Some(2);
            }
            if card(ui, half, "Nunchuk", "la otra mano", theme::ok()) {
                go = Some(4);
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if card(ui, half, "Wii U", "GamePad para Cemu", theme::ok()) {
                go = Some(5);
            }
            if card(ui, half, "Emparejar", "otro PC / código", theme::text_dim()) {
                go = Some(3);
            }
        });
        match go {
            Some(0) => self.open_controller(Some("pointer"), false),
            Some(1) => self.open_controller(None, false),
            Some(2) => self.open_controller(Some("dolphin"), true),
            Some(3) => {
                self.discovered.clear();
                self.last_scan = None;
                self.pair_reason = None;
                self.screen = Screen::Pair;
            }
            Some(4) => self.open_nunchuk(),
            Some(5) => self.open_gamepad(),
            _ => {}
        }

        ui.add_space(16.0);
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).size(13.0).color(theme::error()));
            ui.add_space(6.0);
        }
        ui.label(RichText::new(format!("Sensores: {}", self.sensor_desc)).size(11.0).color(theme::text_dim()));
        if ui
            .add(egui::Button::new(RichText::new("Calibrar sensores").size(13.0).color(theme::text())).fill(theme::card()))
            .on_hover_text("Si el puntero va al revés o a tirones: seis posturas guiadas y listo")
            .clicked()
        {
            self.start_calibration();
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Tema").size(12.0).color(theme::text_dim()));
            let mut pref = self.settings.theme;
            for p in theme::ThemePref::ALL {
                let label = match p {
                    theme::ThemePref::System => "Sistema",
                    theme::ThemePref::Light => "Claro",
                    theme::ThemePref::Dark => "Oscuro",
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
        ui.label(RichText::new(&self.diag).size(11.0).color(theme::text_dim()));
        ui.label(
            RichText::new(format!("v{} · pv1 · Linux móvil", env!("CARGO_PKG_VERSION")))
                .size(11.0)
                .color(theme::text_dim()),
        );
    }

    fn ui_pair(&mut self, ui: &mut egui::Ui) {
        self.poll_scan();
        ui.add_space(10.0);
        let title = if self.pair_reason.is_some() { "Vuelve a emparejar" } else { "Conectar" };
        ui.label(RichText::new(title).size(28.0).strong().color(theme::text()));
        ui.label(
            RichText::new("Abre PepoMote en tu PC. Elige tu PC y teclea el código de 4 dígitos que hay bajo su QR.")
                .size(13.0)
                .color(theme::text_dim()),
        );
        if let Some(reason) = &self.pair_reason {
            ui.add_space(8.0);
            ui.label(RichText::new(reason).size(13.0).color(theme::error()));
        }
        ui.add_space(12.0);
        let msg = if self.discovered.is_empty() {
            if self.scan_rx.is_some() {
                "Buscando receptores en tu red…"
            } else {
                "Ningún receptor a la vista (¿misma Wi-Fi?). Puedes escribir la IP a mano."
            }
        } else {
            "En tu red:"
        };
        ui.label(RichText::new(msg).size(13.0).color(theme::text_dim()));
        ui.add_space(6.0);
        let mut chosen: Option<Receiver> = None;
        for r in &self.discovered {
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
            .add_sized(Vec2::new(ui.available_width(), 48.0), egui::Button::new(RichText::new("Escribir IP a mano").size(15.0)))
            .clicked()
        {
            self.manual.clear();
            self.screen = Screen::Manual;
        }
        ui.add_space(8.0);
        if ui.button(RichText::new("Volver").size(14.0).color(theme::text_dim())).clicked() {
            self.pair_reason = None;
            self.screen = Screen::Home;
        }
    }

    fn ui_manual(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.label(RichText::new("IP del PC").size(26.0).strong().color(theme::text()));
        ui.label(RichText::new("La que muestra el receptor bajo el QR (IP : puerto)").size(13.0).color(theme::text_dim()));
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
        if ui.button(RichText::new("Volver").size(14.0).color(theme::text_dim())).clicked() {
            self.screen = Screen::Pair;
        }
    }

    fn ui_code(&mut self, ui: &mut egui::Ui) {
        self.poll_pairing();
        let name = self.target.as_ref().map(|t| t.name.clone()).unwrap_or_default();
        ui.add_space(10.0);
        ui.label(RichText::new("Código").size(26.0).strong().color(theme::text()));
        ui.label(
            RichText::new(format!("Los 4 dígitos bajo el QR de {name}"))
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
            ui.label(RichText::new("Emparejando…").size(14.0).color(theme::blue()));
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
        if ui.button(RichText::new("Volver").size(14.0).color(theme::text_dim())).clicked() {
            self.pair_rx = None;
            self.screen = Screen::Pair;
        }
    }

    fn ui_controller(&mut self, ui: &mut egui::Ui) {
        let Some(link) = &self.link else {
            ui.add_space(10.0);
            ui.label(RichText::new("Sin conexión").size(20.0).strong().color(theme::text()));
            ui.add_space(8.0);
            if self.pairing.is_some() && ui.button(RichText::new("Reconectar").size(15.0).color(theme::blue())).clicked() {
                self.open_controller(Some("pointer"), self.dolphin_only);
            }
            if ui.button(RichText::new("Volver").size(14.0).color(theme::text_dim())).clicked() {
                self.screen = Screen::Home;
            }
            return;
        };
        let status = link.status();
        let slot0 = matches!(&status, Status::Connected { slot: 0, .. });
        let hz = link.sensor_hz();
        let pending = self.pad_pending.map(|p| p.pad);
        match self.controller.show(ui, &self.buttons, &status, !self.dolphin_only && slot0, pending, hz) {
            Action::Exit => {
                self.close_link();
                self.screen = Screen::Home;
            }
            Action::Mode("cemu") => {
                // chip Wii U: al GamePad ya (optimista); el eco confirma o
                // devuelve aquí con aviso
                let seq = seqs(&status).0;
                link.send_mode("cemu");
                self.intent = Intent::WiiU { seq, since: Instant::now() };
                self.screen = route(&status, self.link_role, self.intent);
            }
            Action::Mode(m) => link.send_mode(m),
            Action::Pad(p) => {
                let seq = seqs(&status).1;
                link.send_pad(p);
                self.pad_pending = Some(PadRequest { pad: p, seq, since: Instant::now() });
            }
            Action::Keyboard => self.open_text_dialog(),
            Action::None => {}
        }
    }

    fn ui_gamepad(&mut self, ui: &mut egui::Ui) {
        let Some(link) = &self.link else {
            ui.add_space(10.0);
            ui.label(RichText::new("Sin conexión").size(20.0).strong().color(theme::text()));
            ui.add_space(8.0);
            if self.pairing.is_some() && ui.button(RichText::new("Reconectar").size(15.0).color(theme::blue())).clicked() {
                self.open_gamepad();
            }
            if ui.button(RichText::new("Volver").size(14.0).color(theme::text_dim())).clicked() {
                self.screen = Screen::Home;
            }
            return;
        };
        let status = link.status();
        let slot0 = matches!(&status, Status::Connected { slot: 0, .. });
        let inputs = GamePadInputs {
            status: &status,
            rotation: self.settings.rotation,
            show_chips: !self.dolphin_only && slot0,
            optimistic: self.intent != Intent::None,
            pad_pending: self.pad_pending.map(|p| p.pad),
            sensor_hz: link.sensor_hz(),
            screen: self.pad_screen.as_ref(),
        };
        match self.gamepad.show(ui, &self.buttons, &inputs) {
            GamePadAction::Exit => {
                self.close_link();
                self.screen = Screen::Home;
            }
            GamePadAction::Mode(m) => link.send_mode(m),
            GamePadAction::Pad(p) => {
                let seq = seqs(&status).1;
                link.send_pad(p);
                self.pad_pending = Some(PadRequest { pad: p, seq, since: Instant::now() });
            }
            GamePadAction::Rotation(r) => {
                self.settings.rotation = r;
                store::save_settings(&self.settings);
                self.buttons.set_rotation(r.as_u8());
                log_line(&format!("giro apaisado: borde superior a la {}", r.label()));
            }
            GamePadAction::Keyboard => self.open_text_dialog(),
            GamePadAction::None => {}
        }
    }

    fn ui_nunchuk(&mut self, ui: &mut egui::Ui) {
        let Some(link) = &self.link else {
            ui.add_space(10.0);
            ui.label(RichText::new("Sin conexión").size(20.0).strong().color(theme::text()));
            ui.add_space(8.0);
            if self.pairing.is_some() && ui.button(RichText::new("Reconectar").size(15.0).color(theme::blue())).clicked() {
                self.open_nunchuk();
            }
            if ui.button(RichText::new("Volver").size(14.0).color(theme::text_dim())).clicked() {
                self.screen = Screen::Home;
            }
            return;
        };
        let status = link.status();
        let hz = link.sensor_hz();
        if let NunchukAction::Exit = self.nunchuk.show(ui, &self.buttons, &status, hz) {
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
                self.error = Some(format!("No puedo abrir los sensores para calibrar: {e}"));
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
            let step = &calib::STEPS[c.step];
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
                    if c.step >= calib::STEPS.len() {
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
        ui.label(RichText::new("Calibrar sensores").size(26.0).strong().color(theme::text()));
        ui.label(
            RichText::new("Tres posturas quietas y tres gestos: así sé hacia dónde mira cada eje de tus sensores.")
                .size(12.0)
                .color(theme::text_dim()),
        );
        if let Some(axes) = c.done {
            ui.add_space(12.0);
            ui.label(RichText::new("Listo. Calibración guardada:").size(16.0).color(theme::text()));
            ui.label(RichText::new(axes.describe()).size(20.0).strong().color(theme::ok()));
            ui.label(
                RichText::new(if axes.is_identity() {
                    "Todos los ejes ya venían bien."
                } else {
                    "Los ejes marcados con - se invierten a partir de ahora."
                })
                .size(13.0)
                .color(theme::text_dim()),
            );
            ui.add_space(16.0);
            if ui
                .add_sized(Vec2::new(ui.available_width(), 56.0), egui::Button::new(RichText::new("Volver").size(18.0)).fill(theme::blue()))
                .clicked()
            {
                leave = true;
            }
        } else {
            let step = &calib::STEPS[c.step];
            ui.label(RichText::new(format!("Paso {} de {}", c.step + 1, calib::STEPS.len())).size(13.0).color(theme::text_dim()));
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
                Some(t) => format!(
                    "{}… {t:.1} s",
                    if step.kind == calib::Kind::Motion { "¡Ya! Mueve" } else { "Midiendo" }
                ),
                None => "Listo".to_owned(),
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
                None => ui.label(RichText::new("Esperando muestras del sensor…").size(12.0).color(theme::warn())),
            };
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Cancelar").size(15.0)).clicked() {
                    leave = true;
                }
                if ui.button(RichText::new("Borrar calibración").size(15.0)).clicked() {
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
        self.update_diag(ctx);

        // El hilo de paquetes manda 80 bytes (y remapea los sensores) solo
        // mientras se juega en la pantalla GamePad con Wii U CONFIRMADO por el
        // receptor (conectando u optimista: 72 bytes como siempre, y los
        // controles están inertes); al dejar de jugar, todo suelto
        let on_gamepad = self.screen == Screen::GamePad;
        let gamepad = on_gamepad && self.wiiu_confirmed();
        if gamepad != self.buttons.is_gamepad() {
            self.buttons.set_gamepad(gamepad);
            if !gamepad {
                self.buttons.release_all();
            }
        }
        // Doble pantalla: el canal de la pantalla del GamePad solo mientras se
        // juega aquí como GamePad (un Pro Controller no tiene pantalla)
        let want_screen = gamepad && self.pad_is_gamepad();
        self.sync_screen(ctx, want_screen);

        // El teclado para Cemu tapa la pantalla de juego sin cambiarla (los
        // INPUT siguen saliendo, el giro no cambia); se va solo si se pierde
        // el enlace, la pantalla de juego o el modo Wii U
        let text_open = self.text_dialog.is_some()
            && matches!(self.screen, Screen::Controller | Screen::GamePad)
            && self.mode_is_cemu();
        if !text_open {
            self.text_dialog = None;
        }

        egui::CentralPanel::default()
            // el GamePad se pinta a mano y aprovecha hasta el borde
            .frame(egui::Frame::default().fill(theme::background()).inner_margin(if on_gamepad && !text_open { 6.0 } else { 16.0 }))
            .show(ctx, |ui| {
                if text_open {
                    return self.ui_text(ui);
                }
                match self.screen {
                    Screen::Home => self.ui_home(ui),
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
            pad: pad.into(),
            notice: None,
            mode_seq,
            pad_seq: 0,
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
        assert_eq!(notice, Some(NOTICE_OLD_PC));
        assert_eq!(route(&st, Role::Wiimote, after), Screen::Controller);
        // jugador 2 con un receptor que sí sabe de Wii U: solo el jugador 1 cambia el modo
        let st = conn("pointer", Role::Wiimote, "pro", 1, true, 1);
        assert_eq!(settle(&st, intent, now), (Intent::None, Some(NOTICE_ONLY_P1)));
        // jugador 2 con receptor antiguo: el aviso es el de la versión
        let st = conn("pointer", Role::Wiimote, "pro", 1, false, 1);
        assert_eq!(settle(&st, intent, now), (Intent::None, Some(NOTICE_OLD_PC)));
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
        assert_eq!(settle(&st, intent, now), (Intent::None, Some(NOTICE_OLD_PC)));
        // sabe de Wii U pero no contesta: a los 3 s se da por perdido
        let st = conn("pointer", Role::Wiimote, "gamepad", 0, true, 0);
        assert_eq!(settle(&st, intent, now + INTENT_TIMEOUT), (intent, None), "justo en el límite aún se espera");
        assert_eq!(settle(&st, intent, now + INTENT_TIMEOUT + Duration::from_millis(1)), (Intent::None, Some(NOTICE_OLD_PC)));
        let st = conn("pointer", Role::Wiimote, "pro", 2, true, 0);
        assert_eq!(settle(&st, intent, now + Duration::from_secs(4)), (Intent::None, Some(NOTICE_ONLY_P1)));
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
