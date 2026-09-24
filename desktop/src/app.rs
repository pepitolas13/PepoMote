use crate::state::LockTolerant;
use crate::pairing::PairingInfo;
use crate::state::{LinkStatus, Mode, PlayerInfo, SharedState};
use crate::theme;
use egui::{Color32, Pos2, Rect, RichText, Rounding, Stroke, Vec2};
use std::time::{Duration, Instant};
use crate::launch::Step;
use std::net::IpAddr;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use crate::i18n;
use crate::state::CfgStatus;
use crate::tr;

/// Cuánto espera la ventana al candado del estado antes de repintar con la
/// foto del fotograma anterior.
const LOCK_WAIT: Duration = Duration::from_millis(200);

pub struct PepoMoteApp {
    updater: crate::update::UpdateUi,
    shared: SharedState,
    pairing: PairingInfo,
    qr_modules: Vec<bool>,
    qr_width: usize,
    /// Autoarranque (registro de Windows). `None` hasta que `reg.exe`,
    /// lanzado en su hilo en el segundo fotograma, contesta: el hilo de la
    /// ventana no lo espera nunca.
    autostart: Option<bool>,
    /// `reg.exe` en curso (la consulta al arrancar, o el cambio pedido en
    /// Ajustes): el resultado llega por aquí. Mientras, la casilla espera
    /// deshabilitada. `Err` trae el valor que hay que restaurar y el motivo.
    autostart_rx: Option<Receiver<Result<bool, (bool, String)>>>,
    /// IP local publicada por el hilo `ip-watch` (el QR debe llevar la IP viva).
    ip: Arc<Mutex<IpAddr>>,
    /// Foto del estado del último fotograma: si el candado del estado tarda
    /// más de `LOCK_WAIT`, se repinta con ella en vez de esperar.
    last_snap: Option<Snapshot>,
    /// Ya se apuntó en el log que el candado tarda (una vez por atasco).
    lock_stall_logged: bool,
    /// Ajustes cambiados en la UI pendientes de escribir a disco.
    config_dirty: bool,
    /// Fotogramas pintados (el segundo marca la ventana como viva).
    frames: u32,
    /// Cuándo se pintó el primer fotograma útil (modo humo).
    painted_at: Option<Instant>,
    /// PEPOMOTE_SMOKE: salir con 0 pasado este tiempo desde ese fotograma.
    smoke: Option<Duration>,
    /// Linux: hay pkexec para el botón "Reparar ahora" (se mira una vez).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pkexec_ok: bool,
    /// Linux: qué comando manual se copió y cuándo (para el «Copiado» efímero).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    copied_at: Option<(String, Instant)>,
    /// Linux: desde cuándo está a la vista la tarjeta de reparación (cuenta
    /// atrás de la reparación automática).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    repair_seen_at: Option<Instant>,
    /// macOS: permiso de Accesibilidad (sondeado una vez por segundo).
    #[cfg(target_os = "macos")]
    ax_ok: bool,
    #[cfg(target_os = "macos")]
    ax_checked: Instant,
    /// Windows: el aviso de que la X deja PepoMote en la bandeja está a la
    /// vista (la primera X; hasta pulsar «Entendido» o «Salir del todo»).
    #[cfg(windows)]
    close_notice: bool,
    /// Windows: ese aviso ya se vio alguna vez (settings.json).
    #[cfg(windows)]
    tray_notice_seen: bool,
}

impl PepoMoteApp {
    /// `start_hidden` (--minimized): en macOS la ventana existe pero oculta y
    /// la app queda sin icono en el Dock hasta «Mostrar» (Windows ni siquiera
    /// llega aquí hasta entonces).
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    pub fn new(cc: &eframe::CreationContext<'_>, shared: SharedState, pairing: PairingInfo, start_hidden: bool) -> Self {
        theme::apply(&cc.egui_ctx);
        let (pref, notice_seen) = {
            let s = shared.lock_tolerant();
            (s.config.theme, s.config.tray_notice_seen)
        };
        #[cfg(not(windows))]
        let _ = notice_seen;
        theme::set_preference(&cc.egui_ctx, pref);
        // macOS: el icono de la barra de menús solo puede nacer en el hilo
        // principal con el bucle de eventos ya en marcha: aquí
        #[cfg(target_os = "macos")]
        {
            if std::env::var_os("PEPOMOTE_NO_TRAY").is_none() {
                crate::tray::start_main_thread(shared.clone());
            }
            if start_hidden {
                crate::macos::set_accessory(true);
            }
        }
        let (qr_modules, qr_width) = build_qr(&pairing.pair_url());
        let ip = Arc::new(Mutex::new(pairing.host));
        start_ip_watch(ip.clone());
        Self {
            updater: crate::update::UpdateUi::default(),
            shared,
            pairing,
            qr_modules,
            qr_width,
            autostart: None,
            autostart_rx: None,
            ip,
            last_snap: None,
            lock_stall_logged: false,
            config_dirty: false,
            frames: 0,
            painted_at: None,
            smoke: crate::launch::smoke_linger(std::env::var(crate::launch::ENV_SMOKE).ok().as_deref()),
            #[cfg(target_os = "linux")]
            pkexec_ok: crate::fixes::pkexec_available(),
            #[cfg(not(target_os = "linux"))]
            pkexec_ok: false,
            copied_at: None,
            repair_seen_at: None,
            #[cfg(target_os = "macos")]
            ax_ok: crate::macos::ax_trusted(),
            #[cfg(target_os = "macos")]
            ax_checked: Instant::now(),
            #[cfg(windows)]
            close_notice: false,
            #[cfg(windows)]
            tray_notice_seen: notice_seen,
        }
    }

    /// El QR se regenera si la IP que publica `ip-watch` ya no es la suya.
    fn refresh_ip(&mut self) {
        let ip = *self.ip.lock_tolerant();
        if ip != self.pairing.host {
            self.pairing.host = ip;
            let (m, w) = build_qr(&self.pairing.pair_url());
            self.qr_modules = m;
            self.qr_width = w;
        }
    }

    /// Lo que contestó `reg.exe` desde su hilo (la consulta o un cambio), si
    /// ya terminó. Un cambio fallido restaura la casilla y cuenta el motivo.
    fn poll_autostart(&mut self) {
        let Some(result) = self.autostart_rx.as_ref().map(|rx| rx.try_recv()) else {
            return;
        };
        match result {
            Ok(Ok(on)) => {
                self.autostart = Some(on);
                self.autostart_rx = None;
            }
            Ok(Err((before, why))) => {
                self.autostart = Some(before);
                self.autostart_rx = None;
                self.shared.lock_tolerant().last_error = Some(tr!("cfg.autostart_err", why));
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                // el hilo murió sin contestar (un pánico, ya en el log): la
                // casilla no se queda esperando para siempre
                self.autostart_rx = None;
                self.autostart.get_or_insert(false);
            }
        }
    }
}

