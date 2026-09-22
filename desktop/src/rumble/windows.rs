//! Mando virtual Xbox 360 por ViGEmBus (Windows). Dolphin lo ve como
//! `XInput/<índice>/Gamepad` (salidas `Motor L` y `Motor R`), Cemu como un
//! mando XInput con ese índice y RetroArch lo autoconfigura en el puerto
//! del jugador. Con los modos de emulador sus botones y ejes se
//! quedan en reposo: ahí solo interesa lo que el juego le manda a los
//! motores, que llega por la notificación del driver y va al hub. En el
//! modo «mando universal» es al revés y además se le escribe el estado
//! del móvil con `apply`.
//!
//! El crate `vigem-client` habla con el driver por IOCTL (Rust puro, sin
//! ViGEmClient.dll). Sin el driver, `Client::connect` devuelve
//! `BusNotFound`: PepoMote lo instala él mismo (`vigem_setup`, el
//! instalador oficial viaja dentro del exe) y se reintenta solo.

use super::ring;
use super::Status;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use vigem_client::{Client, Error, TargetId, XRequestNotification, Xbox360Wired};

pub struct Backend {
    client: Arc<Client>,
}

pub struct Pad {
    target: Option<Xbox360Wired<Arc<Client>>>,
    listener: Option<std::thread::JoinHandle<()>>,
    /// Lo que avise el escuchador de los motores cuenta mientras esto siga a
    /// `true`: se apaga al retirar el mando ([`Pad::retire`]) y al soltarlo.
    alive: Arc<AtomicBool>,
    index: Option<u32>,
    /// Huecos de XInput ocupados justo ANTES de enchufar este mando: el que
    /// aparezca después es el suyo ([`appeared`]).
    before: [bool; XUSER_MAX],
    /// Puesto mientras se le sigue buscando el hueco; se quita al
    /// encontrarlo o al rendirse.
    looking: Option<Instant>,
}

impl Backend {
    /// Abre el bus del driver. BLOQUEA sin límite si el driver no contesta
    /// (SetupAPI + un IOCTL con espera infinita): solo lo llama el hub, en
    /// un hilo propio que sondea sin esperar (`Pending`).
    pub fn probe() -> Result<Backend, Status> {
        super::fake_hang_if_requested("el sondeo del driver");
        match Client::connect() {
            Ok(c) => {
                // Qué driver hay, para leer un receptor.log: un ViGEmBus
                // anterior a 1.17.333 perdía el último «apaga» con una sola
                // petición esperando (ver `ring`)
                crate::log_line!("Mando virtual: {}", driver_line());
                Ok(Backend { client: Arc::new(c) })
            }
            Err(Error::BusNotFound) => Err(Status::NeedsDriver),
            Err(Error::BusVersionMismatch) => Err(Status::NeedsDriver),
            Err(e) => {
                crate::log_line!("Vibración: ViGEmBus no responde: {e:?}");
                Err(Status::Failed)
            }
        }
    }

    /// Enchufa un mando. BLOQUEA sin límite si el driver no contesta
    /// (`plugin` y `wait_ready` esperan sin tope): igual que `probe`, solo
    /// desde el hilo propio que le pone el hub.
    pub fn create(&self, slot: u8) -> Result<Pad, String> {
        super::fake_hang_if_requested("la creación del mando");
        // Foto de los huecos de XInput ANTES de enchufar: el que aparezca
        // después es este mando (ver `enumerated_index`)
        let before = xinput_connected();
        let mut target = Xbox360Wired::new(self.client.clone(), TargetId::XBOX360_WIRED);
        target.plugin().map_err(|e| format!("plugin: {e:?}"))?;
        // El mando ya está enchufado: si el driver no sabe esperar a que esté
        // listo (o falla al hacerlo) se sigue igual, que `settle` le da 3 s
        // para aparecer en XInput. Tirarlo aquí lo enchufaba y desenchufaba
        // en cada reintento, y cada vaivén hacía a Dolphin releer todos sus
        // mandos (el DSU incluido).
        if let Err(e) = target.wait_ready() {
            crate::log_line!("Vibración: el driver no confirma el mando virtual del jugador {} ({e:?}); se sigue sin esperar", slot + 1);
        }
        // Windows tarda unas decenas de ms en enumerarlo. Esperarlo AQUÍ deja
        // colgado el hilo del móvil que acaba de conectarse (y con él su
        // `ok`), así que se prueba una vez —cuesta 0,1 ms— y lo que falte lo
        // resuelve el tic del hub con `settle`.
        let index = appeared(&before, &xinput_connected(), &[false; XUSER_MAX]);
        // Varias peticiones de aviso esperando en el driver (`ring`): con
        // una sola, el segundo aviso de cada cambio de Dolphin se perdía y el
        // móvil se quedaba vibrando. Aquí solo se preparan: las lanza el hilo
        // que las escucha.
        let mut peticiones = Vec::with_capacity(ring::IN_FLIGHT);
        for _ in 0..ring::IN_FLIGHT {
            let n = target.request_notification().map_err(|e| format!("notification: {e:?}"))?;
            peticiones.push(Xusb(Box::pin(n)));
        }
        // Reposo explícito: que ningún programa vea un eje a medias
        let _ = target.update(&vigem_client::XGamepad::default());
        let alive = Arc::new(AtomicBool::new(true));
        let listener = listen(slot, peticiones, alive.clone()).map_err(|e| format!("hilo: {e}"))?;
        Ok(Pad {
            target: Some(target),
            listener: Some(listener),
            alive,
            index,
            before,
            looking: index.is_none().then(Instant::now),
        })
    }
}

