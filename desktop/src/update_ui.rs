//! The same small update dialog is used by the receiver and Linux phone app.
use super::*;

fn text(es: &'static str, en: &'static str) -> &'static str {
    if crate::i18n::current() == crate::i18n::Lang::Es {
        es
    } else {
        en
    }
}

#[derive(Default)]
pub struct UpdateUi {
    open: bool,
    seen: Option<Version>,
    receipt: Option<String>,
    receipt_read: bool,
    #[cfg(test)]
    preview: bool,
}

impl UpdateUi {
    fn preview(&self) -> bool {
        #[cfg(test)]
        {
            return self.preview;
        }
        #[cfg(not(test))]
        {
            false
        }
    }
    pub fn settings(&mut self, ui: &mut egui::Ui) {
        let snapshot = snapshot();
        let available = snapshot
            .release
            .as_ref()
            .is_some_and(|m| m.version() > Version::current());
        let label = if snapshot.phase.busy() {
            text("Ver progreso de actualización…", "View update progress…")
        } else if available {
            text("Ver actualización disponible…", "View available update…")
        } else {
            text("Buscar actualizaciones…", "Check for updates…")
        };
        if ui.button(label).clicked() {
            self.open = true;
            if !available && !self.preview() {
                request_check();
            }
        }
    }
    /// Returns the version whose automatic offer was shown, for persistence.
    /// Explicit access from settings remains available after dismissing it.
    pub fn frame(
        &mut self,
        ctx: &egui::Context,
        automatic: bool,
        dismissed: Option<Version>,
    ) -> Option<Version> {
        install::confirm_health();
        let snapshot = snapshot();
        let mut announced = None;
        if automatic {
            if let Some(version) = snapshot.release.as_ref().map(|m| m.version()).filter(|v| {
                *v > Version::current() && Some(*v) != dismissed && Some(*v) != self.seen
            }) {
                self.seen = Some(version);
                announced = Some(version);
                self.open = true;
                crate::launch::set_window_hidden(false);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                    egui::UserAttentionType::Critical,
                ));
            }
        }
        if OPEN_REQUEST.swap(false, Ordering::Relaxed) {
            self.open = true;
            if snapshot.release.is_none() && !self.preview() {
                request_check();
            }
        }
        if !self.receipt_read {
            if let Some(path) = std::env::var_os("PEPOMOTE_UPDATE_RESULT") {
                if let Ok(receipt) = std::fs::read_to_string(&path) {
                    // A verified copy travels with the installation receipt,
                    // so notes remain available immediately after restart,
                    // even with automatic checks off or the network offline.
                    if let Some(parent) = std::path::Path::new(&path).parent() {
                        if let Ok(file) = std::fs::File::open(parent.join("release.json")) {
                            let mut bytes = Vec::new();
                            if file
                                .take(manifest::MAX_MANIFEST + 1)
                                .read_to_end(&mut bytes)
                                .is_ok()
                            {
                                if let Ok(release) = manifest::Manifest::parse(&bytes) {
                                    let mut state = locked();
                                    if state.snapshot.release.is_none() {
                                        state.snapshot.release = Some(release);
                                    }
                                }
                            }
                        }
                    }
                    self.receipt = Some(receipt);
                    self.receipt_read = true;
                    self.open = true;
                }
            } else {
                self.receipt_read = true;
            }
        }
        if !self.open {
            return announced;
        }
        let mut open = self.open;
        let mut later = false;
        let width = (ctx.screen_rect().width() - 48.0).clamp(230.0, 400.0);
        egui::Window::new(text("Actualización de PepoMote","PepoMote update"))
            .id(egui::Id::new("pepomote-updater"))
            .open(&mut open).collapsible(false).resizable(false)
            .default_width(width).anchor(egui::Align2::CENTER_CENTER,egui::Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx,|ui|{
                if let Some(receipt)=&self.receipt {
                    let message=match receipt.trim() {
                        "installed"=>text("La actualización está instalada y funcionando.","The update is installed and running."),
                        "rolled-back"=>text("La actualización no pudo arrancar. Se ha recuperado la versión anterior. Puedes reintentarlo.","The update could not start. The previous version has been restored. You can try again."),
                        _=>text("No se pudo terminar la recuperación. Conserva la copia anterior y consulta los detalles de la actualización.","Recovery could not finish. Keep the previous copy and check the update details."),
                    };
                    ui.label(message);ui.separator();
                }
                let release=snapshot.release.as_ref().filter(|m|m.version()>=Version::current());
                let available=release.is_some_and(|m|m.version()>Version::current());
                if let Some(release)=release {
                    ui.heading(format!("{} {}",if available{text("Nueva versión","New version")}else{text("Novedades de tu versión","Your version's release notes")},release.version));
                    if available {ui.label(text("Hay una nueva versión. ¿Quieres instalarla?","A new version is available. Would you like to install it?"));}
                    ui.label(format!("{} {}",text("Tu versión:","Your version:"),Version::current()));
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(text("Novedades","What's new")).strong());
                    let notes=if crate::i18n::current()==crate::i18n::Lang::Es{&release.notes.es}else{&release.notes.en};
                    egui::ScrollArea::vertical().max_height(180.0).show(ui,|ui|{for note in notes{ui.label(format!("• {note}"));}});
                    ui.add_space(10.0);
                    if available {match &snapshot.phase {
                        Phase::Downloading{received,total}=>{
                            ui.label(text("Descargando y verificando…","Downloading and verifying…"));
                            ui.add(egui::ProgressBar::new(if *total>0{*received as f32 / *total as f32}else{0.0}).show_percentage());
                            if ui.button(text("Cancelar","Cancel")).clicked(){cancel_download();}
                        }
                        Phase::Preparing|Phase::Ready=>{ui.spinner();ui.label(text("Preparando el reinicio seguro…","Preparing a safe restart…"));}
                        _=>{
                            let target=install::Runtime::current().and_then(|r|install::select_target(&r));
                            let can_install=target.as_ref().ok().is_some_and(|t|release.assets.contains_key(t.key));
                            if can_install {
                                ui.label(text("PepoMote se reiniciará. Tus emparejamientos y ajustes se conservarán.","PepoMote will restart. Your pairings and settings will be kept."));
                                let label=if matches!(snapshot.phase,Phase::Failed(_)|Phase::Cancelled){text("Reintentar instalación","Retry installation")}else{text("Instalar actualización","Install update")};
                                if ui.add_enabled(!snapshot.phase.busy() && !self.preview(),egui::Button::new(label)).clicked(){start_download();}
                            } else {
                                ui.label(if matches!(target,Err(ref e) if e=="managed") {
                                    text("Esta instalación la gestiona el sistema. Actualízala con su gestor de paquetes o instala la nueva versión siguiendo las instrucciones de la descarga.","This installation is managed by the system. Update it with its package manager or follow the release installation instructions.")
                                }else{text("No hay instalación automática para este formato. Abre la descarga y sigue las instrucciones de tu sistema.","Automatic installation is unavailable for this format. Open the release and follow the instructions for your system.")});
                            }
                        }
                    }}
                    ui.add_space(6.0);
                    ui.hyperlink_to(text("Ver la versión en GitHub","View release on GitHub"),&release.release_url);
                }
                match &snapshot.phase {
                    Phase::Checking=>{ui.spinner();ui.label(text("Buscando actualizaciones…","Checking for updates…"));}
                    Phase::Current=>{ui.label(text("Estás usando la última versión publicada.","You are using the latest published version."));}
                    Phase::Failed(error)=>{
                        ui.colored_label(egui::Color32::from_rgb(210,95,55),text("No se pudo completar la actualización. Puedes reintentarlo.","The update could not be completed. You can try again."));
                        ui.collapsing(text("Detalles","Details"),|ui|{ui.label(error);});
                    }
                    Phase::Cancelled=>{ui.label(text("Descarga cancelada. Tu instalación sigue igual.","Download cancelled. Your installation is unchanged."));}
                    _=>{}
                }
                if !snapshot.phase.busy() {
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui|{
                        if ui.add_enabled(!self.preview(),egui::Button::new(text("Buscar de nuevo","Check again"))).clicked(){request_check();}
                        if ui.button(text("Más tarde","Later")).clicked(){later=true;}
                    });
                    if release.is_none(){ui.hyperlink_to(text("Abrir las descargas","Open downloads"),LATEST_URL);}
                }
            });
        self.open = open && !later;
        announced
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "manual, isolated UI review; no network or installation"]
    fn preview_update_dialog() {
        let bytes=serde_json::to_vec(&serde_json::json!({"schema":1,"version":"999.0.0","published_at":"2026-09-19T12:00:00Z",
            "release_url":"https://github.com/pepitolas13/PepoMote/releases/tag/v999.0.0",
            "notes":{"es":["Instalación guiada con progreso y recuperación automática.","Más estabilidad al usar varios mandos."],"en":["Guided installation with progress and automatic recovery.","More stable connections with multiple controllers."]},
            "assets":{"windows-x86_64":{"name":"PepoMote.exe","url":"https://github.com/pepitolas13/PepoMote/releases/download/v999.0.0/PepoMote.exe","size":100000,"sha256":"0".repeat(64)}}})).unwrap();
        let manifest = manifest::Manifest::parse(&bytes).unwrap();
        {
            let mut s = locked();
            s.snapshot.release = Some(manifest);
            s.snapshot.phase = Phase::Available;
        }
        struct Preview {
            ui: UpdateUi,
            dismissed: Option<Version>,
        }
        impl eframe::App for Preview {
            fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("PepoMote · Actualizaciones");
                    ui.label("Vista de prueba: instalación desactivada");
                    self.ui.settings(ui);
                    if ui.button("ES / EN").clicked() {
                        crate::i18n::toggle();
                    }
                });
                if let Some(v) = self.ui.frame(ctx, true, self.dismissed) {
                    self.dismissed = Some(v);
                }
            }
        }
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([480.0, 640.0]),
            #[cfg(windows)]
            event_loop_builder: Some(Box::new(|builder| {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                builder.with_any_thread(true);
            })),
            ..Default::default()
        };
        eframe::run_native(
            "PepoMote — revisión de actualizaciones",
            options,
            Box::new(|cc| {
                crate::theme::apply(&cc.egui_ctx);
                Ok(Box::new(Preview {
                    ui: UpdateUi {
                        preview: true,
                        ..Default::default()
                    },
                    dismissed: None,
                }))
            }),
        )
        .unwrap();
    }
}