/// Con autoarranque el receptor suele nacer antes que la red (QR con
/// 127.0.0.1), y la IP puede cambiar con la Wi-Fi o el DHCP: cada 3 s se
/// re-consulta (un socket UDP sin tráfico) y la ventana regenera el QR si
/// cambió. En su hilo: un cortafuegos que retenga el socket no puede parar
/// el pintado.
fn start_ip_watch(slot: Arc<Mutex<IpAddr>>) {
    let _ = crate::threads::spawn_guarded(
        "ip-watch",
        crate::threads::OnPanic::Restart { after: Duration::from_secs(5), max: 10 },
        move || loop {
            std::thread::sleep(Duration::from_secs(3));
            let ip = crate::pairing::local_ip();
            *slot.lock_tolerant() = ip;
        },
    );
}

#[derive(Clone)]
struct Snapshot {
    status: LinkStatus,
    mode: Mode,
    players: [Option<PlayerInfo>; crate::net::MAX_PLAYERS],
    player_count: usize,
    pps: f32,
    sensor_hz: f32,
    /// RTT de los últimos latidos del Jugador 1 (sparkline).
    rtt_hist: Vec<f32>,
    dsu_clients: usize,
    dolphin_status: Option<CfgStatus>,
    cemu_status: Option<CfgStatus>,
    cemu_screen: Option<CfgStatus>,
    eden_status: Option<CfgStatus>,
    retroarch_status: Option<CfgStatus>,
    retroarch_live: crate::retroarch::Live,
    retroarch_game: Option<crate::retroarch::GameInfo>,
    error: Option<String>,
    port_notice: Option<String>,
    firewall: Option<crate::firewall::FirewallIssue>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    auto_fix_due: bool,
    fix_done: Option<(String, Instant)>,
    /// Backend de inyección activo (pie de la ventana).
    injector: Option<&'static str>,
    uinput_denied: bool,
    uinput_missing: bool,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    ax_denied: bool,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fixing: bool,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fix_failed: Option<String>,
    /// Vibración de los juegos: estado y mandos virtuales que hay.
    rumble: (crate::rumble::Status, Vec<String>),
    /// Windows: en qué punto está la instalación del driver embebido.
    rumble_setup: crate::rumble::RumbleSetup,
    /// Ajustes tal cual están (la ventana los edita sobre esta copia).
    config: crate::state::Config,
    /// Código de emparejamiento sin cámara y lo que le queda.
    pair_code: (String, Duration),
}

impl Snapshot {
    /// La foto del estado, con el candado ya cogido.
    fn of(s: &mut crate::state::Shared, rumble: (crate::rumble::Status, Vec<String>)) -> Snapshot {
        Snapshot {
            status: s.status,
            mode: s.mode,
            players: s.players.clone(),
            player_count: s.player_count(),
            pps: s.pps,
            sensor_hz: s.sensor_hz,
            rtt_hist: s.rtt_hist.iter().copied().collect(),
            dsu_clients: s.dsu_clients,
            dolphin_status: s.dolphin_cfg_status.clone(),
            cemu_status: s.cemu_cfg_status.clone(),
            cemu_screen: s.cemu_screen_status.clone(),
            eden_status: s.eden_cfg_status.clone(),
            retroarch_status: s.retroarch_cfg_status.clone(),
            retroarch_live: s.retroarch_live.clone(),
            retroarch_game: s.retroarch_game.clone(),
            error: s.last_error.clone(),
            port_notice: s.port_notice.clone(),
            firewall: s.firewall,
            auto_fix_due: s.auto_fix_due,
            fix_done: s.fix_done.clone(),
            injector: s.injector,
            uinput_denied: s.uinput_denied,
            uinput_missing: s.uinput_missing,
            ax_denied: s.ax_denied,
            fixing: s.fixing,
            fix_failed: s.fix_failed.clone(),
            rumble,
            rumble_setup: s.rumble_setup,
            config: s.config.clone(),
            pair_code: s.pair_code.current(),
        }
    }
}

