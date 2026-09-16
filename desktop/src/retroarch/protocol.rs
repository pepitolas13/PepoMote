//! Lo que RetroArch entiende por el cable, tal cual está en su código
//! (`input/input_driver.c`, `cores/libretro-net-retropad/net_retropad_core.c`,
//! `command.c`, `command.h`), sin adivinar nada:
//!
//! - **Mando en red** («Network Gamepad» / Remote RetroPad): UDP al puerto
//!   base + jugador (55400, 55401…). Cada datagrama es UN control:
//!   `struct remote_message { int port; int device; int index; int id;
//!   uint16_t state; }` = 20 bytes (18 + relleno), enteros nativos
//!   (little-endian en todo PC). Solo se aceptan `device` 1 (RetroPad, `id`
//!   0..15) y 5 (analógico: `index` 0 = stick izquierdo, 1 = derecho; `id`
//!   0 = X, 1 = Y; `state` int16, +Y = abajo). Un tamaño distinto de 20 pone
//!   el mando a cero. Un botón pulsado sigue pulsado hasta que llega su 0.
//!   RetroArch lee **un** datagrama por jugador y por sondeo de entrada
//!   (una vez por fotograma emulado): más rápido se acumula cola; más lento
//!   quedan fotogramas sin nada (en Windows eso puede vaciar el mando).
//! - **Interfaz de comandos**: UDP al 55355, texto. `VERSION` y `GET_STATUS`
//!   responden al remitente en el mismo sondeo; las teclas rápidas
//!   (`MENU_TOGGLE`, `SAVE_STATE`…) valen un fotograma: las de mantener
//!   (`FAST_FORWARD_HOLD`, `REWIND`, `SLOWMOTION_HOLD`) hay que repetirlas
//!   cada fotograma.

/// `DEFAULT_NETWORK_REMOTE_BASE_PORT` (config.def.h).
pub const DEFAULT_BASE_PORT: u16 = 55400;
/// `DEFAULT_NETWORK_CMD_PORT` (config.def.h).
pub const DEFAULT_CMD_PORT: u16 = 55355;
/// `sizeof(struct remote_message)`: 4 enteros + uint16 + 2 de relleno.
pub const MESSAGE_LEN: usize = 20;
/// `RETRO_DEVICE_JOYPAD` / `RETRO_DEVICE_ANALOG` (libretro.h).
const DEVICE_JOYPAD: i32 = 1;
const DEVICE_ANALOG: i32 = 5;

/// Botones del RetroPad: `RETRO_DEVICE_ID_JOYPAD_*` (libretro.h). El orden
/// es el de la SNES: B abajo, A derecha, Y izquierda, X arriba.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum RetroPad {
    B = 0,
    Y = 1,
    Select = 2,
    Start = 3,
    Up = 4,
    Down = 5,
    Left = 6,
    Right = 7,
    A = 8,
    X = 9,
    L = 10,
    R = 11,
    L2 = 12,
    R2 = 13,
    L3 = 14,
    R3 = 15,
}

/// Botones y ejes de un jugador tal como los guarda RetroArch
/// (`input_remote_state_t`): 16 bits y cuatro int16 (LX, LY, RX, RY).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PadState {
    pub buttons: u16,
    pub axes: [i16; 4],
}

impl PadState {
    pub fn button(&self, id: u8) -> bool {
        self.buttons & (1 << id) != 0
    }

    pub fn is_zero(&self) -> bool {
        self.buttons == 0 && self.axes == [0; 4]
    }
}

/// Un control del mando remoto: un botón (0..15) o un eje (0..3).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Control {
    Button(u8),
    Axis(u8),
}

/// Número de controles distintos (16 botones + 4 ejes).
pub const CONTROLS: u8 = 20;

impl Control {
    /// Índice 0..19: botones primero, ejes después.
    pub fn index(self) -> u8 {
        match self {
            Control::Button(b) => b,
            Control::Axis(a) => 16 + a,
        }
    }

    pub fn from_index(i: u8) -> Control {
        if i < 16 {
            Control::Button(i)
        } else {
            Control::Axis(i - 16)
        }
    }
}

/// El datagrama de un control: `remote_message` en little-endian.
pub fn encode(port: u8, control: Control, state: i16) -> [u8; MESSAGE_LEN] {
    let (device, index, id) = match control {
        Control::Button(b) => (DEVICE_JOYPAD, 0i32, b as i32),
        Control::Axis(a) => (DEVICE_ANALOG, (a / 2) as i32, (a % 2) as i32),
    };
    let mut out = [0u8; MESSAGE_LEN];
    out[0..4].copy_from_slice(&(port as i32).to_le_bytes());
    out[4..8].copy_from_slice(&device.to_le_bytes());
    out[8..12].copy_from_slice(&index.to_le_bytes());
    out[12..16].copy_from_slice(&id.to_le_bytes());
    out[16..18].copy_from_slice(&(state as u16).to_le_bytes());
    out
}

/// Valor del control `c` en `s` tal como viaja en `state`.
pub fn value(s: &PadState, c: Control) -> i16 {
    match c {
        Control::Button(b) => s.button(b) as i16,
        Control::Axis(a) => s.axes[a as usize],
    }
}