/// El hilo que escucha los motores del mando del slot. Las peticiones las
/// lanza él, dentro de `ring::serve`: Windows cancela la E/S de un hilo
/// cuando ese hilo termina, y `create` corre en uno de un solo uso.
fn listen(slot: u8, mut peticiones: Vec<Xusb>, alive: Arc<AtomicBool>) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new().name(format!("pmp-rumble-xusb-{}", slot + 1)).spawn(move || {
        let fin = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ring::serve(&mut peticiones, |grande, pequeno| super::set_from_pad(slot, &alive, grande, pequeno))
        }));
        let motivo = match fin {
            Ok(ring::Ending::Unplugged) => "el driver lo dio por desenchufado".to_owned(),
            Ok(ring::Ending::Failed(e)) => e,
            Err(_) => "pánico (detalles arriba)".to_owned(),
        };
        // Retirado por el hub: su cero ya está puesto y esto es lo esperado.
        // Si no, el mando sigue en su sitio y ya nadie oye sus motores: lo
        // último que avisó no vale, y dejarlo seguiría haciendo vibrar el móvil.
        if alive.load(Ordering::SeqCst) {
            crate::log_line!(
                "Vibración: el jugador {} deja de oír los motores de su mando virtual ({motivo}); se para su vibración (cambia de modo o reconecta el móvil para recuperarla)",
                slot + 1
            );
            super::set_from_pad(slot, &alive, 0, 0);
        }
    })
}

/// Una petición de aviso del crate, para el anillo de `ring`.
struct Xusb(Pin<Box<XRequestNotification>>);

impl ring::Request for Xusb {
    fn request(&mut self) -> Result<(), String> {
        use windows::Win32::Foundation::{GetLastError, SetLastError, ERROR_IO_PENDING, WIN32_ERROR};
        // El crate no mira lo que devuelve `DeviceIoControl`, y una petición
        // que el driver rechaza en el acto no se completa nunca: esperarla
        // colgaría el hilo. Se mira el último error del hilo, puesto a cero
        // antes (una petición que queda esperando deja ERROR_IO_PENDING).
        unsafe { SetLastError(WIN32_ERROR(0)) };
        self.0.as_mut().request();
        let e = unsafe { GetLastError() };
        if e == WIN32_ERROR(0) || e == ERROR_IO_PENDING {
            Ok(())
        } else {
            Err(format!("el driver rechazó la petición de aviso (error {})", e.0))
        }
    }

    fn wait(&mut self) -> Result<(u8, u8), ring::Ending> {
        match self.0.as_mut().poll(true) {
            Ok(Some(n)) => Ok((n.large_motor, n.small_motor)),
            // Esperando, «aún no» no puede llegar; si llegara, se sale en
            // vez de dar vueltas
            Ok(None) => Err(ring::Ending::Failed("la espera volvió sin aviso".to_owned())),
            Err(Error::OperationAborted) => Err(ring::Ending::Unplugged),
            Err(e) => Err(ring::Ending::Failed(format!("{e:?}"))),
        }
    }
}