impl eframe::App for PepoMoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Latido: el vigilante apunta si este hilo deja de entrar aquí
        crate::launch::mark_frame();
        theme::sync(ctx);

        // Ventana viva: egui usa el primer fotograma para medir; el segundo ya
        // está en pantalla. Marca «pintada» (a partir de ahí un fallo no se
        // relanza) y, en modo humo, programa la salida limpia
        self.frames = self.frames.saturating_add(1);
        if self.frames == 2 {
            crate::launch::mark_first_frame();
            self.painted_at = Some(Instant::now());
            crate::launch::ui_step(Step::Log);
            crate::log_line!("Ventana: primer fotograma pintado ({})", crate::launch::describe_current());
            // Windows: la marca de «intento sin pintar» fuera, y lo que ha
            // pintado queda recordado para esta gráfica
            #[cfg(windows)]
            {
                crate::launch::ui_step(Step::SaveConfig);
                crate::gpu::painted(&self.shared);
            }
            crate::launch::ui_step(Step::Egui);
        }
        crate::launch::fake_ui_hang_if_requested(self.frames);
        // Con la ventana ya pintada, `reg.exe` en su hilo (decenas de ms; sin
        // tope con algún antivirus): este hilo nunca lo espera. En 1.12 iba
        // en `new`, delante del primer fotograma; en 1.13.0, aquí pero en línea
        if self.autostart.is_none() && self.autostart_rx.is_none() && self.frames >= 2 {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = crate::threads::spawn_once("autostart-query", move || {
                let _ = tx.send(Ok(crate::autostart::is_enabled()));
            });
            self.autostart_rx = Some(rx);
        }
        self.poll_autostart();
        if let (Some(t), Some(linger)) = (self.painted_at, self.smoke) {
            if t.elapsed() >= linger {
                crate::log_line!("PEPOMOTE_SMOKE: fin, salgo con 0");
                crate::launch::exit(0);
            }
        }
        // Linux: cerrar = salir (no hay bandeja); solo queda constancia
        #[cfg(target_os = "linux")]
        if ctx.input(|i| i.viewport().close_requested()) {
            crate::log_line!("Ventana cerrada por el usuario: salgo (en Linux no hay bandeja)");
        }

        // En Windows, cerrar = esconder a la bandeja ("Salir" está en el
        // tray). La primera vez, antes, un aviso: quien lo usa por primera vez
        // cree que la X lo ha cerrado y luego lo ve en el Administrador de
        // tareas. Sin icono en la bandeja no hay desde dónde volver: minimiza
        #[cfg(windows)]
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !crate::tray::icon_present() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            } else if self.close_notice || self.tray_notice_seen {
                // con el aviso a la vista, otra X = «Entendido»
                if self.close_notice {
                    self.accept_close_notice();
                }
                hide_to_tray(ctx);
            } else {
                self.close_notice = true;
                crate::log_line!("Ventana: aviso de la bandeja a la vista (primera X)");
            }
        }
        #[cfg(windows)]
        if self.close_notice {
            self.ui_close_notice(ctx);
        }
        // En macOS, el botón rojo = minimizar (el Dock la restaura; "Salir"
        // está en la barra de menús y en ⌘Q). Nunca ocultarla: oculta no se
        // repinta y no habría forma de volver desde el Dock.
        #[cfg(target_os = "macos")]
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }

        self.refresh_ip();

        // Windows: el driver del mando virtual que faltaba al arrancar se
        // instala ahora, con la ventana pintada y delante (su permiso de
        // administrador sale entonces en primer plano)
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        crate::rumble::window_ready(self.frames >= 2, focused);

        // Fuera del candado del estado: coge el de los mandos virtuales, que
        // el hub puede tener un rato (crear un mando, sondear el driver), y
        // con el estado cogido pararía a la telemetría y al DSU
        crate::launch::ui_step(Step::Rumble);
        let rumble = crate::rumble::ui_lines();
        // El candado del estado, con tope: si otro hilo lo retiene (un
        // driver o un archivo que no vuelven), se repinta con la foto
        // anterior y el vigilante sabe en qué paso se está. Sin foto anterior
        // (los primeros fotogramas) no hay más remedio que esperar
        crate::launch::ui_step(Step::StateLock);
        let snap = match self.shared.lock_within(LOCK_WAIT) {
            Some(mut s) => {
                let snap = Snapshot::of(&mut s, rumble);
                drop(s);
                if self.lock_stall_logged {
                    crate::log_line!("Ventana: el estado compartido vuelve a estar libre");
                    self.lock_stall_logged = false;
                }
                self.last_snap = Some(snap.clone());
                snap
            }
            None => match &self.last_snap {
                Some(prev) => {
                    if !self.lock_stall_logged {
                        crate::log_line!(
                            "Ventana: el estado compartido lleva más de {} ms en manos de otro hilo; se repinta con la foto anterior",
                            LOCK_WAIT.as_millis()
                        );
                        self.lock_stall_logged = true;
                    }
                    let mut snap = prev.clone();
                    snap.rumble = rumble;
                    snap
                }
                None => Snapshot::of(&mut self.shared.lock_tolerant(), rumble),
            },
        };
        crate::launch::ui_step(Step::Egui);
        // Con móviles, 20 fps (el latido respira); en espera, 10 bastan
        ctx.request_repaint_after(Duration::from_millis(if snap.player_count > 0 { 50 } else { 100 }));

        // Idioma: ES / EN arriba a la derecha (se guarda en Ajustes)
        egui::Area::new(egui::Id::new("lang"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 12.0))
            .show(ctx, |ui| {
                let lang = i18n::current();
                let button = egui::Button::new(RichText::new(lang.code().to_uppercase()).size(11.0).color(theme::text_dim()))
                    .fill(theme::card())
                    .stroke(Stroke::new(1.0_f32, theme::card_border()));
                if ui.add(button).on_hover_text(tr!("win.lang_switch", lang.other().name())).clicked() {
                    let next = i18n::toggle();
                    let mut s = self.shared.lock_tolerant();
                    s.config.lang = Some(next.code().to_owned());
                    s.config.save();
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme::background()).inner_margin(24.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("PepoMote")
                                    .size(34.0)
                                    .strong()
                                    .color(theme::text()),
                            );
                            ui.label(
                                RichText::new(tr!("win.tagline"))
                                    .size(14.0)
                                    .color(theme::text_dim()),
                            );
                            ui.add_space(18.0);

                            // El permiso del driver en cualquier modo: en el
                            // de puntero (el de arranque) no hay tarjeta que
                            // lo cuente, y es lo que Windows está esperando
                            if snap.rumble_setup == crate::rumble::RumbleSetup::Installing {
                                ui.label(RichText::new(tr!("rumble.installing")).size(12.0).color(theme::warn()));
                                ui.add_space(10.0);
                            }

                            self.ui_repair(ui, &snap);

                            if snap.player_count == 0 {
                                self.ui_qr(ui, 280.0, tr!("win.waiting"), &snap.pair_code);
                            } else {
                                ui_players(ui, &snap);
                                if snap.mode == Mode::Dolphin {
                                    self.ui_dolphin(ui, &snap);
                                } else if snap.mode == Mode::Cemu {
                                    self.ui_cemu(ui, &snap);
                                } else if snap.mode == Mode::Switch {
                                    self.ui_switch(ui, &snap);
                                } else if snap.mode == Mode::RetroArch {
                                    self.ui_retroarch(ui, &snap);
                                } else if snap.mode == Mode::Gamepad {
                                    self.ui_gamepad(ui, &snap);
                                }
                                if snap.player_count < crate::net::MAX_PLAYERS {
                                    ui.add_space(12.0);
                                    self.ui_qr(ui, 170.0, tr!("win.another_player"), &snap.pair_code);
                                }
                            }

                            ui.add_space(10.0);
                            self.ui_settings(ui, &snap.config);

                            self.ui_repair(ui, &snap);
                            #[cfg(target_os = "macos")]
                            self.ui_mac_permissions(ui, &snap);

                            if let Some(err) = &snap.error {
                                ui.add_space(10.0);
                                ui.label(RichText::new(err).size(12.0).color(theme::error()));
                            }
                            if let Some(n) = &snap.port_notice {
                                ui.add_space(6.0);
                                ui.label(RichText::new(n).size(12.0).color(theme::warn()));
                            }

                            ui.add_space(12.0);
                            ui.label(
                                RichText::new(tr!(
                                    "win.footer",
                                    env!("CARGO_PKG_VERSION"),
                                    snap.injector.unwrap_or(tr!("win.inj_none"))
                                ))
                                .size(11.0)
                                .color(theme::text_dim()),
                            );
                        });
                    });
            });

        crate::launch::ui_step(Step::Updater);
        if let Some(version) = self.updater.frame(ctx, snap.config.update_check, snap.config.update_dismissed) {
            let mut s = self.shared.lock_tolerant();
            s.config.update_dismissed = Some(version);
            s.config.save();
        }
        if let Some(plan) = crate::update::take_prepared() {
            self.shared.lock_tolerant().config.save();
            let _ = crate::update::finish_install(&plan);
        }
        crate::launch::ui_step(Step::Egui);
        let _ = snap.status;
    }
}

/// Windows: la ventana a la bandeja (se vuelve con el icono o reabriendo el exe).
#[cfg(windows)]
fn hide_to_tray(ctx: &egui::Context) {
    crate::launch::set_window_hidden(true);
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
}

#[cfg(windows)]
impl PepoMoteApp {
    /// El aviso de la bandeja, ya visto: no vuelve a salir.
    fn accept_close_notice(&mut self) {
        self.close_notice = false;
        self.tray_notice_seen = true;
        crate::launch::ui_step(Step::SaveConfig);
        {
            let mut s = self.shared.lock_tolerant();
            s.config.tray_notice_seen = true;
            s.config.save();
        }
        crate::launch::ui_step(Step::Egui);
    }

    /// La primera X: PepoMote sigue en la bandeja para que el móvil siga
    /// conectado; «Entendido» esconde la ventana como siempre y «Salir del
    /// todo» cierra el receptor.
    fn ui_close_notice(&mut self, ctx: &egui::Context) {
        let (mut ok, mut quit) = (false, false);
        egui::Window::new(RichText::new(tr!("win.tray_notice_title")).size(15.0).strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(340.0);
                ui.label(RichText::new(tr!("win.tray_notice_body")).size(13.0).color(theme::text()));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ok = ui.button(RichText::new(tr!("win.tray_notice_ok")).size(13.0)).clicked();
                    quit = ui.button(RichText::new(tr!("win.tray_notice_quit")).size(13.0)).clicked();
                });
            });
        if ok {
            self.accept_close_notice();
            hide_to_tray(ctx);
        } else if quit {
            self.accept_close_notice();
            crate::log_line!("Ventana: «Salir del todo» en el aviso de la bandeja: salgo");
            crate::launch::exit(0);
        }
    }
}

