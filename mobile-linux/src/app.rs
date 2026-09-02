//! Pantallas: Inicio, Conectar (descubrimiento), IP a mano, Código y Mando.
//! Misma lógica de navegación que MainActivity en Android.

use crate::buttons::Buttons;
use crate::calib::{self, Axes};
use crate::discovery::{self, Receiver};
use crate::link::{self, Link, Status};
use crate::sensor;
use crate::store::{self, Pairing};
use crate::theme;
use crate::ui::controller::{Action, ControllerUi};
use crate::ui::keypad::{keypad, Key};
use egui::{RichText, Vec2};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
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
    Calibrate,
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
    calib: Option<Calib>,
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
            calib: None,
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
        match sensor::open_corrected(self.fake) {
            Ok(source) => {
                self.buttons.release_all();
                self.link = Some(Link::connect(pairing, self.buttons.clone(), source, mode.map(|m| m.to_owned())));
                self.was_connected = false;
                self.screen = Screen::Controller;
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
        if ui
            .add(egui::Button::new(RichText::new("Calibrar sensores").size(13.0).color(theme::TEXT)).fill(theme::CARD))
            .on_hover_text("Si el puntero va al revés o a tirones: seis posturas guiadas y listo")
            .clicked()
        {
            self.start_calibration();
        }
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
        ui.label(RichText::new("Calibrar sensores").size(26.0).strong().color(theme::TEXT));
        ui.label(
            RichText::new("Tres posturas quietas y tres gestos: así sé hacia dónde mira cada eje de tus sensores.")
                .size(12.0)
                .color(theme::TEXT_DIM),
        );
        if let Some(axes) = c.done {
            ui.add_space(12.0);
            ui.label(RichText::new("Listo. Calibración guardada:").size(16.0).color(theme::TEXT));
            ui.label(RichText::new(axes.describe()).size(20.0).strong().color(theme::OK));
            ui.label(
                RichText::new(if axes.is_identity() {
                    "Todos los ejes ya venían bien."
                } else {
                    "Los ejes marcados con - se invierten a partir de ahora."
                })
                .size(13.0)
                .color(theme::TEXT_DIM),
            );
            ui.add_space(16.0);
            if ui
                .add_sized(Vec2::new(ui.available_width(), 56.0), egui::Button::new(RichText::new("Volver").size(18.0)).fill(theme::BLUE))
                .clicked()
            {
                leave = true;
            }
        } else {
            let step = &calib::STEPS[c.step];
            ui.label(RichText::new(format!("Paso {} de {}", c.step + 1, calib::STEPS.len())).size(13.0).color(theme::TEXT_DIM));
            ui.add_space(10.0);
            ui.label(RichText::new(step.title).size(20.0).strong().color(theme::TEXT));
            ui.add_space(6.0);
            ui.label(RichText::new(step.text).size(16.0).color(theme::TEXT));
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
                egui::Button::new(RichText::new(label).size(20.0).strong().color(theme::TEXT))
                    .min_size(Vec2::new(ui.available_width(), 64.0))
                    .fill(theme::BLUE),
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
                ui.label(RichText::new(m).size(14.0).color(theme::ERROR));
            }
            ui.add_space(12.0);
            match c.last {
                Some(s) => ui.label(
                    RichText::new(format!(
                        "accel [{:.1} {:.1} {:.1}] m/s²   gyro [{:.2} {:.2} {:.2}] rad/s",
                        s.accel[0], s.accel[1], s.accel[2], s.gyro[0], s.gyro[1], s.gyro[2]
                    ))
                    .size(11.0)
                    .color(theme::TEXT_DIM),
                ),
                None => ui.label(RichText::new("Esperando muestras del sensor…").size(12.0).color(theme::WARN)),
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
        ctx.request_repaint_after(Duration::from_millis(if self.screen == Screen::Calibrate { 30 } else { 100 }));
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
                Screen::Calibrate => self.ui_calibrate(ui),
            });
    }
}