/// Stick del móvil (−127..127, +Y arriba) → eje de libretro (int16, +Y abajo).
pub fn axis_from_stick(v: i8, invert: bool) -> i16 {
    let v = if invert { -(v as i32) } else { v as i32 };
    (v.clamp(-127, 127) * i16::MAX as i32 / 127) as i16
}

/// Tecla rápida de RetroArch que el móvil puede pedir (mensaje `hotkey`,
/// PROTOCOL.md §3). Nombre en el protocolo, comando de RetroArch y si es de
/// mantener (se repite cada fotograma mientras el móvil la sostiene).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hotkey {
    pub name: &'static str,
    pub command: &'static str,
    pub hold: bool,
}

/// Tabla completa (`map[]` de command.h). `menu` es especial en RetroArch:
/// se ejecuta directamente, no como pulsación de un fotograma.
pub const HOTKEYS: &[Hotkey] = &[
    Hotkey { name: "menu", command: "MENU_TOGGLE", hold: false },
    Hotkey { name: "save_state", command: "SAVE_STATE", hold: false },
    Hotkey { name: "load_state", command: "LOAD_STATE", hold: false },
    Hotkey { name: "slot_plus", command: "STATE_SLOT_PLUS", hold: false },
    Hotkey { name: "slot_minus", command: "STATE_SLOT_MINUS", hold: false },
    Hotkey { name: "fast_forward", command: "FAST_FORWARD_HOLD", hold: true },
    Hotkey { name: "fast_forward_toggle", command: "FAST_FORWARD", hold: false },
    Hotkey { name: "rewind", command: "REWIND", hold: true },
    Hotkey { name: "slow_motion", command: "SLOWMOTION_HOLD", hold: true },
    Hotkey { name: "pause", command: "PAUSE_TOGGLE", hold: false },
    Hotkey { name: "frame_advance", command: "FRAMEADVANCE", hold: false },
    Hotkey { name: "reset", command: "RESET", hold: false },
    Hotkey { name: "screenshot", command: "SCREENSHOT", hold: false },
    Hotkey { name: "fullscreen", command: "FULLSCREEN_TOGGLE", hold: false },
    Hotkey { name: "mute", command: "MUTE", hold: false },
    Hotkey { name: "volume_up", command: "VOLUME_UP", hold: false },
    Hotkey { name: "volume_down", command: "VOLUME_DOWN", hold: false },
    Hotkey { name: "close_content", command: "CLOSE_CONTENT", hold: false },
    Hotkey { name: "quit", command: "QUIT", hold: false },
    Hotkey { name: "shader_next", command: "SHADER_NEXT", hold: false },
    Hotkey { name: "shader_prev", command: "SHADER_PREV", hold: false },
    Hotkey { name: "shader_toggle", command: "SHADER_TOGGLE", hold: false },
    Hotkey { name: "disk_eject", command: "DISK_EJECT_TOGGLE", hold: false },
    Hotkey { name: "disk_next", command: "DISK_NEXT", hold: false },
    Hotkey { name: "disk_prev", command: "DISK_PREV", hold: false },
    Hotkey { name: "osk", command: "OSK", hold: false },
    Hotkey { name: "game_focus", command: "GAME_FOCUS_TOGGLE", hold: false },
    Hotkey { name: "grab_mouse", command: "GRAB_MOUSE_TOGGLE", hold: false },
    Hotkey { name: "turbo_fire", command: "TURBO_FIRE_TOGGLE", hold: false },
];

pub fn hotkey(name: &str) -> Option<Hotkey> {
    HOTKEYS.iter().copied().find(|h| h.name == name)
}

/// Comandos que responden (`action_map[]`): sirven de sonda del reloj.
pub const CMD_VERSION: &str = "VERSION";
pub const CMD_GET_STATUS: &str = "GET_STATUS";
/// `SHOW_MSG <texto>`: aviso en pantalla de RetroArch (180 fotogramas).
pub const CMD_SHOW_MSG: &str = "SHOW_MSG";

/// Qué hace RetroArch según `GET_STATUS`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Activity {
    /// Sin contenido cargado (en el menú principal).
    Contentless,
    Playing { core: String, content: String },
    Paused { core: String, content: String },
}

/// Una respuesta de la interfaz de comandos.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Reply {
    Version(String),
    Status(Activity),
    Other(String),
}