impl PepoMoteApp {
    /// macOS: los permisos que hacen falta (Accesibilidad para mover el
    /// cursor y pulsar teclas; Grabación de pantalla solo para la doble
    /// pantalla de Cemu), con el botón que abre el panel de Ajustes. La
    /// Accesibilidad se activa sola al concederla (telemetría reintenta); la
    /// Grabación de pantalla exige reiniciar la app.
    #[cfg(target_os = "macos")]
    fn ui_mac_permissions(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        if self.ax_checked.elapsed() >= Duration::from_secs(1) {
            self.ax_checked = Instant::now();
            self.ax_ok = crate::macos::ax_trusted();
        }
        let need_ax = !self.ax_ok || snap.ax_denied;
        let need_screen = snap.mode == Mode::Cemu && !crate::macos::screen_capture_allowed();
        if !need_ax && !need_screen {
            return;
        }
        ui.add_space(10.0);
        egui::Frame::none()
            .fill(theme::card())
            .stroke(Stroke::new(1.5_f32, theme::warn()))
            .rounding(theme::RADIUS)
            .inner_margin(12.0)
            .show(ui, |ui| {
                if need_ax {
                    ui.label(RichText::new(tr!("mac.ax_needed")).size(13.0).color(theme::warn()));
                    if ui.button(RichText::new(tr!("mac.ax_open")).size(13.0)).clicked() {
                        crate::macos::open_settings_pane("Privacy_Accessibility");
                    }
                    ui.label(RichText::new(tr!("mac.ax_hint")).size(11.0).color(theme::text_dim()));
                }
                if need_screen {
                    if need_ax {
                        ui.add_space(8.0);
                    }
                    ui.label(RichText::new(tr!("mac.screen_needed")).size(13.0).color(theme::warn()));
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new(tr!("mac.screen_open")).size(13.0)).clicked() {
                            crate::macos::open_settings_pane("Privacy_ScreenCapture");
                        }
                        if ui.button(RichText::new(tr!("mac.restart")).size(13.0)).clicked() {
                            crate::macos::relaunch();
                        }
                    });
                    ui.label(RichText::new(tr!("mac.screen_hint")).size(11.0).color(theme::text_dim()));
                }
                ui.add_space(4.0);
                ui.label(RichText::new(tr!("mac.local_network")).size(11.0).color(theme::text_dim()));
            });
    }

    /// Linux: lo que el sistema necesita para que el móvil funcione (uinput
    /// para el cursor, el puerto del firewall) va ARRIBA de la ventana, con la
    /// explicación de lo que va a pedir el diálogo de contraseña antes de
    /// abrirlo (cuenta atrás, o botón), «Listo» al terminar y, sin diálogo
    /// posible (sin pkexec, sesión sin agente de polkit), los comandos para
    /// pegar en un terminal. Con el backend Wayland nunca hay uinput que
    /// pintar; en Windows y macOS los flags jamás se activan.
    fn ui_repair(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        self.ui_fix_done(ui, snap);
        let uinput_problem = snap.uinput_denied || snap.uinput_missing;
        if snap.firewall.is_none() && !uinput_problem {
            self.repair_seen_at = None;
            return;
        }
        let port = self.pairing.port;
        egui::Frame::none()
            .fill(theme::card())
            .stroke(Stroke::new(1.5_f32, theme::warn()))
            .rounding(theme::RADIUS)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width().min(412.0));
                ui.label(RichText::new(tr!("fix.title")).size(15.0).strong().color(theme::text()));
                ui.add_space(4.0);
                if let Some(fw) = &snap.firewall {
                    let text = if fw.certain {
                        tr!("fw.blocked", fw.kind.name(), port)
                    } else {
                        tr!("fw.unknown", fw.kind.name(), port)
                    };
                    ui.label(RichText::new(text).size(12.0).color(theme::warn()));
                }
                if snap.uinput_missing {
                    ui.label(RichText::new(tr!("fix.uinput_missing")).size(12.0).color(theme::warn()));
                } else if snap.uinput_denied {
                    ui.label(RichText::new(tr!("fix.uinput_denied")).size(12.0).color(theme::warn()));
                }
                #[cfg(target_os = "linux")]
                self.ui_repair_actions(ui, snap, uinput_problem, port);
            });
        ui.add_space(10.0);
    }

    /// Tarjeta verde tras una reparación con éxito (30 s o hasta «Vale»).
    fn ui_fix_done(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        let Some((text, when)) = &snap.fix_done else {
            return;
        };
        if when.elapsed() > Duration::from_secs(30) {
            self.shared.lock_tolerant().fix_done = None;
            return;
        }
        egui::Frame::none()
            .fill(theme::card())
            .stroke(Stroke::new(1.5_f32, theme::ok()))
            .rounding(theme::RADIUS)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width().min(412.0));
                ui.label(RichText::new(text).size(13.0).strong().color(theme::ok()));
                if ui.button(RichText::new(tr!("fix.ok_dismiss")).size(12.0)).clicked() {
                    self.shared.lock_tolerant().fix_done = None;
                }
            });
        ui.add_space(10.0);
    }

    /// Botones y textos de la reparación: explicación del diálogo, cuenta
    /// atrás de la automática, «Reparar ahora» / «Ahora no», y los comandos
    /// manuales cuando no hay diálogo posible.
    #[cfg(target_os = "linux")]
    fn ui_repair_actions(&mut self, ui: &mut egui::Ui, snap: &Snapshot, uinput_problem: bool, port: u16) {
        ui.add_space(6.0);
        if snap.fixing {
            ui.label(RichText::new(tr!("fix.applying")).size(12.0).color(theme::text_dim()));
            return;
        }
        if self.pkexec_ok {
            ui.label(RichText::new(tr!("fix.explain_dialog", port)).size(11.0).color(theme::text_dim()));
            ui.add_space(4.0);
            let mut fire = false;
            if snap.auto_fix_due {
                let seen = *self.repair_seen_at.get_or_insert_with(Instant::now);
                if crate::fixes::auto_fire_due(seen, Instant::now()) {
                    fire = true;
                } else {
                    let left = crate::fixes::AUTO_FIX_DELAY.saturating_sub(seen.elapsed()).as_secs() + 1;
                    ui.label(RichText::new(tr!("fix.auto_in", left)).size(12.0).color(theme::text()));
                }
            }
            ui.horizontal(|ui| {
                let label = if snap.fix_failed.is_some() { tr!("fix.retry") } else { tr!("fix.now") };
                if ui.button(RichText::new(label).size(14.0)).clicked() {
                    fire = true;
                }
                if snap.auto_fix_due && ui.button(RichText::new(tr!("fix.not_now")).size(12.0)).clicked() {
                    let mut s = self.shared.lock_tolerant();
                    s.auto_fix_due = false;
                    s.config.fix_attempted = true;
                    s.config.save();
                    self.repair_seen_at = None;
                }
            });
            if fire {
                self.repair_seen_at = None;
                crate::fixes::fix_all(self.shared.clone(), port);
            }
            match &snap.fix_failed {
                Some(why) => {
                    ui.label(RichText::new(why).size(11.0).color(theme::warn()));
                }
                None => {
                    ui.label(RichText::new(tr!("fix.dialog_once")).size(11.0).color(theme::text_dim()));
                }
            }
        }
        // Sin pkexec, o con el diálogo fallando (sesión sin agente de
        // polkit): los comandos manuales, listos para copiar
        if !self.pkexec_ok || snap.fix_failed.is_some() {
            ui.add_space(6.0);
            let intro = if self.pkexec_ok { tr!("fix.manual_no_dialog") } else { tr!("fix.manual_no_pkexec") };
            ui.label(RichText::new(intro).size(11.0).color(theme::text_dim()));
            if uinput_problem {
                self.ui_manual_cmd(ui, "uinput", crate::fixes::UINPUT_MANUAL_CMD);
                ui.label(RichText::new(tr!("fix.relogin")).size(11.0).color(theme::text_dim()));
            }
            if let Some(fw) = &snap.firewall {
                ui.label(RichText::new(tr!("fix.manual_firewall")).size(11.0).color(theme::text_dim()));
                self.ui_manual_cmd(ui, "firewall", &crate::fixes::firewall_manual_cmd(fw.kind, port));
            }
        }
    }

    /// Un comando en monoespaciada con su botón «Copiar comando».
    #[cfg(target_os = "linux")]
    fn ui_manual_cmd(&mut self, ui: &mut egui::Ui, id: &str, cmd: &str) {
        ui.label(RichText::new(cmd).monospace().size(11.0).color(theme::text()));
        ui.horizontal(|ui| {
            if ui.button(RichText::new(tr!("fix.copy_cmd")).size(12.0)).clicked() {
                ui.output_mut(|o| o.copied_text = cmd.to_owned());
                self.copied_at = Some((id.to_owned(), Instant::now()));
            }
            if self
                .copied_at
                .as_ref()
                .is_some_and(|(i, t)| i == id && t.elapsed() < Duration::from_secs(2))
            {
                ui.label(RichText::new(tr!("fix.copied")).size(11.0).color(theme::ok()));
            }
        });
    }

    fn ui_qr(&self, ui: &mut egui::Ui, size: f32, caption: &str, pair_code: &(String, Duration)) {
        draw_qr_card(ui, &self.qr_modules, self.qr_width, size);
        ui.add_space(8.0);
        ui.label(RichText::new(caption).size(13.0).color(theme::text_dim()));
        ui.label(
            RichText::new(format!("{} : {}", self.pairing.host, self.pairing.port))
                .size(11.0)
                .color(theme::text_dim()),
        );
        // Sin cámara (Linux móvil): código de 4 dígitos, un solo uso, 120 s
        let (code, left) = pair_code;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("win.no_camera_code")).size(12.0).color(theme::text_dim()));
            ui.label(RichText::new(code).size(20.0).strong().color(theme::text()));
            ui.label(
                RichText::new(format!("({} s)", left.as_secs()))
                    .size(11.0)
                    .color(theme::text_dim()),
            );
        });
    }

    fn ui_dolphin(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.add_space(8.0);
        ui.label(
            RichText::new(tr!("win.dolphin_clients", snap.dsu_clients))
                .size(13.0)
                .color(theme::blue()),
        );
        if ui
            .button(RichText::new(tr!("win.configure_dolphin")).size(13.0))
            .clicked()
        {
            crate::dolphin::configure_now(&self.shared);
        }
        if let Some(st) = &snap.dolphin_status {
            let color = if st.ok { theme::ok() } else { theme::warn() };
            ui.label(RichText::new(&st.text).size(12.0).color(color));
        }
        self.ui_rumble(ui, snap);
        ui.label(
            RichText::new(tr!("win.dolphin_help"))
            .size(11.0)
            .color(theme::text_dim()),
        );
    }

    /// Vibración de los juegos: estado del mando virtual y cómo arreglarlo
    /// (en Windows, el driver; en Linux, el permiso de uinput).
    fn ui_rumble(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        let (st, pads) = &snap.rumble;
        let color = match st {
            crate::rumble::Status::Ready => theme::ok(),
            // los primeros milisegundos, sin resultado aún: nada que avisar
            crate::rumble::Status::Checking => theme::text_dim(),
            _ => theme::warn(),
        };
        ui.label(RichText::new(crate::rumble::status_text(*st)).size(12.0).color(color));
        for l in pads {
            ui.label(RichText::new(l).size(11.0).color(theme::text_dim()));
        }
        self.ui_driver_setup(ui, snap);
    }

    /// Windows: el driver del mando virtual viene dentro del exe y se
    /// instala solo al arrancar. Si falta, aquí se cuenta en qué punto está
    /// (instalando, cancelado, fallido) y queda el botón para instalarlo.
    fn ui_driver_setup(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        use crate::rumble::{RumbleSetup, Status};
        // También con el driver colgado: el instalador repara la instalación
        if !matches!(snap.rumble.0, Status::NeedsDriver | Status::Unresponsive) {
            return;
        }
        match snap.rumble_setup {
            // ya lo cuenta la línea de arriba, bajo el título
            RumbleSetup::Installing => return,
            RumbleSetup::Declined => {
                ui.label(RichText::new(tr!("rumble.declined")).size(11.0).color(theme::text_dim()));
            }
            RumbleSetup::Failed(code) => {
                ui.label(RichText::new(tr!("rumble.install_failed", code)).size(11.0).color(theme::text_dim()));
            }
            RumbleSetup::Idle | RumbleSetup::Installed => {}
        }
        if ui.button(RichText::new(tr!("rumble.install")).size(13.0)).clicked() {
            crate::rumble::install_driver_now();
        }
    }

    /// Mando universal. Aquí el mando virtual no es un accesorio para la
    /// vibración: **es** la salida del móvil, así que el estado se cuenta en
    /// esos términos y no en los de `ui_rumble`. No hay botón de configurar
    /// nada porque no hay nada que configurar: el mando lo crea el sistema.
    fn ui_gamepad(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.add_space(8.0);
        let (st, pads) = &snap.rumble;
        match *st {
            crate::rumble::Status::Ready => {
                ui.label(RichText::new(tr!("win.gamepad_ready")).size(13.0).color(theme::ok()));
            }
            crate::rumble::Status::NeedsDriver => {
                ui.label(RichText::new(tr!("win.gamepad_driver")).size(13.0).color(theme::warn()));
            }
            _ => {
                ui.label(RichText::new(tr!("win.gamepad_nopad")).size(13.0).color(theme::warn()));
                ui.label(
                    RichText::new(crate::rumble::status_text(*st))
                        .size(11.0)
                        .color(theme::text_dim()),
                );
            }
        }
        for l in pads {
            ui.label(RichText::new(l).size(11.0).color(theme::text_dim()));
        }
        self.ui_driver_setup(ui, snap);
        ui.label(
            RichText::new(tr!("win.gamepad_help"))
                .size(11.0)
                .color(theme::text_dim()),
        );
    }

    fn ui_cemu(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.add_space(8.0);
        ui.label(
            RichText::new(tr!("win.cemu_clients", snap.dsu_clients))
                .size(13.0)
                .color(theme::blue()),
        );
        if ui
            .button(RichText::new(tr!("win.configure_cemu")).size(13.0))
            .clicked()
        {
            crate::cemu::configure_now(&self.shared);
        }
        if let Some(st) = &snap.cemu_status {
            let color = if st.ok { theme::ok() } else { theme::warn() };
            ui.label(RichText::new(&st.text).size(12.0).color(color));
        }
        if let Some(st) = &snap.cemu_screen {
            let color = if st.ok { theme::ok() } else { theme::text_dim() };
            ui.label(RichText::new(&st.text).size(12.0).color(color));
        }
        self.ui_rumble(ui, snap);
        ui.label(
            RichText::new(tr!("win.cemu_help"))
                .size(11.0)
                .color(theme::text_dim()),
        );
    }

    fn ui_switch(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.add_space(8.0);
        ui.label(RichText::new(tr!("win.switch_clients",snap.dsu_clients)).size(13.0).color(theme::blue()));
        ui.horizontal_wrapped(|ui| {
            if ui.button(RichText::new(tr!("win.configure_eden")).size(13.0)).clicked() {
                crate::eden::configure_now(&self.shared);
            }
            if ui.button(RichText::new(tr!("win.restore_eden")).size(13.0)).clicked() {
                crate::eden::restore_now(&self.shared);
            }
        });
        if let Some(st)=&snap.eden_status {
            ui.label(RichText::new(&st.text).size(12.0).color(if st.ok {theme::ok()} else {theme::warn()}));
        }
        ui.label(RichText::new(tr!("win.switch_help")).size(11.0).color(theme::text_dim()));
    }

    fn ui_retroarch(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.add_space(8.0);
        // Estado vivo: responde (y qué corre), abierto pero mudo, o cerrado
        let live = &snap.retroarch_live;
        let (text, color) = if live.reachable {
            let version = live.version.clone().unwrap_or_default();
            let polls = live.polls_per_sec.round() as i64;
            // Qué corre solo se sabe en versiones posteriores a la 1.22.2 (a
            // las demás no se les pregunta: se cierran con GET_STATUS)
            let text = match &live.activity {
                Some(crate::retroarch::Activity::Playing { core, content }) => {
                    tr!("win.retroarch_live", version, tr!("win.retroarch_playing", content, core), polls)
                }
                Some(crate::retroarch::Activity::Paused { core, content }) => {
                    tr!("win.retroarch_live", version, tr!("win.retroarch_paused", content, core), polls)
                }
                Some(crate::retroarch::Activity::Contentless) => {
                    tr!("win.retroarch_live", version, tr!("win.retroarch_menu"), polls)
                }
                None => tr!("win.retroarch_live_short", version, polls),
            };
            (text, theme::ok())
        } else if live.degraded {
            (tr!("win.retroarch_degraded").to_owned(), theme::warn())
        } else {
            (tr!("win.retroarch_waiting").to_owned(), theme::blue())
        };
        ui.label(RichText::new(text).size(13.0).color(color));
        // Último juego del historial: el móvil enseña el mando de su consola
        if let Some(g) = &snap.retroarch_game {
            let text = if g.console.is_some() {
                tr!("win.retroarch_game", g.title, g.system, g.core)
            } else {
                tr!("win.retroarch_game_generic", g.title, g.core)
            };
            ui.label(RichText::new(text).size(12.0).color(theme::text_dim()));
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(RichText::new(tr!("win.configure_retroarch")).size(13.0)).clicked() {
                crate::retroarch::configure_now(&self.shared);
            }
            if ui.button(RichText::new(tr!("win.restore_retroarch")).size(13.0)).clicked() {
                crate::retroarch::restore_now(&self.shared);
            }
        });
        if let Some(st) = &snap.retroarch_status {
            ui.label(RichText::new(&st.text).size(12.0).color(if st.ok { theme::ok() } else { theme::warn() }));
        }
        ui.label(RichText::new(tr!("win.retroarch_help")).size(11.0).color(theme::text_dim()));
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui, current: &crate::state::Config) {
        let mut config = current.clone();
        let before = config.clone();

        egui::CollapsingHeader::new(
            RichText::new(tr!("cfg.title")).size(14.0).color(theme::text_dim()),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("cfg.sensitivity")).size(13.0).color(theme::text_dim()));
                ui.add(
                    egui::Slider::new(&mut config.sens_deg, 15.0..=60.0)
                        .suffix("°")
                        .fixed_decimals(0),
                );
            });
            ui.label(
                RichText::new(tr!("cfg.sens_help"))
                    .size(11.0)
                    .color(theme::text_dim()),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut config.abs_mode, RichText::new(tr!("cfg.abs")).size(13.0));
                info_icon(ui, tr!("cfg.abs_help"));
            });
            ui.add_space(4.0);
            ui.checkbox(
                &mut config.auto_dolphin,
                RichText::new(tr!("cfg.auto_dolphin")).size(13.0),
            );
            ui.add_space(4.0);
            ui.checkbox(
                &mut config.auto_cemu,
                RichText::new(tr!("cfg.auto_cemu")).size(13.0),
            );
            ui.add_space(4.0);
            ui.checkbox(&mut config.auto_eden, RichText::new(tr!("cfg.auto_eden")).size(13.0));
            ui.add_space(4.0);
            ui.checkbox(&mut config.auto_retroarch, RichText::new(tr!("cfg.auto_retroarch")).size(13.0));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut config.auto_mode,
                    RichText::new(tr!("cfg.auto_mode")).size(13.0),
                );
                info_icon(ui, tr!("cfg.auto_mode_help"));
            });
            ui.add_enabled_ui(config.auto_mode, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(
                        &mut config.return_to_pointer,
                        RichText::new(tr!("cfg.return_to_pointer")).size(13.0),
                    );
                    info_icon(ui, tr!("cfg.return_to_pointer_help"));
                });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("cfg.cemu_folder")).size(13.0).color(theme::text_dim()));
                ui.add(
                    egui::TextEdit::singleline(&mut config.cemu_dir)
                        .desired_width(200.0)
                        .hint_text(tr!("cfg.auto_hint")),
                );
                if ui.button(RichText::new(tr!("cfg.detect")).size(12.0)).clicked() {
                    crate::cemu::detect_now(&self.shared);
                }
            });
            ui.label(
                RichText::new(tr!("cfg.cemu_folder_help"))
                    .size(11.0)
                    .color(theme::text_dim()),
            );
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr!("cfg.eden_folder")).size(13.0).color(theme::text_dim()));
                ui.add(egui::TextEdit::singleline(&mut config.eden_dir).desired_width(200.0).hint_text(tr!("cfg.auto_hint")));
                if ui.button(RichText::new(tr!("cfg.detect")).size(12.0)).clicked() {crate::eden::detect_now(&self.shared);}
            });
            ui.label(RichText::new(tr!("cfg.eden_folder_help")).size(11.0).color(theme::text_dim()));
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr!("cfg.retroarch_folder")).size(13.0).color(theme::text_dim()));
                ui.add(egui::TextEdit::singleline(&mut config.retroarch_dir).desired_width(200.0).hint_text(tr!("cfg.auto_hint")));
                if ui.button(RichText::new(tr!("cfg.detect")).size(12.0)).clicked() {crate::retroarch::detect_now(&self.shared);}
            });
            ui.label(RichText::new(tr!("cfg.retroarch_folder_help")).size(11.0).color(theme::text_dim()));
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("cfg.dolphin_folder")).size(13.0).color(theme::text_dim()));
                ui.add(
                    egui::TextEdit::singleline(&mut config.dolphin_dir)
                        .desired_width(200.0)
                        .hint_text(tr!("cfg.auto_hint")),
                );
                if ui.button(RichText::new(tr!("cfg.detect")).size(12.0)).clicked() {
                    crate::dolphin::detect_now(&self.shared);
                }
            });
            ui.label(
                RichText::new(tr!("cfg.dolphin_folder_help"))
                    .size(11.0)
                    .color(theme::text_dim()),
            );
            // Linux y macOS con varios monitores: a cuál apunta el móvil
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            {
                let screens = self.shared.lock_tolerant().screens.clone();
                if screens.len() > 1 {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr!("cfg.pointing_on")).size(13.0).color(theme::text_dim()));
                        let current = if config.screen.is_empty() {
                            tr!("cfg.all_screens").to_owned()
                        } else {
                            config.screen.clone()
                        };
                        egui::ComboBox::from_id_salt("pantalla_apuntado")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut config.screen, String::new(), tr!("cfg.all_screens"));
                                for (name, w, h) in &screens {
                                    ui.selectable_value(&mut config.screen, name.clone(), tr!("cfg.only_screen", name, w, h));
                                }
                            });
                    });
                    ui.label(
                        RichText::new(tr!("cfg.screens_help"))
                            .size(11.0)
                            .color(theme::text_dim()),
                    );
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("cfg.theme")).size(13.0).color(theme::text_dim()));
                egui::ComboBox::from_id_salt("tema")
                    .selected_text(theme_label(config.theme))
                    .show_ui(ui, |ui| {
                        for p in theme::ThemePref::ALL {
                            ui.selectable_value(&mut config.theme, p, theme_label(p));
                        }
                    });
            });
            if config.theme != before.theme {
                theme::set_preference(ui.ctx(), config.theme);
            }
            ui.add_space(4.0);
            // Sin respuesta de reg.exe todavía (al arrancar, o tras un
            // cambio): la casilla espera
            let busy = self.autostart_rx.is_some();
            let mut auto = self.autostart.unwrap_or(false);
            ui.add_enabled(
                self.autostart.is_some() && !busy,
                egui::Checkbox::new(&mut auto, RichText::new(tr!("cfg.autostart")).size(13.0)),
            );
            if let Some(before_auto) = self.autostart {
                if auto != before_auto && !busy {
                    // reg.exe en su hilo: la casilla cambia ya y, si falla,
                    // vuelve atrás con el motivo (`poll_autostart`)
                    self.autostart = Some(auto);
                    let (tx, rx) = std::sync::mpsc::channel();
                    let _ = crate::threads::spawn_once("autostart-set", move || {
                        let _ = tx.send(
                            crate::autostart::set_enabled(auto).map(|()| auto).map_err(|why| (before_auto, why)),
                        );
                    });
                    self.autostart_rx = Some(rx);
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut config.update_check,
                    RichText::new(tr!("cfg.update_check")).size(13.0),
                );
                info_icon(ui, tr!("cfg.update_check_help"));
            });
            self.updater.settings(ui);
        });

        if config != before {
            // En caliente para el puntero ya; a disco cuando sueltes el
            // slider (arrastrarlo escribía el archivo en cada frame)
            self.shared.lock_tolerant().config = config.clone();
            self.config_dirty = true;
        }
        if self.config_dirty && !ui.input(|i| i.pointer.any_down()) {
            crate::launch::ui_step(Step::SaveConfig);
            config.save();
            crate::launch::ui_step(Step::Egui);
            self.config_dirty = false;
        }
    }
}

