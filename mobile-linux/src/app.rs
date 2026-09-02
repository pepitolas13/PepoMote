//! Pantallas: Inicio, Conectar (descubrimiento), IP a mano, Código y Mando.
//! Misma lógica de navegación que MainActivity en Android.

use crate::buttons::Buttons;
use crate::discovery::{self, Receiver};
use crate::link::{self, Link, Status};
use crate::sensor;
use crate::store::{self, Pairing};
use crate::theme;
use crate::ui::controller::{Action, ControllerUi};
use crate::ui::keypad::{keypad, Key};
use egui::{RichText, Vec2};
use std::io::Write;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Log de diagnóstico: ~/.config/pepotech/PepoMote/mobile.log (tamaño de
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    Pair,
    Manual,
    Code,
    Controller,
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
    // Mando
    controller: ControllerUi,
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
}

fn describe_sensors(fake: bool) -> String {
    match sensor::open(fake) {
        Ok(s) => s.describe(),
        Err(e) => format!("sin sensores: {e}"),
    }
}

impl MobileApp {
    pub fn new(cc: &eframe::CreationContext<'_>, fake: bool, autoconnect: Option<String>) -> Self {
        theme::apply(&cc.egui_ctx);
        let mut app = Self::build(fake);
        if let Some(m) = autoconnect {
            let dolphin = m == "dolphin";
            app.open_controller(Some(if dolphin { "dolphin" } else { "pointer" }), dolphin);
        }
        app
    }

    fn build(fake: bool) -> Self {
        Self {
            screen: Screen::Home,
            pairing: store::load(),
            link: None,
            buttons: Arc::new(Buttons::new()),
            fake,
            sensor_desc: describe_sensors(fake),
            diag: String::new(),
            diag_touches: 0,
            diag_pointer: 0,
            discovered: Vec::new(),
            scan_rx: None,
            last_scan: None,
            target: None,
            code: String::new(),
            manual: String::new(),
            pair_rx: None,
            pair_error: None,
            controller: ControllerUi::new(),
            dolphin_only: false,
            recenter_at: None,
            was_connected: false,
            error: None,
        }
    }

    fn link_alive(&self) -> bool {
        self.link
            .as_ref()
            .is_some_and(|l| matches!(l.status(), Status::Connected { .. } | Status::Connecting))
    }

    /// Conecta (o cambia de modo si ya hay enlace) y va al mando.
    fn open_controller(&mut self, mode: Option<&'static str>, dolphin_only: bool) {
        self.dolphin_only = dolphin_only;
        if self.link_alive() {
            if let (Some(m), Some(l)) = (mode, &self.link) {
                l.send_mode(m);
            }
            self.screen = Screen::Controller;
            return;
        }
        let Some(pairing) = self.pairing.clone() else {
            self.screen = Screen::Pair;
            return;
        };
        match sensor::open(self.fake) {
            Ok(source) => {
                self.buttons.release_all();
                self.link = Some(Link::connect(pairing, self.buttons.clone(), source, mode.map(|m| m.to_owned())));
                self.was_connected = false;
                self.screen = Screen::Controller;
            }
            Err(e) => {
                // Sin sensores no hay mando: a Inicio, donde se ve el motivo
                // (antes se quedaba en la pantalla del código, en silencio)
                self.sensor_desc = format!("sin sensores: {e}");
                self.error = Some(e);
                self.screen = Screen::Home;
            }
        }
    }

    fn close_link(&mut self) {
        if let Some(l) = self.link.take() {
            l.disconnect();
        }
        self.buttons.release_all();
    }

