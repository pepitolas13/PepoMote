//! El driver del mando virtual (ViGEmBus) viaja DENTRO del exe: el
//! instalador oficial firmado de Nefarius (`packaging/windows/`, BSD-3), sin
//! tocar un byte. Es un driver de núcleo y Windows 10/11 solo carga la firma
//! original, así que no se recompila ni se modifica: PepoMote lo instala él
//! mismo la primera vez que arranca sin él, con la única ventana de permiso
//! de administrador que Windows exige a cualquier driver, y si el usuario
//! dice que no, o el instalador falla, no vuelve a intentarlo por su cuenta
//! con esta versión (queda el botón de la ventana): un instalador que falla
//! no puede sacar la ventana de permiso en cada arranque. `PepoMote.exe
//! --install-driver` lo instala desde la línea de comandos (la CI, que corre
//! como administrador y sin driver).

use super::Status;
use std::path::PathBuf;

/// El instalador oficial, byte a byte (`packaging/windows/README.md`).
const SETUP: &[u8] = include_bytes!("../../../packaging/windows/ViGEmBus_1.22.0_x64_x86_arm64.exe");
/// Versión del instalador embebido: se guarda en los ajustes al intentarlo
/// (instalado, cancelado o fallido) para no volver a preguntar; una versión
/// nueva vuelve a intentarlo.
pub const SETUP_VERSION: &str = "1.22.0";
const SETUP_FILE: &str = "ViGEmBus_1.22.0_x64_x86_arm64.exe";
/// SHA-256 del instalador tal como se descargó de GitHub (releases/v1.22.0).
pub const SETUP_SHA256: &str = "89220a7865076b342892f98865f3499fb7c4cfd673159e89d352c360fd014c6a";
/// Advanced Installer: sin la interfaz del arranque, MSI en silencio y sin
/// reiniciar (el bus no lo necesita).
const SETUP_ARGS: &str = "/exenoui /qn /norestart";

/// Cómo acabó el instalador.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Installed,
    /// El usuario canceló la ventana de permiso de administrador.
    Declined,
    /// El instalador terminó con este código (1603 = fallo genérico del MSI).
    Failed(u32),
}

/// Qué significa el código de salida del instalador: 0 = hecho; 3010 =
/// hecho y Windows querría reiniciar (el bus funciona sin reiniciar); 1641 =
/// hecho y Windows ya ha empezado a reiniciar (`ERROR_SUCCESS_REBOOT_INITIATED`);
/// 1223 = `ERROR_CANCELLED`, el usuario dijo que no al permiso.
pub fn outcome_of(code: u32) -> Outcome {
    match code {
        0 | 3010 | 1641 => Outcome::Installed,
        1223 => Outcome::Declined,
        c => Outcome::Failed(c),
    }
}

/// Otra instalación en marcha (`ERROR_INSTALL_ALREADY_RUNNING`): el único
/// fallo que se resuelve solo, así que no se apunta y se vuelve a intentar en
/// el siguiente arranque.
pub const INSTALL_ALREADY_RUNNING: u32 = 1618;

/// Si toca instalar al arrancar: falta el driver, no se intentó ya con esta
/// versión del instalador (instalado, cancelado o fallido) y no lo apaga
/// `PEPOMOTE_NO_DRIVER_SETUP` (receptores de prueba: nunca un UAC).
pub fn should_install(status: Status, tried_version: Option<&str>, skip_env: bool) -> bool {
    status == Status::NeedsDriver && tried_version != Some(SETUP_VERSION) && !skip_env
}

/// Vuelca el instalador en la carpeta de configuración de PepoMote (se
/// reescribe siempre: 6 MB una vez cada instalación, y así nunca se lanza
/// un archivo que otro haya tocado).
fn unpack() -> Result<PathBuf, String> {
    let dir = crate::state::config_dir().ok_or_else(|| "sin carpeta de configuración".to_owned())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(SETUP_FILE);
    std::fs::write(&path, SETUP).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Lanza el instalador embebido elevado (`runas`: la ventana de permiso de
/// Windows; si el proceso ya es administrador, no pregunta) y espera a que
/// termine. Bloquea: llamar desde un hilo propio.
pub fn install() -> Result<Outcome, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let path = unpack()?;
    let wide = |s: &std::ffi::OsStr| -> Vec<u16> { s.encode_wide().chain(std::iter::once(0)).collect() };
    let verb = wide("runas".as_ref());
    let file = wide(path.as_os_str());
    let params = wide(SETUP_ARGS.as_ref());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        if e.code() == ERROR_CANCELLED.to_hresult() {
            return Ok(Outcome::Declined);
        }
        return Err(format!("ShellExecuteEx: {e}"));
    }
    let process = info.hProcess;
    if process.is_invalid() {
        return Err("el instalador no dejó proceso que esperar".to_owned());
    }
    unsafe { WaitForSingleObject(process, INFINITE) };
    let mut code = 0u32;
    let read = unsafe { GetExitCodeProcess(process, &mut code) };
    let _ = unsafe { CloseHandle(process) };
    read.map_err(|e| format!("GetExitCodeProcess: {e}"))?;
    Ok(outcome_of(code))
}

