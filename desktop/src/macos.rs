//! Lo específico de macOS que no es inyección ni captura: permisos (TCC:
//! Accesibilidad y Grabación de pantalla), AppKit (ventanas, Dock), la cola
//! principal y utilidades del sistema (nombre del equipo, versión, bundle).
#![cfg(target_os = "macos")]

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::MainThreadMarker;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// ¿Tenemos el permiso de Accesibilidad (mover el cursor, pulsar teclas)?
/// Barato: se puede sondear cada segundo.
pub fn ax_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Pide Accesibilidad con el diálogo del sistema («PepoMote quiere controlar
/// este ordenador…», con el botón que abre Ajustes), una vez por proceso.
pub fn ax_prompt_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef());
    });
}

/// ¿Tenemos «Grabación de pantalla» (la doble pantalla de Cemu)?
pub fn screen_capture_allowed() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Pide «Grabación de pantalla» con el diálogo del sistema, una vez por
/// proceso. macOS solo lo aplica al reiniciar la app.
pub fn screen_capture_request_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        CGRequestScreenCaptureAccess();
    });
}

/// Ejecuta `f` en la cola principal (AppKit solo se toca desde ahí). No
/// depende del repintado de egui: funciona con la ventana oculta o minimizada.
pub fn on_main(f: impl FnOnce() + Send + 'static) {
    dispatch2::Queue::main().exec_async(f);
}

/// Con --minimized: sin icono en el Dock hasta «Mostrar» (Accessory);
/// Regular lo devuelve. SOLO desde el hilo principal.
pub fn set_accessory(on: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(if on {
        NSApplicationActivationPolicy::Accessory
    } else {
        NSApplicationActivationPolicy::Regular
    });
}

/// «Mostrar»: icono en el Dock, ventanas visibles y delante, app activa.
/// Directamente por AppKit: vale aunque la ventana esté oculta o minimizada
/// (ahí egui no repinta y sus comandos de viewport no llegarían). SOLO desde
/// el hilo principal (ver `on_main`).
pub fn show_windows() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    unsafe {
        app.unhide(None);
        let windows = app.windows();
        for w in windows.iter() {
            w.deminiaturize(None);
            w.makeKeyAndOrderFront(None);
        }
    }
    app.activateIgnoringOtherApps(true);
}

/// Abre Ajustes del Sistema → Privacidad y seguridad en un panel
/// (`Privacy_Accessibility`, `Privacy_ScreenCapture`).
pub fn open_settings_pane(anchor: &str) {
    let _ = Command::new("open")
        .arg(format!("x-apple.systempreferences:com.apple.preference.security?{anchor}"))
        .spawn();
}

/// El .app que nos contiene (…/PepoMote.app), si corremos desde un bundle.
pub fn bundle_path() -> Option<PathBuf> {
    bundle_of(&std::env::current_exe().ok()?)
}

/// `…/PepoMote.app/Contents/MacOS/PepoMote` → `…/PepoMote.app`.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

/// Relanza PepoMote y sale (tras conceder «Grabación de pantalla» macOS
/// exige reiniciar la app). El relanzamiento espera un segundo a que este
/// proceso suelte el cerrojo de instancia única.
pub fn relaunch() -> ! {
    let cmd = match bundle_path() {
        Some(b) => format!("sleep 1; open -n \"{}\"", b.display()),
        None => match std::env::current_exe() {
            Ok(exe) => format!("sleep 1; \"{}\" &", exe.display()),
            Err(_) => std::process::exit(0),
        },
    };
    let _ = Command::new("sh").args(["-c", &cmd]).spawn();
    std::process::exit(0)
}

/// «15.1 (24B83)» vía sw_vers.
pub fn os_version() -> String {
    let get = |flag: &str| {
        Command::new("sw_vers")
            .arg(flag)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    match (get("-productVersion"), get("-buildVersion")) {
        (Some(v), Some(b)) => format!("{v} ({b})"),
        (Some(v), None) => v,
        _ => "?".to_owned(),
    }
}

/// Nombre del equipo tal y como lo puso el usuario («MacBook de Dani»).
pub fn computer_name() -> Option<String> {
    Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}