    fn poll_link(&mut self) {
        let Some(l) = &self.link else { return };
        match l.status() {
            Status::Failed { msg, .. } => {
                self.error = Some(msg);
                self.link = None;
                if self.screen == Screen::Controller {
                    self.screen = Screen::Home;
                }
            }
            Status::Connected { .. } => {
                if !self.was_connected {
                    self.was_connected = true;
                    // "Pulsar la diana" al conectar: centra el cursor con el
                    // móvil ya en la mano (igual que en Android)
                    self.recenter_at = Some(Instant::now() + Duration::from_millis(400));
                }
            }
            Status::Disconnected => {
                self.link = None;
                self.was_connected = false;
            }
            Status::Connecting => {}
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
        ui.label(RichText::new("PepoMote").size(38.0).strong().color(theme::TEXT));
        ui.label(RichText::new("Apunta. Haz clic. Juega.").size(14.0).color(theme::TEXT_DIM));
        ui.add_space(6.0);
        let (dot, txt) = match self.link.as_ref().map(|l| l.status()) {
            Some(Status::Connected { pc_name, .. }) => (theme::OK, format!("Conectado a {pc_name}")),
            Some(Status::Connecting) => (theme::WARN, "Conectando…".to_owned()),
            _ => match &self.pairing {
                Some(p) => (theme::TEXT_DIM, format!("Emparejado con {} · sin conexión", p.pc_name)),
                None => (theme::TEXT_DIM, "Sin emparejar: toca Conectar".to_owned()),
            },
        };
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 5.0, dot);
            ui.label(RichText::new(txt).size(13.0).color(theme::TEXT_DIM));
        });
        ui.add_space(18.0);

        let w = ui.available_width();
        let cw = (w - 12.0) / 2.0;
        let ch = 112.0;
        let card = |ui: &mut egui::Ui, title: &str, sub: &str, accent: egui::Color32| -> bool {
            ui.add_sized(
                Vec2::new(cw, ch),
                egui::Button::new(
                    RichText::new(format!("{title}\n{sub}")).size(16.0).color(theme::TEXT),
                )
                .fill(theme::CARD)
                .stroke(egui::Stroke::new(2.0_f32, accent)),
            )
            .clicked()
        };
        let mut go: Option<u8> = None;
        ui.horizontal(|ui| {
            if card(ui, "Conectar", "apunta y haz clic", theme::BLUE) {
                go = Some(0);
            }
            if card(ui, "Mando", "solo botones", theme::BLUE) {
                go = Some(1);
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if card(ui, "Dolphin", "Wiimote virtual", theme::OK) {
                go = Some(2);
            }
            if card(ui, "Emparejar", "otro PC / código", theme::TEXT_DIM) {
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
                self.screen = Screen::Pair;
            }
            _ => {}
        }

        ui.add_space(16.0);
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).size(13.0).color(theme::ERROR));
            ui.add_space(6.0);
        }
        ui.label(RichText::new(format!("Sensores: {}", self.sensor_desc)).size(11.0).color(theme::TEXT_DIM));
        ui.label(RichText::new(&self.diag).size(11.0).color(theme::TEXT_DIM));
        ui.label(
            RichText::new(format!("v{} · pv1 · Linux móvil", env!("CARGO_PKG_VERSION")))
                .size(11.0)
                .color(theme::TEXT_DIM),
        );
    }

    fn ui_pair(&mut self, ui: &mut egui::Ui) {
        self.poll_scan();
        ui.add_space(10.0);
        ui.label(RichText::new("Conectar").size(28.0).strong().color(theme::TEXT));
        ui.label(
            RichText::new("Abre PepoMote en tu PC. Elige tu PC y teclea el código de 4 dígitos que hay bajo su QR.")
                .size(13.0)
                .color(theme::TEXT_DIM),
        );
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
        ui.label(RichText::new(msg).size(13.0).color(theme::TEXT_DIM));
        ui.add_space(6.0);
        let mut chosen: Option<Receiver> = None;
        for r in &self.discovered {
            if ui
                .add_sized(
                    Vec2::new(ui.available_width(), 64.0),
                    egui::Button::new(
                        RichText::new(format!("{}\n{}:{}", r.name, r.host, r.port)).size(15.0).color(theme::TEXT),
                    )
                    .fill(theme::CARD),
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
        if ui.button(RichText::new("Volver").size(14.0).color(theme::TEXT_DIM)).clicked() {
            self.screen = Screen::Home;
        }
    }

    fn ui_manual(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.label(RichText::new("IP del PC").size(26.0).strong().color(theme::TEXT));
        ui.label(RichText::new("La que muestra el receptor bajo el QR (IP : puerto)").size(13.0).color(theme::TEXT_DIM));
        ui.add_space(10.0);
        let shown = if self.manual.is_empty() { "…".to_owned() } else { self.manual.clone() };
        ui.label(RichText::new(shown).size(30.0).strong().color(theme::TEXT));
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
            None => {}
        }
        ui.add_space(8.0);
        if ui.button(RichText::new("Volver").size(14.0).color(theme::TEXT_DIM)).clicked() {
            self.screen = Screen::Pair;
        }
    }

    fn ui_code(&mut self, ui: &mut egui::Ui) {
        self.poll_pairing();
        let name = self.target.as_ref().map(|t| t.name.clone()).unwrap_or_default();
        ui.add_space(10.0);
        ui.label(RichText::new("Código").size(26.0).strong().color(theme::TEXT));
        ui.label(
            RichText::new(format!("Los 4 dígitos bajo el QR de {name}"))
                .size(13.0)
                .color(theme::TEXT_DIM),
        );
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 4.0 * 58.0) / 2.0);
            for i in 0..4 {
                let (r, _) = ui.allocate_exact_size(Vec2::new(50.0, 60.0), egui::Sense::hover());
                ui.painter().rect(r, egui::Rounding::same(12.0), theme::CARD, egui::Stroke::new(1.5_f32, theme::CARD_BORDER));
                if let Some(c) = self.code.chars().nth(i) {
                    ui.painter().text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        c,
                        egui::FontId::proportional(30.0),
                        theme::TEXT,
                    );
                }
            }
        });
        ui.add_space(10.0);
        if self.pair_rx.is_some() {
            ui.label(RichText::new("Emparejando…").size(14.0).color(theme::BLUE));
        } else if let Some(e) = &self.pair_error {
            ui.label(RichText::new(e).size(13.0).color(theme::ERROR));
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
        if ui.button(RichText::new("Volver").size(14.0).color(theme::TEXT_DIM)).clicked() {
            self.pair_rx = None;
            self.screen = Screen::Pair;
        }
    }

    fn ui_controller(&mut self, ui: &mut egui::Ui) {
        let Some(link) = &self.link else {
            ui.add_space(10.0);
            ui.label(RichText::new("Sin conexión").size(20.0).strong().color(theme::TEXT));
            ui.add_space(8.0);
            if self.pairing.is_some() && ui.button(RichText::new("Reconectar").size(15.0).color(theme::BLUE)).clicked() {
                self.open_controller(Some("pointer"), self.dolphin_only);
            }
            if ui.button(RichText::new("Volver").size(14.0).color(theme::TEXT_DIM)).clicked() {
                self.screen = Screen::Home;
            }
            return;
        };
        let status = link.status();
        let slot0 = matches!(&status, Status::Connected { slot: 0, .. });
        let hz = link.sensor_hz();
        match self.controller.show(ui, &self.buttons, &status, !self.dolphin_only && slot0, hz) {
            Action::Exit => {
                self.close_link();
                self.screen = Screen::Home;
            }
            Action::Mode(m) => link.send_mode(m),
            Action::None => {}
        }
    }
}

impl eframe::App for MobileApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(100));
        self.poll_link();
        self.update_diag(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme::BACKGROUND).inner_margin(16.0))
            .show(ctx, |ui| match self.screen {
                Screen::Home => self.ui_home(ui),
                Screen::Pair => self.ui_pair(ui),
                Screen::Manual => self.ui_manual(ui),
                Screen::Code => self.ui_code(ui),
                Screen::Controller => self.ui_controller(ui),
            });
    }
}