fn theme_label(p: theme::ThemePref) -> &'static str {
    match p {
        theme::ThemePref::System => tr!("cfg.theme_system"),
        theme::ThemePref::Light => tr!("cfg.theme_light"),
        theme::ThemePref::Dark => tr!("cfg.theme_dark"),
    }
}

/// Circulito con una «i»: al pasar el ratón por encima enseña `text`.
fn info_icon(ui: &mut egui::Ui, text: &str) {
    let size = 15.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let color = if resp.hovered() { theme::blue() } else { theme::text_dim() };
    let painter = ui.painter();
    painter.circle_stroke(rect.center(), size / 2.0 - 1.0, Stroke::new(1.3_f32, color));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "i",
        egui::FontId::proportional(11.0),
        color,
    );
    resp.on_hover_text(text);
}

/// Alfa del punto de latido: respira a 1 Hz entre 89 y 255.
fn breath_alpha(t: f64) -> u8 {
    let s = 0.5 - 0.5 * (t * std::f64::consts::TAU).cos();
    (89.0 + s * 166.0).round() as u8
}

/// Los últimos `cap` valores normalizados a 0..1 contra el máximo (o contra
/// `floor_ms` si es mayor: que un RTT de 5 ms no parezca una montaña).
fn sparkline_norm(values: &[f32], cap: usize, floor_ms: f32) -> Vec<f32> {
    let tail = &values[values.len().saturating_sub(cap)..];
    let max = tail.iter().copied().fold(floor_ms, f32::max);
    tail.iter().map(|v| (v / max).clamp(0.0, 1.0)).collect()
}