/// Versión de archivo del driver instalado (`ViGEmBus.sys`). Para el log y
/// `--diag`: el paquete 1.22.0 lleva el driver 1.21.442.0.
pub fn driver_version() -> Result<(u16, u16, u16, u16), &'static str> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };
    let root = std::env::var_os("SystemRoot").ok_or("sin SystemRoot")?;
    let path = std::path::Path::new(&root).join("System32").join("drivers").join("ViGEmBus.sys");
    if !path.is_file() {
        return Err("no está System32\\drivers\\ViGEmBus.sys");
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let ilegible = "versión ilegible";
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
        if size == 0 {
            return Err(ilegible);
        }
        let mut buf = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(wide.as_ptr()), 0, size, buf.as_mut_ptr().cast()).map_err(|_| ilegible)?;
        let mut info: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if !VerQueryValueW(buf.as_ptr().cast(), w!("\\"), &mut info, &mut len).as_bool()
            || info.is_null()
            || (len as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>()
        {
            return Err(ilegible);
        }
        let f = std::ptr::read_unaligned(info as *const VS_FIXEDFILEINFO);
        Ok((
            (f.dwFileVersionMS >> 16) as u16,
            (f.dwFileVersionMS & 0xFFFF) as u16,
            (f.dwFileVersionLS >> 16) as u16,
            (f.dwFileVersionLS & 0xFFFF) as u16,
        ))
    }
}

/// ¿Es un ViGEmBus anterior a 1.17.333? Esos tiran el aviso de los motores
/// si no hay ninguna petición esperando (nefarius/ViGEmBus#68).
fn drops_notices(v: (u16, u16, u16, u16)) -> bool {
    (v.0, v.1, v.2) < (1, 17, 333)
}

/// Línea del driver para el log y `--diag`.
pub fn driver_line() -> String {
    match driver_version() {
        Ok(v) => {
            let viejo = if drops_notices(v) {
                " (anterior a 1.17.333: pierde avisos de los motores; PepoMote deja varias peticiones esperando)"
            } else {
                ""
            };
            format!("ViGEmBus {}.{}.{}.{}{viejo}", v.0, v.1, v.2, v.3)
        }
        Err(e) => format!("ViGEmBus ({e})"),
    }
}

/// Huecos de XInput (`XUSER_MAX_COUNT`).
pub const XUSER_MAX: usize = 4;

/// Qué huecos de XInput tienen un mando ahora mismo. Cuesta hasta unos
/// milisegundos por hueco vacío (XInput vuelve a enumerar), así que el hub
/// la llama FUERA de sus candados y pasa la foto a [`Pad::settle`].
pub fn xinput_connected() -> [bool; XUSER_MAX] {
    use windows::Win32::UI::Input::XboxController::{XInputGetState, XINPUT_STATE};
    let mut out = [false; XUSER_MAX];
    for (i, hueco) in out.iter_mut().enumerate() {
        let mut st = XINPUT_STATE::default();
        // ERROR_SUCCESS; cualquier otra cosa es «ahí no hay mando»
        *hueco = unsafe { XInputGetState(i as u32, &mut st) } == 0;
    }
    out
}

/// El hueco que ha aparecido entre las dos fotos y que no tenga ya dueño.
///
/// `taken` son los huecos que ya se llevaron OTROS mandos nuestros, y hace
/// falta: `reconcile` enchufa de una tacada todos los mandos que faltan, y
/// como Windows tarda unas decenas de milisegundos en enumerarlos, los dos
/// guardan la MISMA foto `before`. Sin este filtro, los dos se quedaban con
/// el hueco más bajo y los jugadores 2..4 acababan apuntando al mando del
/// Jugador 1: justo el fallo que todo esto viene a arreglar.
///
/// Si aparecen varios libres se coge el más bajo, que es el que Windows
/// reparte antes.
fn appeared(before: &[bool; XUSER_MAX], after: &[bool; XUSER_MAX], taken: &[bool; XUSER_MAX]) -> Option<u32> {
    (0..XUSER_MAX)
        .find(|i| after[*i] && !before[*i] && !taken[*i])
        .map(|i| i as u32)
}

/// Cuánto se le da a Windows para enumerar un mando virtual recién enchufado
/// antes de darlo por perdido.
const ENUMERATE_LIMIT: Duration = Duration::from_secs(3);

impl Pad {
    /// Desde aquí, lo que avise el escuchador de sus motores ya no cuenta. El
    /// hub lo llama al retirar el mando y ANTES de poner a cero la vibración
    /// del jugador: hasta que el hilo que lo destruye lo desenchufa, el
    /// emulador aún puede escribirle, y ese aviso tardío dejaba al jugador
    /// vibrando sin mando (o pisaba al mando nuevo del mismo jugador).
    pub fn retire(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }

