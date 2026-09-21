//! Mando virtual por uinput con force feedback (Linux). Dolphin lo lista
//! como `evdev/0/PepoMote Wiimote N` (salidas `Strong`/`Weak` del efecto
//! rumble) solo si parece un mando de verdad: al menos dos ejes u ocho
//! botones (así lo filtra su backend evdev), y con ellos udev lo etiqueta
//! como joystick y da acceso al usuario de la sesión (`uaccess`), que es lo
//! que Dolphin, Cemu (SDL) y RetroArch (udev) necesitan para abrirlo en
//! lectura/escritura. En los modos de emulador no se pulsa nada: está para
//! recibir efectos. En el modo «mando universal» es además la salida, y se
//! le escribe el estado del móvil con `apply`.
//!
//! El núcleo nos manda por el propio descriptor de uinput las subidas y
//! borrados de efectos (`UI_FF_UPLOAD`/`UI_FF_ERASE`, que hay que
//! contestar) y las órdenes de reproducir (`EV_FF`, código = efecto, valor
//! = veces; 0 = parar). El nivel del mando es el máximo de los efectos que
//! suenan, escalado por `FF_GAIN` y con la duración `replay.length` que
//! pidió el programa (Dolphin pide medio segundo y lo renueva; Cemu por SDL
//! 5 s; RetroArch «infinito» hasta parar).

use super::{Source, Status};
use crate::pad::evdev as padev;
use crate::pad::PadState;
use crate::state::LockTolerant;
use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{
    AbsInfo, AbsoluteAxisType, AttributeSet, BusType, EventType, FFEffectData, FFEffectKind, FFEffectType, InputEvent,
    InputId, Key, UinputAbsSetup,
};
use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Backend;

pub struct Pad {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    name: String,
    /// Compartido con el hilo que atiende el force feedback: el `poll` va
    /// sobre el descriptor pelado, así que escribir botones no le quita
    /// tiempo.
    dev: Arc<Mutex<VirtualDevice>>,
}

/// Botones que declara el dispositivo. Tienen que cubrir todo lo que
/// `pad::evdev` puede llegar a pulsar: un código no declarado el kernel lo
/// descarta en silencio y ese botón sencillamente no existe para el juego.
/// Ese contrato lo vigila un test.
const DECLARED: [Key; 11] = [
    Key::BTN_SOUTH,
    Key::BTN_EAST,
    Key::BTN_NORTH,
    Key::BTN_WEST,
    Key::BTN_TL,
    Key::BTN_TR,
    Key::BTN_SELECT,
    Key::BTN_START,
    Key::BTN_MODE,
    Key::BTN_THUMBL,
    Key::BTN_THUMBR,
];

/// Un efecto subido por el programa: cuánto vibra y hasta cuándo.
#[derive(Clone, Copy)]
struct Effect {
    strong: u16,
    weak: u16,
    /// Duración que pidió (0 = hasta que lo pare).
    length_ms: u16,
    playing_until: Option<Instant>,
    playing: bool,
}

fn effect_from(data: &FFEffectData) -> Effect {
    let (strong, weak) = match data.kind {
        FFEffectKind::Rumble { strong_magnitude, weak_magnitude } => (strong_magnitude, weak_magnitude),
        FFEffectKind::Periodic { magnitude, .. } => (magnitude.unsigned_abs().saturating_mul(2), 0),
        FFEffectKind::Constant { level, .. } => (level.unsigned_abs().saturating_mul(2), 0),
        FFEffectKind::Ramp { start_level, end_level, .. } => {
            (start_level.unsigned_abs().max(end_level.unsigned_abs()).saturating_mul(2), 0)
        }
        _ => (0, 0),
    };
    Effect { strong, weak, length_ms: data.replay.length, playing_until: None, playing: false }
}