/// Sparkline (80×16) del RTT de los últimos latidos del Jugador 1.
fn sparkline(ui: &mut egui::Ui, values: &[f32]) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(80.0, 16.0), egui::Sense::hover());
    let norm = sparkline_norm(values, crate::state::RTT_HIST, 20.0);
    if norm.len() < 2 {
        return;
    }
    let n = norm.len() as f32;
    let pts: Vec<Pos2> = norm
        .iter()
        .enumerate()
        .map(|(i, v)| {
            Pos2::new(
                rect.left() + rect.width() * i as f32 / (n - 1.0),
                rect.bottom() - 1.0 - v * (rect.height() - 2.0),
            )
        })
        .collect();
    ui.painter().add(egui::Shape::line(pts, Stroke::new(1.2_f32, theme::blue())));
    let last = values.last().copied().unwrap_or(0.0);
    resp.on_hover_text(tr!("win.heartbeat_tip", last.round() as i64, values.len().min(crate::state::RTT_HIST)));
}

fn ui_players(ui: &mut egui::Ui, snap: &Snapshot) {
    let card_w = 340.0f32.min(ui.available_width());
    let row_h = 34.0;
    let head_h = 62.0;
    let card_h = head_h + row_h * snap.player_count as f32 + 14.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(card_w, card_h), egui::Sense::hover());
    ui.painter().rect(
        rect,
        Rounding::same(theme::RADIUS),
        theme::card(),
        Stroke::new(1.5_f32, theme::card_border()),
    );

    let mut child = ui.child_ui(
        rect.shrink(16.0),
        egui::Layout::top_down(egui::Align::Min),
        None,
    );
    let mode_txt = match snap.mode {
        Mode::Pointer => tr!("win.mode_pointer"),
        Mode::Dolphin => tr!("win.mode_dolphin"),
        Mode::Cemu => tr!("win.mode_cemu"),
        Mode::Switch => tr!("win.mode_switch"),
        Mode::RetroArch => tr!("win.mode_retroarch"),
        Mode::Gamepad => tr!("win.mode_gamepad"),
    };
    let cemu = snap.mode == Mode::Cemu;
    let cemu_layout = crate::state::cemu_layout(&snap.players);
    child.label(RichText::new(mode_txt).size(13.0).color(theme::blue()));
    child.horizontal(|ui| {
        // Latido: un punto verde que respira mientras llegan paquetes
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
        let color = if snap.pps > 0.0 {
            let c = theme::ok();
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), breath_alpha(ui.input(|i| i.time)))
        } else {
            theme::text_dim()
        };
        ui.painter().circle_filled(dot.center(), 4.0, color);
        ui.label(
            RichText::new(tr!("win.pps", snap.pps.round() as i64, snap.sensor_hz.round() as i64))
                .size(11.0)
                .color(theme::text_dim()),
        );
        if snap.rtt_hist.len() >= 2 {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| sparkline(ui, &snap.rtt_hist));
        }
    });
    child.add_space(4.0);

    let layout = crate::state::player_layout(&snap.players);
    for (i, slot) in snap.players.iter().enumerate() {
        let Some(p) = slot else { continue };
        let pad_state = crate::state::pad_state(snap.mode,&snap.players,i as u8);
        let number = pad_state.player;
        let badge = if snap.mode==Mode::Switch {
            if p.role==crate::state::Role::Nunchuk {tr!("win.badge_nunchuk_switch") .to_owned()}
            else {tr!("win.badge_switch_pro", number)}
        } else if snap.mode == Mode::RetroArch {
            if p.role == crate::state::Role::Nunchuk {
                tr!("win.badge_nunchuk_retroarch").to_owned()
            } else {
                match p.retro_pad {
                    crate::retroarch::RetroPadKind::RetroPad => match p.retro_layout {
                        Some(l) if l != "retropad" => tr!("win.badge_retro_console", number, crate::retroarch::layout_name(l)),
                        _ => tr!("win.badge_retropad", number),
                    },
                    crate::retroarch::RetroPadKind::Nes => tr!("win.badge_retro_nes", number),
                    crate::retroarch::RetroPadKind::Gun => tr!("win.badge_retro_gun", number),
                }
            }
        } else if p.role == crate::state::Role::Nunchuk {
            if cemu && !cemu_layout.iter().any(|c| c.nunchuk_slot == Some(i as u8)) {
                tr!("win.badge_nunchuk_unused", number)
            } else if !cemu && !layout.iter().any(|(_, n)| *n == Some(i as u8)) {
                // su jugador lleva el Nunchuk en el mismo móvil
                tr!("win.badge_nunchuk_spare", number)
            } else {
                tr!("win.badge_nunchuk", number)
            }
        } else if !cemu && p.own_nunchuk {
            tr!("win.badge_player_nunchuk", number)
        } else if cemu {
            match cemu_layout.iter().find(|c| c.dsu_slot == Some(i as u8)).map(|c| (c.kind, c.screen_only)) {
                Some((crate::state::PadKind::GamePad, true)) => tr!("win.badge_screen_only", number),
                Some((crate::state::PadKind::GamePad, false)) => tr!("win.badge_gamepad", number),
                Some((crate::state::PadKind::Pro, _)) => tr!("win.badge_pro", number),
                Some((crate::state::PadKind::Wiimote, _)) => tr!("win.badge_wiimote", number),
                None => tr!("win.badge_player", number),
            }
        } else {
            tr!("win.badge_player", number)
        };
        child.horizontal(|ui| {
            ui.label(
                RichText::new(badge)
                    .size(13.0)
                    .strong()
                    .color(theme::ON_ACCENT)
                    .background_color(theme::blue()),
            );
            let name = if p.model.is_empty() {
                p.name.clone()
            } else {
                p.model.clone()
            };
            ui.label(RichText::new(name).size(13.0).color(theme::text()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let rtt = p
                    .rtt_ms
                    .map(|r| format!("{r:.0} ms"))
                    .unwrap_or_else(|| "—".into());
                ui.label(
                    RichText::new(format!("{}% · {rtt}", p.battery_pct))
                        .size(12.0)
                        .color(theme::text_dim()),
                );
                // Apunta por inclinación (sin giroscopio real): que se sepa
                if p.tilt && p.role == crate::state::Role::Wiimote {
                    ui.label(RichText::new(tr!("win.tilt")).size(12.0).color(theme::text_dim()))
                        .on_hover_text(tr!("win.tilt_tip"));
                }
            });
        });
    }
}