/// `GET_STATUS PLAYING core,contenido` / `GET_STATUS PAUSED …` /
/// `GET_STATUS CONTENTLESS`; cualquier otra línea corta sin espacios es la
/// versión (`1.22.2`).
pub fn parse_reply(text: &str) -> Reply {
    let line = text.lines().next().unwrap_or("").trim();
    if let Some(rest) = line.strip_prefix("GET_STATUS") {
        let rest = rest.trim();
        let (state, detail) = match rest.split_once(' ') {
            Some((s, d)) => (s, d.trim()),
            None => (rest, ""),
        };
        // versiones antiguas añadían «,crc32=…»: se ignora
        let mut parts = detail.splitn(2, ',');
        let core = parts.next().unwrap_or("").trim().to_owned();
        let content = parts.next().unwrap_or("").trim();
        let content = content
            .rsplit_once(",crc32=")
            .map(|(c, _)| c)
            .unwrap_or(content)
            .to_owned();
        return Reply::Status(match state {
            "PLAYING" => Activity::Playing { core, content },
            "PAUSED" => Activity::Paused { core, content },
            _ => Activity::Contentless,
        });
    }
    if !line.is_empty() && !line.contains(' ') && line.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Reply::Version(line.to_owned());
    }
    Reply::Other(line.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_datagrama_es_remote_message_en_little_endian() {
        // port 2, RETRO_DEVICE_JOYPAD, index 0, id A (8), state 1
        let m = encode(2, Control::Button(RetroPad::A as u8), 1);
        assert_eq!(m.len(), MESSAGE_LEN);
        assert_eq!(&m[0..4], &[2, 0, 0, 0]);
        assert_eq!(&m[4..8], &[1, 0, 0, 0]);
        assert_eq!(&m[8..12], &[0, 0, 0, 0]);
        assert_eq!(&m[12..16], &[8, 0, 0, 0]);
        assert_eq!(&m[16..18], &[1, 0]);
        assert_eq!(&m[18..20], &[0, 0], "relleno a cero");
        // stick derecho, eje Y, −32767: device 5, index 1, id 1
        let m = encode(0, Control::Axis(3), -32767);
        assert_eq!(&m[4..8], &[5, 0, 0, 0]);
        assert_eq!(&m[8..12], &[1, 0, 0, 0]);
        assert_eq!(&m[12..16], &[1, 0, 0, 0]);
        assert_eq!(&m[16..18], &(-32767i16 as u16).to_le_bytes());
    }

    #[test]
    fn los_ids_del_retropad_son_los_de_libretro() {
        assert_eq!(RetroPad::B as u8, 0);
        assert_eq!(RetroPad::Y as u8, 1);
        assert_eq!(RetroPad::Select as u8, 2);
        assert_eq!(RetroPad::Start as u8, 3);
        assert_eq!(RetroPad::Up as u8, 4);
        assert_eq!(RetroPad::Right as u8, 7);
        assert_eq!(RetroPad::A as u8, 8);
        assert_eq!(RetroPad::X as u8, 9);
        assert_eq!(RetroPad::L as u8, 10);
        assert_eq!(RetroPad::R3 as u8, 15);
    }

    #[test]
    fn indices_de_control_ida_y_vuelta() {
        for i in 0..CONTROLS {
            assert_eq!(Control::from_index(i).index(), i);
        }
        assert_eq!(Control::from_index(16), Control::Axis(0));
        assert_eq!(Control::from_index(19), Control::Axis(3));
    }

    #[test]
    fn el_stick_del_movil_se_escala_e_invierte() {
        assert_eq!(axis_from_stick(0, false), 0);
        assert_eq!(axis_from_stick(127, false), i16::MAX);
        assert_eq!(axis_from_stick(-127, false), -i16::MAX);
        assert_eq!(axis_from_stick(-128, false), -i16::MAX, "recortado a ±127");
        // +Y arriba en el móvil = −Y en libretro (arriba es negativo)
        assert_eq!(axis_from_stick(127, true), -i16::MAX);
        assert!(axis_from_stick(64, false) > 16000 && axis_from_stick(64, false) < 16600);
    }

    #[test]
    fn las_respuestas_se_entienden() {
        assert_eq!(parse_reply("1.22.2\n"), Reply::Version("1.22.2".into()));
        assert_eq!(parse_reply("GET_STATUS CONTENTLESS"), Reply::Status(Activity::Contentless));
        assert_eq!(
            parse_reply("GET_STATUS PLAYING snes9x,Super Mario World.sfc\n"),
            Reply::Status(Activity::Playing { core: "snes9x".into(), content: "Super Mario World.sfc".into() })
        );
        assert_eq!(
            parse_reply("GET_STATUS PAUSED nes,Zelda, The (U).nes,crc32=abcd1234\n"),
            Reply::Status(Activity::Paused { core: "nes".into(), content: "Zelda, The (U).nes".into() })
        );
        assert_eq!(parse_reply("GET_STATUS ERROR"), Reply::Status(Activity::Contentless));
        assert_eq!(parse_reply("READ_CORE_MEMORY 0 -1"), Reply::Other("READ_CORE_MEMORY 0 -1".into()));
    }

    #[test]
    fn las_teclas_rapidas_tienen_nombres_unicos_y_comandos_de_retroarch() {
        let mut names = std::collections::HashSet::new();
        for h in HOTKEYS {
            assert!(names.insert(h.name), "repetida: {}", h.name);
            assert!(h.command.chars().all(|c| c.is_ascii_uppercase() || c == '_'));
        }
        assert_eq!(hotkey("fast_forward").unwrap().command, "FAST_FORWARD_HOLD");
        assert!(hotkey("rewind").unwrap().hold);
        assert!(!hotkey("save_state").unwrap().hold);
        assert!(hotkey("nada").is_none());
    }
}
