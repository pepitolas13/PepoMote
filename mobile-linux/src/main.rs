//! PepoMote para Linux móvil (Mobian, postmarketOS y cualquier móvil con
//! kernel mainline): el EMISOR. Sensores por IIO, fusión de orientación
//! propia, UI táctil (egui sobre Wayland) y el mismo protocolo PMP v1 que la
//! app Android. Emparejamiento por código de 4 dígitos (sin cámara).

mod app;
mod buttons;
mod discovery;
mod fusion;
mod link;
mod sensor;
mod store;
mod ui;
// Mismo diseño que el receptor: paleta, tipografía e icono compartidos por
// ruta (un único origen de verdad, sin duplicar assets).
#[path = "../../desktop/src/theme.rs"]
mod theme;
#[path = "../../desktop/src/icon.rs"]
mod icon;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // --pair HOST[:PUERTO] CODIGO → empareja sin UI (scripts y pruebas)
    if let Some(i) = args.iter().position(|a| a == "--pair") {
        let (Some(target), Some(code)) = (args.get(i + 1), args.get(i + 2)) else {
            eprintln!("uso: PepoMote-Mobile --pair HOST[:PUERTO] CODIGO");
            std::process::exit(2);
        };
        let (host, port) = store::split_host_port(target);
        match link::pair(&host, port, code, &host) {
            Ok(p) => {
                store::save(&p);
                println!("Emparejado con {} ({}:{})", p.pc_name, p.host, p.port);
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }

    // --sensors → inventario de sensores del sistema (diagnóstico) y salir
    if args.iter().any(|a| a == "--sensors") {
        print!("{}", sensor::inventory());
        match sensor::open(false) {
            Ok(src) => {
                println!("=> {}", src.describe());
                println!("   {}", sensor::sample_summary(src, std::time::Duration::from_millis(1500)));
            }
            Err(e) => println!("=> {e}"),
        }
        std::process::exit(0);
    }
    let fake = args.iter().any(|a| a == "--fake-sensors");
    // --autoconnect [pointer|dolphin] → directo al mando, conectado (lanzadores)
    let autoconnect = args
        .iter()
        .position(|a| a == "--autoconnect")
        .map(|i| args.get(i + 1).cloned().unwrap_or_else(|| "pointer".into()));
    let fullscreen = args.iter().any(|a| a == "--fullscreen");
    app::log_line(&format!("arranque v{} args={:?}", env!("CARGO_PKG_VERSION"), args));
    let options = eframe::NativeOptions {
        // Ventana de MÓVIL: maximizada (el compositor decide el tamaño real),
        // sin decoraciones de escritorio y sin tamaño mínimo. Un tamaño fijo
        // mayor que la pantalla dejaba la ventana desbordada: se veía solo un
        // trozo en blanco y los botones quedaban fuera de alcance.
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([360.0, 640.0])
            .with_maximized(true)
            .with_fullscreen(fullscreen)
            .with_decorations(false)
            .with_title("PepoMote")
            // app_id = nombre del .desktop: así Phosh/Plasma Mobile asocian
            // la ventana a su icono y nombre en el lanzador
            .with_app_id("dev.pepotech.PepoMote")
            .with_icon(egui::IconData {
                rgba: icon::logo_rgba(64),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "PepoMote",
        options,
        Box::new(move |cc| Ok(Box::new(app::MobileApp::new(cc, fake, autoconnect)))),
    )
}