    /// Todavía no se sabe en qué hueco de XInput ha caído.
    pub fn pending_index(&self) -> bool {
        self.looking.is_some()
    }

    /// Un intento de averiguar el hueco con la foto `after` de XInput que
    /// trae el hub. `true` si lo acaba de encontrar: quien llama reescribe
    /// la configuración del emulador, que hasta ahora no llevaba este mando
    /// (sin índice no se escribe nada que apunte a un XInput cualquiera).
    ///
    /// No se le pregunta al driver: `get_user_index()` contesta **0 para
    /// todos** los mandos virtuales. Medido con tres a la vez y 8 s de
    /// separación entre ellos: XInput los tenía en 0, 1 y 2 y el driver decía
    /// 0 en los tres. Como no falla, sino que miente, el plan B de «el índice
    /// es el slot» no llegaba a entrar nunca y los jugadores 2, 3 y 4 se
    /// quedaban sin vibración: su perfil de Cemu y de Dolphin apuntaba al
    /// mando del Jugador 1, que vibraba por todos.
    pub fn settle(&mut self, slot: u8, taken: &[bool; XUSER_MAX], after: &[bool; XUSER_MAX]) -> bool {
        let Some(desde) = self.looking else { return false };
        if let Some(i) = appeared(&self.before, after, taken) {
            self.index = Some(i);
            self.looking = None;
            crate::log_line!("Vibración: el mando virtual del jugador {} es XInput {i}", slot + 1);
            return true;
        }
        if desde.elapsed() >= ENUMERATE_LIMIT {
            // XInput solo tiene 4 huecos: con cuatro mandos ya puestos, el
            // nuestro no entra. Sin índice no hay vibración para ese
            // jugador: suponer «el número de jugador» apuntaba al mando de
            // verdad de otra persona.
            self.looking = None;
            crate::log_line!(
                "Vibración: XInput no enumera el mando virtual del jugador {}; sin vibración para él hasta que aparezca",
                slot + 1
            );
        }
        false
    }

    /// Escribe el estado del móvil en el mando virtual (modo mando
    /// universal). Un solo IOCTL; quien llama ya se encarga de no repetir
    /// un estado que no ha cambiado.
    pub fn apply(&mut self, s: &crate::pad::PadState) -> Result<(), String> {
        let Some(t) = self.target.as_mut() else { return Ok(()) };
        t.update(&vigem_client::XGamepad {
            buttons: vigem_client::XButtons { raw: s.buttons },
            left_trigger: s.lt,
            right_trigger: s.rt,
            thumb_lx: s.lx,
            thumb_ly: s.ly,
            thumb_rx: s.rx,
            thumb_ry: s.ry,
        })
        .map_err(|e| format!("update: {e:?}"))
    }

    pub fn xinput_index(&self) -> Option<u32> {
        self.index
    }

    pub fn describe(&self) -> String {
        match self.index {
            Some(i) => format!("Xbox 360 virtual, XInput {i}"),
            None => "Xbox 360 virtual".to_owned(),
        }
    }
}

impl Drop for Pad {
    fn drop(&mut self) {
        self.retire();
        // Desenchufar aborta las peticiones de aviso pendientes y su hilo
        // termina solo. No se le espera: si el driver no llegara a abortarlas,
        // un `join` aquí dejaría este hilo (y el candado de los mandos)
        // colgado para siempre, y con él la ventana y los móviles.
        if let Some(t) = self.target.take() {
            drop(t);
        }
        drop(self.listener.take());
    }
}

/// Sondea el driver AHORA y dice qué hay. BLOQUEA sin límite si no contesta:
/// nunca en línea, solo a través de `super::bounded_status` (que lo corre
/// en un hilo con tope de espera) o del hub.
pub fn probe_status() -> Status {
    super::fake_hang_if_requested("el sondeo del driver");
    match Client::connect() {
        Ok(_) => Status::Ready,
        Err(Error::BusNotFound) | Err(Error::BusVersionMismatch) => Status::NeedsDriver,
        Err(_) => Status::Failed,
    }
}

/// Solo con el mando virtual creado Y su hueco de XInput ya averiguado.
/// Antes se suponía «índice = slot» mientras tanto (y sin driver): el perfil
/// de Dolphin llevaba un `XInput/0/Gamepad` que no era nuestro, o que no
/// existía. Sin mando, sin línea: como en 1.11.
pub fn motor_expression(slot: u8) -> Option<String> {
    let i = super::xinput_index(slot)?;
    Some(motor_expression_for(i))
}