/// Nivel combinado (0..255 por motor) de los efectos que suenan ahora.
fn level(effects: &HashMap<i16, Effect>, gain: u16, now: Instant) -> (u8, u8) {
    let mut strong: u32 = 0;
    let mut weak: u32 = 0;
    for e in effects.values() {
        if !e.playing || e.playing_until.is_some_and(|t| now >= t) {
            continue;
        }
        strong = strong.max(e.strong as u32);
        weak = weak.max(e.weak as u32);
    }
    let scale = |v: u32| ((v * gain as u32) / 0xFFFF / 257).min(255) as u8;
    (scale(strong), scale(weak))
}

impl Backend {
    pub fn probe() -> Result<Backend, Status> {
        match crate::input::uinput_probe() {
            Ok(()) => Ok(Backend),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(Status::UinputDenied),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Status::UinputMissing),
            Err(_) => Err(Status::Failed),
        }
    }

    pub fn create(&self, slot: u8) -> Result<Pad, String> {
        let name = super::pad_name(slot);
        let mut keys = AttributeSet::<Key>::new();
        for k in DECLARED {
            keys.insert(k);
        }
        let mut ff = AttributeSet::<FFEffectType>::new();
        for t in [
            FFEffectType::FF_RUMBLE,
            FFEffectType::FF_PERIODIC,
            FFEffectType::FF_SINE,
            FFEffectType::FF_SQUARE,
            FFEffectType::FF_TRIANGLE,
            FFEffectType::FF_CONSTANT,
            FFEffectType::FF_RAMP,
            FFEffectType::FF_GAIN,
        ] {
            ff.insert(t);
        }
        let abs = AbsInfo::new(0, -32767, 32767, 16, 128, 0);
        let hat = AbsInfo::new(0, -1, 1, 0, 0, 0);
        // Gatillos: 0..255 es el rango de XInput y el que espera SDL. No los
        // había porque hasta ahora este mando no se pulsaba nunca.
        let trig = AbsInfo::new(0, 0, 255, 0, 0, 0);
        let e = |e: std::io::Error| format!("uinput: {e}");
        let mut dev = VirtualDeviceBuilder::new()
            .map_err(e)?
            .name(name.as_str())
            .input_id(InputId::new(BusType::BUS_VIRTUAL, super::PAD_VENDOR, super::PAD_PRODUCT, super::PAD_VERSION))
            .with_keys(&keys)
            .map_err(e)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_X, abs))
            .map_err(e)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_Y, abs))
            .map_err(e)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_RX, abs))
            .map_err(e)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_RY, abs))
            .map_err(e)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_Z, trig))
            .map_err(e)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_RZ, trig))
            .map_err(e)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_HAT0X, hat))
            .map_err(e)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_HAT0Y, hat))
            .map_err(e)?
            .with_ff(&ff)
            .map_err(e)?
            .with_ff_effects_max(16)
            .build()
            .map_err(e)?;
        // Nodo que salió: para el log (y para saber que existe)
        let node = dev
            .enumerate_dev_nodes_blocking()
            .ok()
            .and_then(|mut it| it.next().and_then(|n| n.ok()))
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "/dev/input/event?".to_owned());
        crate::log_line!("Vibración: «{name}» en {node}");

        let fd = dev.as_raw_fd();
        let dev = Arc::new(Mutex::new(dev));
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let dev2 = dev.clone();
        let thread = std::thread::Builder::new()
            .name(format!("pmp-rumble-uinput-{}", slot + 1))
            .spawn(move || serve(dev2, fd, slot, stop2))
            .map_err(|e| format!("hilo: {e}"))?;
        Ok(Pad { stop, thread: Some(thread), name, dev })
    }
}