/// `--install-driver`: instala (o confirma) y sale con 0 si al final hay
/// driver; 3 cancelado, 4 el instalador falló, 5 no se pudo lanzar.
pub fn run_from_args() -> bool {
    if !std::env::args().any(|a| a == "--install-driver") {
        return false;
    }
    // Con tope: un driver que no contesta no puede colgar la CI ni a quien
    // lo prueba a mano
    let code = if super::bounded_status(super::ONE_SHOT_LIMIT) == Status::Ready {
        println!("El mando virtual ya está instalado (ViGEmBus).");
        0
    } else {
        println!("Instalando el mando virtual embebido (ViGEmBus {SETUP_VERSION})…");
        let mut result = install();
        if matches!(result, Ok(Outcome::Installed)) && super::bounded_status(super::ONE_SHOT_LIMIT) != Status::Ready {
            println!("El driver no responde tras el primer pase (versión anterior): segundo pase…");
            result = install();
        }
        match result {
            Ok(Outcome::Installed) => {
                let st = super::bounded_status(super::ONE_SHOT_LIMIT);
                let ok = st == Status::Ready;
                println!(
                    "Instalador terminado. Driver {}",
                    match st {
                        Status::Ready => "presente.",
                        Status::Unresponsive => "instalado pero no contesta (¿hace falta reiniciar?).",
                        _ => "todavía no visible (¿hace falta reiniciar?).",
                    }
                );
                if ok { 0 } else { 4 }
            }
            Ok(Outcome::Declined) => {
                println!("Instalación cancelada en la ventana de permiso.");
                3
            }
            Ok(Outcome::Failed(c)) => {
                println!("El instalador terminó con el código {c}.");
                4
            }
            Err(e) => {
                println!("No se pudo lanzar el instalador: {e}");
                5
            }
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El binario commiteado es el oficial: mismo tamaño y mismo SHA-256 que
    /// el descargado de la release v1.22.0 (y con firma Authenticode válida
    /// de Nefarius, comprobada a mano al traerlo).
    #[test]
    fn el_instalador_embebido_es_el_oficial() {
        use sha2::{Digest, Sha256};
        assert_eq!(SETUP.len(), 6_278_576);
        assert!(SETUP.starts_with(b"MZ"), "un ejecutable de Windows");
        assert_eq!(format!("{:x}", Sha256::digest(SETUP)), SETUP_SHA256);
    }

    #[test]
    fn el_codigo_de_salida_se_lee_como_windows() {
        assert_eq!(outcome_of(0), Outcome::Installed);
        assert_eq!(outcome_of(3010), Outcome::Installed, "instalado, reinicio sugerido");
        assert_eq!(outcome_of(1641), Outcome::Installed, "instalado, reinicio ya iniciado");
        assert_eq!(outcome_of(1223), Outcome::Declined, "ERROR_CANCELLED: el usuario dijo que no");
        assert_eq!(outcome_of(1603), Outcome::Failed(1603));
    }

    #[test]
    fn solo_se_instala_al_arrancar_si_falta_y_no_se_intento_con_esta_version() {
        assert!(should_install(Status::NeedsDriver, None, false));
        assert!(should_install(Status::NeedsDriver, Some("1.21.0"), false), "instalador nuevo: se vuelve a intentar");
        assert!(!should_install(Status::NeedsDriver, Some(SETUP_VERSION), false), "ya se intentó (instalado, cancelado o fallido)");
        assert!(!should_install(Status::Ready, None, false), "ya está");
        assert!(!should_install(Status::Failed, None, false), "el driver está, es otra cosa");
        assert!(!should_install(Status::NeedsDriver, None, true), "receptor de prueba: nunca un UAC");
        assert!(!should_install(Status::Checking, None, false), "aún sin resultado: se decide cuando llegue");
        assert!(!should_install(Status::Unresponsive, None, false), "el driver no contesta: no se instala solo, queda el botón");
    }
}