/// `Rumble/Motor` del Mando de Wii emulado de Dolphin para el mando XInput `i`.
pub(super) fn motor_expression_for(i: u32) -> String {
    format!("`XInput/{i}/Gamepad:Motor L`|`XInput/{i}/Gamepad:Motor R`")
}

pub fn cemu_node(slot: u8, player: u8) -> Option<String> {
    let i = super::xinput_index(slot)?;
    Some(super::cemu_node_with("XInput", &i.to_string(), player))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún hueco repartido todavía.
    const LIBRE: [bool; XUSER_MAX] = [false; XUSER_MAX];

    /// El caso de tres jugadores: cada mando virtual se lleva SU hueco. Es lo
    /// que fallaba cuando el índice se le pedía al driver (decía 0 los tres
    /// veces) y los jugadores 2 y 3 se quedaban sin vibración.
    #[test]
    fn cada_mando_virtual_se_queda_con_el_hueco_que_aparece() {
        let vacio = [false; XUSER_MAX];
        assert_eq!(appeared(&vacio, &[true, false, false, false], &LIBRE), Some(0));
        let uno = [true, false, false, false];
        assert_eq!(appeared(&uno, &[true, true, false, false], &LIBRE), Some(1));
        let dos = [true, true, false, false];
        assert_eq!(appeared(&dos, &[true, true, true, false], &LIBRE), Some(2));
    }

    /// Con mandos de verdad ya enchufados (lo normal en una partida de varios)
    /// el hueco nuevo NO es el número de jugador: por eso no vale suponerlo.
    #[test]
    fn con_mandos_reales_delante_el_hueco_no_es_el_slot() {
        let dos_reales = [true, true, false, false];
        assert_eq!(appeared(&dos_reales, &[true, true, true, false], &LIBRE), Some(2));
    }

    /// Los dos mandos de una misma pasada de `reconcile` comparten la foto
    /// `before` (Windows aún no había enumerado ninguno). Sin descontar lo ya
    /// repartido, los dos se quedaban con el hueco 0 y el Jugador 2 apuntaba
    /// al mando del Jugador 1.
    #[test]
    fn dos_mandos_de_la_misma_tacada_no_comparten_hueco() {
        let antes = [false; XUSER_MAX];
        let ahora = [true, true, false, false]; // los dos ya enumerados
        let primero = appeared(&antes, &ahora, &LIBRE).expect("hueco del primero");
        assert_eq!(primero, 0);
        let mut repartido = [false; XUSER_MAX];
        repartido[primero as usize] = true;
        assert_eq!(appeared(&antes, &ahora, &repartido), Some(1), "el segundo coge OTRO hueco");
    }

    #[test]
    fn la_expresion_del_motor_lleva_el_indice_real() {
        assert_eq!(motor_expression_for(1), "`XInput/1/Gamepad:Motor L`|`XInput/1/Gamepad:Motor R`");
        // sin hub (tests) no hay mando ni índice: nada que escribir
        assert!(motor_expression(0).is_none());
        assert!(cemu_node(0, 1).is_none());
    }

    /// Los ViGEmBus que tiraban el aviso de los motores sin petición
    /// esperando: hasta 1.17.306. El paquete 1.22.0 lleva el driver 1.21.442.0.
    #[test]
    fn que_drivers_pierden_avisos() {
        assert!(drops_notices((1, 16, 116, 0)), "el de BetterJoy y x360ce");
        assert!(drops_notices((1, 17, 306, 0)));
        assert!(!drops_notices((1, 17, 333, 0)), "el primero con la cola de avisos");
        assert!(!drops_notices((1, 21, 442, 0)), "el que instala PepoMote");
        assert!(!drops_notices((2, 0, 0, 0)));
    }

    /// Leer la versión del driver no falla haya o no driver (en la CI no hay).
    #[test]
    fn la_linea_del_driver_se_lee_siempre() {
        assert!(driver_line().starts_with("ViGEmBus"));
    }

    #[test]
    fn sin_hueco_nuevo_no_se_inventa_ninguno() {
        let lleno = [true; XUSER_MAX];
        assert_eq!(appeared(&lleno, &lleno, &LIBRE), None);
        assert_eq!(appeared(&[true, false, false, false], &[true, false, false, false], &LIBRE), None);
        // uno que se va no es uno que llega
        assert_eq!(appeared(&[true, true, false, false], &[true, false, false, false], &LIBRE), None);
    }
}