/// Atiende el descriptor de uinput hasta que se pida parar.
fn serve(dev: Arc<Mutex<VirtualDevice>>, fd: RawFd, slot: u8, stop: Arc<AtomicBool>) {
    let mut effects: HashMap<i16, Effect> = HashMap::new();
    let mut gain: u16 = 0xFFFF;
    let mut last = (0u8, 0u8);
    while !stop.load(Ordering::Relaxed) {
        let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        let ready = unsafe { libc::poll(&mut pfd, 1, 50) };
        let now = Instant::now();
        if ready > 0 {
            // El candado solo se toma para atender lo que ya llegó; el
            // `poll` de arriba va sobre el descriptor pelado.
            let mut dev = dev.lock_tolerant();
            let events: Vec<_> = match dev.fetch_events() {
                Ok(it) => it.collect(),
                Err(_) => break,
            };
            for ev in events {
                match ev.event_type() {
                    EventType::UINPUT => {
                        if ev.code() == evdev::UInputEventType::UI_FF_UPLOAD.0 {
                            if let Ok(mut up) = dev.process_ff_upload(ev) {
                                let data = up.effect();
                                let id = up.effect_id();
                                let mut eff = effect_from(&data);
                                // Un efecto que se re-sube mientras suena sigue sonando
                                if let Some(old) = effects.get(&id) {
                                    eff.playing = old.playing;
                                    eff.playing_until = old.playing_until;
                                }
                                effects.insert(id, eff);
                                up.set_retval(0);
                            }
                        } else if ev.code() == evdev::UInputEventType::UI_FF_ERASE.0 {
                            if let Ok(mut er) = dev.process_ff_erase(ev) {
                                effects.remove(&(er.effect_id() as i16));
                                er.set_retval(0);
                            }
                        }
                    }
                    EventType::FORCEFEEDBACK => {
                        let code = ev.code();
                        if code == FFEffectType::FF_GAIN.0 {
                            gain = ev.value().clamp(0, 0xFFFF) as u16;
                        } else if let Some(e) = effects.get_mut(&(code as i16)) {
                            if ev.value() > 0 {
                                e.playing = true;
                                e.playing_until =
                                    (e.length_ms > 0).then(|| now + Duration::from_millis(e.length_ms as u64));
                            } else {
                                e.playing = false;
                                e.playing_until = None;
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else if ready < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        let now_level = level(&effects, gain, Instant::now());
        if now_level != last {
            last = now_level;
            super::set(slot, Source::Pad, now_level.0, now_level.1);
        }
    }
    super::set(slot, Source::Pad, 0, 0);
}

impl Pad {
    /// Escribe el estado del móvil en el mando virtual (modo mando
    /// universal). Quien llama ya se encarga de no repetir lo que no cambia.
    pub fn apply(&mut self, s: &PadState) -> Result<(), String> {
        let ev: Vec<InputEvent> = padev::events(s)
            .iter()
            .map(|e| {
                let t = match e.kind {
                    padev::Kind::Key => EventType::KEY,
                    padev::Kind::Abs => EventType::ABSOLUTE,
                };
                InputEvent::new(t, e.code, e.value)
            })
            .collect();
        self.dev.lock_tolerant().emit(&ev).map_err(|e| format!("uinput: {e}"))
    }

    pub fn xinput_index(&self) -> Option<u32> {
        None
    }

    /// En Linux el mando es un uinput con nombre propio: no hay hueco de
    /// XInput que averiguar (ver `rumble/windows.rs`).
    pub fn pending_index(&self) -> bool {
        false
    }

    pub fn settle(&mut self, _slot: u8, _taken: &[bool; 4]) -> bool {
        false
    }

    pub fn describe(&self) -> String {
        format!("gamepad uinput «{}»", self.name)
    }
}

impl Drop for Pad {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn static_status() -> Status {
    match Backend::probe() {
        Ok(_) => Status::Ready,
        Err(s) => s,
    }
}

pub fn motor_expression(slot: u8) -> Option<String> {
    let n = super::pad_name(slot);
    Some(format!("`evdev/0/{n}:Strong`|`evdev/0/{n}:Weak`"))
}

pub fn cemu_node(slot: u8, player: u8) -> Option<String> {
    let guid = super::sdl_guid(0x06, &super::pad_name(slot), super::PAD_VENDOR, super::PAD_PRODUCT, super::PAD_VERSION);
    Some(super::cemu_node_with("SDLController", &format!("0_{guid}"), player))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `pad::evdev` lleva los números de la ABI de Linux escritos a mano para
    /// poder probarse en Windows, donde el crate `evdev` ni compila. Aquí,
    /// que sí está, se cotejan con los de verdad: si alguno no cuadra, el
    /// mando pulsaría un botón que no es.
    #[test]
    fn los_codigos_a_mano_son_los_del_crate_evdev() {
        for (nuestro, suyo) in [
            (padev::BTN_SOUTH, Key::BTN_SOUTH),
            (padev::BTN_EAST, Key::BTN_EAST),
            (padev::BTN_NORTH, Key::BTN_NORTH),
            (padev::BTN_WEST, Key::BTN_WEST),
            (padev::BTN_TL, Key::BTN_TL),
            (padev::BTN_TR, Key::BTN_TR),
            (padev::BTN_SELECT, Key::BTN_SELECT),
            (padev::BTN_START, Key::BTN_START),
            (padev::BTN_MODE, Key::BTN_MODE),
            (padev::BTN_THUMBL, Key::BTN_THUMBL),
            (padev::BTN_THUMBR, Key::BTN_THUMBR),
        ] {
            assert_eq!(nuestro, suyo.code(), "{suyo:?}");
        }
        for (nuestro, suyo) in [
            (padev::ABS_X, AbsoluteAxisType::ABS_X),
            (padev::ABS_Y, AbsoluteAxisType::ABS_Y),
            (padev::ABS_Z, AbsoluteAxisType::ABS_Z),
            (padev::ABS_RX, AbsoluteAxisType::ABS_RX),
            (padev::ABS_RY, AbsoluteAxisType::ABS_RY),
            (padev::ABS_RZ, AbsoluteAxisType::ABS_RZ),
            (padev::ABS_HAT0X, AbsoluteAxisType::ABS_HAT0X),
            (padev::ABS_HAT0Y, AbsoluteAxisType::ABS_HAT0Y),
        ] {
            assert_eq!(nuestro, suyo.0, "{suyo:?}");
        }
    }

    /// Un código que se pulsa sin estar declarado en el dispositivo el kernel
    /// lo tira sin decir nada: el botón no existiría para el juego y no habría
    /// ni un error en el log.
    #[test]
    fn todo_boton_que_se_pulsa_esta_declarado_en_el_dispositivo() {
        for (_, code) in padev::KEYS {
            assert!(
                DECLARED.iter().any(|k| k.code() == code),
                "el código {code:#x} se pulsa pero el dispositivo no lo declara"
            );
        }
    }

    #[test]
    fn el_nivel_es_el_maximo_de_lo_que_suena_con_ganancia() {
        let now = Instant::now();
        let mut effects = HashMap::new();
        effects.insert(0, Effect { strong: 0xFFFF, weak: 0x8000, length_ms: 0, playing_until: None, playing: true });
        effects.insert(1, Effect { strong: 0x4000, weak: 0xFFFF, length_ms: 0, playing_until: None, playing: false });
        assert_eq!(level(&effects, 0xFFFF, now), (255, 128));
        effects.get_mut(&1).unwrap().playing = true;
        assert_eq!(level(&effects, 0xFFFF, now), (255, 255));
        assert_eq!(level(&effects, 0x8000, now), (127, 127), "ganancia a la mitad");
        // caducado: no cuenta
        effects.get_mut(&0).unwrap().playing_until = Some(now - Duration::from_millis(1));
        effects.get_mut(&1).unwrap().playing = false;
        assert_eq!(level(&effects, 0xFFFF, now), (0, 0));
    }

    #[test]
    fn la_expresion_de_dolphin_y_el_nodo_de_cemu_llevan_el_nombre() {
        assert_eq!(
            motor_expression(0).unwrap(),
            "`evdev/0/PepoMote Wiimote 1:Strong`|`evdev/0/PepoMote Wiimote 1:Weak`"
        );
        let n = cemu_node(1, 2).unwrap();
        assert!(n.contains("<api>SDLController</api>"));
        assert!(n.contains("<uuid>0_0600"));
        assert!(n.contains("PepoMote J2 vibración"));
    }
}