fn build_qr(url: &str) -> (Vec<bool>, usize) {
    match qrcode::QrCode::new(url.as_bytes()) {
        Ok(code) => {
            let width = code.width();
            let modules = code
                .to_colors()
                .into_iter()
                .map(|c| c == qrcode::Color::Dark)
                .collect();
            (modules, width)
        }
        Err(_) => (vec![], 0),
    }
}

fn draw_qr_card(ui: &mut egui::Ui, modules: &[bool], width: usize, size: f32) {
    let card_size = ui.available_width().clamp(120.0, size);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(card_size), egui::Sense::hover());
    let painter = ui.painter();

    // El QR va SIEMPRE claro (módulos oscuros sobre blanco): con el tema
    // oscuro invertido no lo lee cualquier cámara
    painter.rect(
        rect,
        Rounding::same(theme::RADIUS),
        theme::LIGHT.card,
        Stroke::new(1.5_f32, theme::LIGHT.card_border),
    );

    if width == 0 {
        return;
    }

    let quiet = 4;
    let total = width + quiet * 2;
    let module = (card_size - 20.0) / total as f32;
    let origin = Pos2::new(
        rect.min.x + 10.0 + quiet as f32 * module,
        rect.min.y + 10.0 + quiet as f32 * module,
    );

    for y in 0..width {
        for x in 0..width {
            if modules[y * width + x] {
                let min = Pos2::new(origin.x + x as f32 * module, origin.y + y as f32 * module);
                painter.rect_filled(
                    Rect::from_min_size(min, Vec2::splat(module + 0.5)),
                    0.0,
                    theme::LIGHT.text,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_latido_respira_entre_89_y_255_a_un_hercio() {
        assert_eq!(breath_alpha(0.0), 89);
        assert_eq!(breath_alpha(0.5), 255);
        assert_eq!(breath_alpha(1.0), 89);
        for i in 0..=100 {
            let a = breath_alpha(i as f64 / 100.0);
            assert!((89..=255).contains(&a), "t={i}: {a}");
        }
    }

    #[test]
    fn la_sparkline_normaliza_con_suelo_y_se_queda_con_los_ultimos() {
        assert!(sparkline_norm(&[], 60, 20.0).is_empty());
        assert_eq!(sparkline_norm(&[5.0, 10.0], 60, 20.0), vec![0.25, 0.5]);
        let n = sparkline_norm(&[10.0, 40.0], 60, 20.0);
        assert!((n[0] - 0.25).abs() < 1e-6 && (n[1] - 1.0).abs() < 1e-6, "por encima del suelo manda el máximo");
        let many: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let n = sparkline_norm(&many, 60, 20.0);
        assert_eq!(n.len(), 60);
        assert!((n[59] - 1.0).abs() < 1e-6);
        assert!((n[0] - 40.0 / 99.0).abs() < 1e-6, "empieza en el valor 40");
    }
}
