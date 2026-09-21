//! Auto-configuración de Cemu (modo Wii U): un perfil de mando emulado por
//! móvil conectado en `controllerProfiles/controller{N}.xml`, leyendo del pad
//! DSU del móvil: el Jugador 1 es el Wii U GamePad (con movimiento y pantalla
//! táctil), los demás Pro Controller, y el que lo pida un Mando Wii (Wiimote
//! emulado con MotionPlus, puntero y su Nunchuk).
//!
//! El GamePad va SIEMPRE en el mando 1 (Cemu lo exige: sin él el juego ni
//! arranca), pero los demás móviles se colocan en los mandos que estén libres
//! (`placement`), sin tocar los que el usuario ya tenga configurados para los
//! otros jugadores.
//!
//! Formato y rutas verificados contra el código fuente de Cemu 2.x:
//! - config en `%APPDATA%\Cemu` (Windows), `$XDG_CONFIG_HOME/Cemu` (Linux),
//!   `~/.var/app/info.cemu.Cemu/config/Cemu` (Flatpak), o `portable/` junto
//!   al ejecutable (y el propio directorio del exe en instalaciones antiguas
//!   con `settings.xml` al lado);
//! - la ip/puerto del cliente DSU va DENTRO del `<controller>` del perfil,
//!   así que `settings.xml` no se toca;
//! - Cemu re-guarda el perfil al cerrar (sin comentarios) pero conserva
//!   `<profile>`, que es nuestra marca de autoría.
//!
//! Nunca escribe con Cemu abierto (su config se sobreescribe al salir) y
//! deja backup `.pepomote.bak` de cualquier perfil ajeno que sustituya.

use crate::state::LockTolerant;
use crate::state::{cemu_layout, CemuPlayer, Config, Mode, PadKind, SharedState};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Dos móviles conectando a la vez disparan dos configuraciones: en serie.
static CONFIGURE_LOCK: Mutex<()> = Mutex::new(());

/// Marca de autoría que sobrevive al re-guardado de Cemu.
const PROFILE_MARK: &str = "<profile>PepoMote</profile>";
const DSU_IP: &str = "127.0.0.1";
const DSU_PORT: u16 = 26760;
/// Cemu admite hasta 8 mandos (`InputManager::kMaxController`).
const MAX_CONTROLLERS: u8 = 8;

/// Botones y ejes del `DSUController` de Cemu: botón i = bit i del byte 36
/// del PadData, 8+i = bit i del byte 37, 16 = Touch (byte 39); los ejes
/// empiezan en 38 (`kAxisXP`) en el orden de `Buttons2` de Controller.h.
mod dsu {
    pub const SHARE: u32 = 0;
    pub const L3: u32 = 1;
    pub const R3: u32 = 2;
    pub const OPTIONS: u32 = 3;
    pub const UP: u32 = 4;
    pub const RIGHT: u32 = 5;
    pub const DOWN: u32 = 6;
    pub const LEFT: u32 = 7;
    pub const L2: u32 = 8;
    pub const R2: u32 = 9;
    pub const L1: u32 = 10;
    pub const R1: u32 = 11;
    pub const TRIANGLE: u32 = 12;
    pub const CIRCLE: u32 = 13;
    pub const CROSS: u32 = 14;
    pub const SQUARE: u32 = 15;
    pub const TOUCH: u32 = 16;
    pub const AXIS_XP: u32 = 38;
    pub const AXIS_YP: u32 = 39;
    pub const ROT_XP: u32 = 40;
    pub const ROT_YP: u32 = 41;
    pub const TRIG_XP: u32 = 42;
    pub const TRIG_YP: u32 = 43;
    pub const AXIS_XN: u32 = 44;
    pub const AXIS_YN: u32 = 45;
    pub const ROT_XN: u32 = 46;
    pub const ROT_YN: u32 = 47;
}
use dsu::*;
use crate::state::CfgStatus;
use crate::tr;

/// Wii U GamePad (`VPADController::ButtonId`): (mapping, botón DSU). El
/// PadData de PepoMote en modo Wii U pone A→Cross, B→Circle, X→Square,
/// Y→Triangle, L/R→L1/R1, ZL/ZR→gatillos analógicos, Mic→L2, Pantalla→R2,
/// clicks→L3/R3, +/−→Options/Share, Home→Touch (dsu/mapping.rs).
const GAMEPAD: &[(u32, u32)] = &[
    (1, CROSS),   // A
    (2, CIRCLE),  // B
    (3, SQUARE),  // X
    (4, TRIANGLE), // Y
    (5, L1),      // L
    (6, R1),      // R
    (7, TRIG_XP), // ZL
    (8, TRIG_YP), // ZR
    (9, OPTIONS), // +
    (10, SHARE),  // −
    (11, UP),
    (12, DOWN),
    (13, LEFT),
    (14, RIGHT),
    (15, L3), // click stick izq
    (16, R3), // click stick dcho
    (17, AXIS_YP), // stick izq arriba
    (18, AXIS_YN),
    (19, AXIS_XN),
    (20, AXIS_XP),
    (21, ROT_YP), // stick dcho arriba
    (22, ROT_YN),
    (23, ROT_XN),
    (24, ROT_XP),
    (25, L2),    // soplar al micro
    (26, R2),    // mostrar pantalla del GamePad
    (27, TOUCH), // Home
];

/// Wii U Pro Controller (`ProController::ButtonId`).
const PRO: &[(u32, u32)] = &[
    (1, CROSS),
    (2, CIRCLE),
    (3, SQUARE),
    (4, TRIANGLE),
    (5, L1),
    (6, R1),
    (7, TRIG_XP),
    (8, TRIG_YP),
    (9, OPTIONS),
    (10, SHARE),
    (11, TOUCH), // Home
    (12, UP),
    (13, DOWN),
    (14, LEFT),
    (15, RIGHT),
    (16, L3),
    (17, R3),
    (18, AXIS_YP),
    (19, AXIS_YN),
    (20, AXIS_XN),
    (21, AXIS_XP),
    (22, ROT_YP),
    (23, ROT_YN),
    (24, ROT_XN),
    (25, ROT_XP),
];

/// Mando Wii emulado (`WiimoteController::ButtonId`), parte del propio móvil.
const WIIMOTE: &[(u32, u32)] = &[
    (1, CROSS),    // A
    (2, CIRCLE),   // B
    (3, SQUARE),   // 1
    (4, TRIANGLE), // 2
    (7, OPTIONS),  // +
    (8, SHARE),    // −
    (9, UP),
    (10, DOWN),
    (11, LEFT),
    (12, RIGHT),
    (17, TOUCH), // Home
];

/// Parte del Nunchuk (el OTRO móvil, su propio pad DSU): C/Z van por L1/R1,
/// como en el perfil Wii de Dolphin.
const NUNCHUK: &[(u32, u32)] = &[
    (5, R1),       // Z
    (6, L1),       // C
    (13, AXIS_YP), // stick arriba
    (14, AXIS_YN),
    (15, AXIS_XN),
    (16, AXIS_XP),
];

/// `WPADDeviceType` de Cemu: MotionPlus solo / MotionPlus + Nunchuk.
const DEVICE_MPLS: u8 = 5;
const DEVICE_MPLS_NUNCHUK: u8 = 6;

/// Teclas del GamePad por teclado del PC (mando 1 sin móvil: todos son Mando
/// Wii). Cemu exige un Wii U GamePad con un dispositivo detrás: sin él lo da
/// por desconectado y el juego no arranca ni lee los Mandos Wii. El teclado
/// está siempre conectado y, de paso, sirve para lo poco que un juego pueda
/// pedir al GamePad. Códigos de tecla virtual de Windows, los mismos que
/// escribe Cemu al asignar el teclado (Intro 13, Retroceso 8, flechas 37-40,
/// letras = ASCII en mayúscula); `mapping` = `VPADController::ButtonId`.
const KEYBOARD_GAMEPAD: &[(u32, u32)] = &[
    (1, 13),  // A → Intro
    (2, 8),   // B → Retroceso
    (3, 88),  // X → X
    (4, 89),  // Y → Y
    (5, 81),  // L → Q
    (6, 69),  // R → E
    (7, 90),  // ZL → Z
    (8, 67),  // ZR → C
    (9, 80),  // + → P
    (10, 77), // − → M
    (11, 38), // cruceta ↑ → flecha arriba
    (12, 40), // cruceta ↓ → flecha abajo
    (13, 37), // cruceta ← → flecha izquierda
    (14, 39), // cruceta → → flecha derecha
    (15, 70), // click stick izq → F
    (16, 71), // click stick dcho → G
    (17, 87), // stick izq ↑ → W
    (18, 83), // stick izq ↓ → S
    (19, 65), // stick izq ← → A
    (20, 68), // stick izq → → D
    (21, 73), // stick dcho ↑ → I
    (22, 75), // stick dcho ↓ → K
    (23, 74), // stick dcho ← → J
    (24, 76), // stick dcho → → L
    (25, 78), // soplar al micro → N
    (26, 84), // mostrar pantalla del GamePad → T
    (27, 72), // Home → H
];

/// Nombre del GamePad por teclado (empieza por «PepoMote»: `is_pepomote_node`).
const KEYBOARD_GAMEPAD_NAME: &str = "PepoMote GamePad (teclado del PC)";

pub type Layout = [CemuPlayer];

fn write_mappings(out: &mut String, mappings: &[(u32, u32)]) {
    out.push_str("\t\t<mappings>\n");
    for (mapping, button) in mappings {
        let _ = writeln!(
            out,
            "\t\t\t<entry>\n\t\t\t\t<mapping>{mapping}</mapping>\n\t\t\t\t<button>{button}</button>\n\t\t\t</entry>"
        );
    }
    out.push_str("\t\t</mappings>\n\t</controller>\n");
}

fn controller_node(out: &mut String, uuid: u8, name: &str, motion: bool, mappings: &[(u32, u32)]) {
    out.push_str("\t<controller>\n\t\t<api>DSUController</api>\n");
    let _ = writeln!(out, "\t\t<uuid>{uuid}</uuid>");
    let _ = writeln!(out, "\t\t<display_name>{name}</display_name>");
    if motion {
        out.push_str("\t\t<motion>true</motion>\n");
    }
    // El móvil ya aplica su zona muerta (8 %): la de Cemu, mínima
    out.push_str("\t\t<axis>\n\t\t\t<deadzone>0.1</deadzone>\n\t\t\t<range>1</range>\n\t\t</axis>\n");
    out.push_str("\t\t<rotation>\n\t\t\t<deadzone>0.1</deadzone>\n\t\t\t<range>1</range>\n\t\t</rotation>\n");
    out.push_str("\t\t<trigger>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</trigger>\n");
    let _ = writeln!(out, "\t\t<ip>{DSU_IP}</ip>\n\t\t<port>{DSU_PORT}</port>");
    write_mappings(out, mappings);
}

/// El teclado del PC como dispositivo de un mando emulado: lo que escribe
/// Cemu al elegir «Keyboard» (api `Keyboard`, uuid `keyboard`), con nuestro
/// nombre y las zonas muertas por defecto de Cemu.
fn keyboard_node(out: &mut String, name: &str, mappings: &[(u32, u32)]) {
    out.push_str("\t<controller>\n\t\t<api>Keyboard</api>\n\t\t<uuid>keyboard</uuid>\n");
    let _ = writeln!(out, "\t\t<display_name>{name}</display_name>");
    out.push_str("\t\t<axis>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</axis>\n");
    out.push_str("\t\t<rotation>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</rotation>\n");
    out.push_str("\t\t<trigger>\n\t\t\t<deadzone>0.25</deadzone>\n\t\t\t<range>1</range>\n\t\t</trigger>\n");
    write_mappings(out, mappings);
}

/// El mando virtual de la vibración (rumble/) como dispositivo más del
/// mando emulado: sin botones, solo `<rumble>`. Cemu manda ahí lo que el
/// juego pide y el receptor lo reenvía al móvil. Nada en macOS.
fn rumble_node(out: &mut String, slot: u8, player: u8) {
    if let Some(node) = crate::rumble::cemu_node(slot, player) {
        out.push_str(&node);
    }
}

/// El perfil `controller{N}.xml` de un jugador.
pub fn profile_xml(pl: &CemuPlayer) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<emulated_controller>\n");
    let player = pl.player;
    let Some(slot) = pl.dsu_slot else {
        // GamePad del mando 1 SIN móvil (todos son Mando Wii): Cemu exige un
        // Wii U GamePad con un dispositivo detrás (sin dispositivo lo da por
        // desconectado: el juego no arranca ni lee los Mandos Wii). El
        // dispositivo es el teclado del PC, siempre conectado, con las teclas
        // de `KEYBOARD_GAMEPAD` por si un juego pide algo al GamePad.
        out.push_str("\t<type>Wii U GamePad</type>\n");
        let _ = writeln!(out, "\t{PROFILE_MARK}");
        keyboard_node(&mut out, KEYBOARD_GAMEPAD_NAME, KEYBOARD_GAMEPAD);
        out.push_str("</emulated_controller>\n");
        return out;
    };
    match pl.kind {
        PadKind::GamePad => {
            out.push_str("\t<type>Wii U GamePad</type>\n");
            let _ = writeln!(out, "\t{PROFILE_MARK}");
            controller_node(&mut out, slot, &format!("PepoMote J{player} GamePad"), true, GAMEPAD);
            rumble_node(&mut out, slot, player);
        }
        PadKind::Pro => {
            out.push_str("\t<type>Wii U Pro Controller</type>\n");
            let _ = writeln!(out, "\t{PROFILE_MARK}");
            controller_node(&mut out, slot, &format!("PepoMote J{player} Pro"), false, PRO);
            rumble_node(&mut out, slot, player);
        }
        PadKind::Wiimote => {
            out.push_str("\t<type>Wiimote</type>\n");
            let _ = writeln!(out, "\t{PROFILE_MARK}");
            let device = if pl.nunchuk_slot.is_some() { DEVICE_MPLS_NUNCHUK } else { DEVICE_MPLS };
            let _ = writeln!(out, "\t<device_type>{device}</device_type>");
            controller_node(&mut out, slot, &format!("PepoMote J{player} Mando Wii"), true, WIIMOTE);
            if let Some(ns) = pl.nunchuk_slot {
                controller_node(&mut out, ns, &format!("PepoMote J{player} Nunchuk"), false, NUNCHUK);
            }
            rumble_node(&mut out, slot, player);
        }
    }
    out.push_str("</emulated_controller>\n");
    out
}

fn is_ours(content: &str) -> bool {
    content.contains(PROFILE_MARK)
}

fn profile_path(dir: &Path, index: u8) -> PathBuf {
    dir.join(format!("controller{index}.xml"))
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("xml.pepomote.bak")
}

// ---------------------------------------------------------------------------
// Solo pantalla: el móvil como pantalla táctil junto al mando real del usuario
// ---------------------------------------------------------------------------

/// Algo que decirle al móvil aunque la configuración se haya hecho.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Warning {
    /// El perfil del usuario en ese índice no es un GamePad: el táctil del
    /// móvil no se aplicará.
    NotGamePad { index: u8, dsu_slot: u8 },
    /// No queda ni un mando libre en Cemu (admite 8) sin pisar los que el
    /// usuario ya tenía configurados: ese móvil se queda sin perfil.
    NoFreeSlot { dsu_slot: u8 },
}

/// Nodo DSU del móvil «solo pantalla» (`slot` = su pad DSU): sin movimiento
/// ni botones (los pone el mando real del usuario); el táctil viene implícito
/// con el DSU.
fn screen_only_node(pl: &CemuPlayer, slot: u8) -> String {
    let mut out = String::new();
    controller_node(&mut out, slot, &format!("PepoMote J{} pantalla", pl.player), false, &[]);
    out
}

/// Rangos (inicio, fin) de cada `<controller>…</controller>`: el inicio
/// retrocede al principio de línea si solo hay espacios delante y el fin se
/// lleva el salto de línea, así quitar un bloque no deja huecos.
fn controller_blocks(xml: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = xml[from..].find("<controller>") {
        let start = from + i;
        let Some(j) = xml[start..].find("</controller>") else { break };
        let end = start + j + "</controller>".len();
        let line_start = xml[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let s = if xml[line_start..start].trim().is_empty() { line_start } else { start };
        let e = if xml[end..].starts_with("\r\n") {
            end + 2
        } else if xml[end..].starts_with('\n') {
            end + 1
        } else {
            end
        };
        out.push((s, e));
        from = end;
    }
    out
}

/// Texto (recortado) entre `<tag>` y `</tag>`.
fn tag_text<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let i = block.find(&open)? + open.len();
    let j = block[i..].find(&close)? + i;
    Some(block[i..j].trim())
}

fn is_pepomote_named(block: &str) -> bool {
    tag_text(block, "display_name").is_some_and(|n| n.starts_with("PepoMote"))
}

/// Nuestro nodo, por nombre («PepoMote …») o por servidor (un DSUController
/// en la ip y puerto de PepoMote): Cemu re-guarda los perfiles al cerrar y
/// puede cambiarle el nombre, la ip y el puerto sobreviven.
fn is_pepomote_node(block: &str) -> bool {
    is_pepomote_named(block)
        || (tag_text(block, "api") == Some("DSUController")
            && tag_text(block, "ip") == Some(DSU_IP)
            && tag_text(block, "port") == Some(&DSU_PORT.to_string()))
}

fn strip_nodes(xml: &str, pred: fn(&str) -> bool) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut last = 0;
    for (s, e) in controller_blocks(xml) {
        if pred(&xml[s..e]) {
            out.push_str(&xml[last..s]);
            last = e;
        }
    }
    out.push_str(&xml[last..]);
    out
}

pub(crate) fn strip_pepomote_nodes(xml: &str) -> String {
    strip_nodes(xml, is_pepomote_node)
}

pub(crate) fn has_pepomote_node(xml: &str) -> bool {
    controller_blocks(xml).iter().any(|(s, e)| is_pepomote_node(&xml[*s..*e]))
}

pub(crate) fn has_gamepad_type(xml: &str) -> bool {
    tag_text(xml, "type") == Some("Wii U GamePad")
}

/// El perfil del usuario con nuestro nodo (y sin restos de nodos nuestros
/// anteriores) justo antes del cierre; `None` si no es un perfil de Cemu.
pub(crate) fn merge_screen_only(original: &str, node: &str) -> Option<String> {
    let base = strip_pepomote_nodes(original);
    let at = base.rfind("</emulated_controller>")?;
    let mut out = String::with_capacity(base.len() + node.len() + 1);
    out.push_str(&base[..at]);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(node);
    out.push_str(&base[at..]);
    Some(out)
}

/// Perfil de un móvil «solo pantalla»: fusión con el mando real del usuario
/// (el perfil ajeno actual, o el que guardamos antes en `.pepomote.bak`),
/// con copia del ajeno una sola vez. Sin mando del usuario, el perfil
/// completo de siempre (táctil y movimiento del móvil, nada que fusionar).
fn write_screen_only(path: &Path, pl: &CemuPlayer) -> Result<Option<Warning>, String> {
    // «solo pantalla» siempre es un móvil; el GamePad vacío nunca llega aquí
    let Some(slot) = pl.dsu_slot else {
        write_empty_gamepad(path)?;
        return Ok(None);
    };
    let current = std::fs::read_to_string(path).unwrap_or_default();
    let bak = backup_path(path);
    let base = if !current.is_empty() && !is_ours(&current) {
        Some(current.clone())
    } else {
        std::fs::read_to_string(&bak).ok().filter(|b| !b.is_empty() && !is_ours(b))
    };
    let Some(base) = base else {
        write_if_changed(path, &profile_xml(pl))?;
        return Ok(None);
    };
    let Some(merged) = merge_screen_only(&base, &screen_only_node(pl, slot)) else {
        write_if_changed(path, &profile_xml(pl))?;
        return Ok(None);
    };
    if merged != current {
        if !bak.exists() && !current.is_empty() {
            // el ajeno tal cual (sin nodos nuestros de otra sesión), una sola vez
            std::fs::write(&bak, strip_pepomote_nodes(&current)).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, &merged).map_err(|e| e.to_string())?;
    }
    Ok((!has_gamepad_type(&base)).then_some(Warning::NotGamePad { index: pl.order, dsu_slot: slot }))
}

/// Mando 1 sin móvil (todos son Mando Wii): Cemu necesita un GamePad emulado
/// con dispositivo igualmente. Si el usuario tiene ahí su propio GamePad (un
/// mando real, o el teclado configurado por él), ese manda y no se toca; si
/// lo tenía y lo sustituimos antes por un móvil (`.pepomote.bak`), vuelve; si
/// no hay nada (o hay otra cosa, que queda a salvo), el nuestro por teclado.
fn write_empty_gamepad(path: &Path) -> Result<(), String> {
    // lo nuestro fuera (y si sustituyó a un ajeno, ese vuelve), como en un
    // índice sin jugador
    clean_index(path);
    let current = std::fs::read_to_string(path).unwrap_or_default();
    if !current.is_empty() && !is_ours(&current) && has_gamepad_type(&current) {
        return Ok(());
    }
    write_if_changed(path, &profile_xml(&CemuPlayer::empty_gamepad()))?;
    Ok(())
}

/// Escribe el perfil solo si cambia. Un perfil AJENO (un mando real del
/// usuario) se guarda antes como `.pepomote.bak`, una sola vez.
fn write_if_changed(path: &Path, new: &str) -> Result<bool, String> {
    let original = std::fs::read_to_string(path).unwrap_or_default();
    if original == new {
        return Ok(false);
    }
    if !original.is_empty() && !is_ours(&original) {
        let bak = backup_path(path);
        if !bak.exists() {
            std::fs::copy(path, bak).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(path, new).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Un índice sin jugador: un perfil nuestro, fuera (y si sustituyó a uno
/// ajeno, ese vuelve); un perfil ajeno con nuestro nodo «solo pantalla»,
/// vuelve el respaldo o, sin respaldo, se le quita solo el nodo nuestro.
fn clean_index(path: &Path) {
    let Ok(content) = std::fs::read_to_string(path) else { return };
    let bak = backup_path(path);
    if is_ours(&content) {
        if bak.exists() {
            let _ = std::fs::rename(&bak, path);
        } else {
            let _ = std::fs::remove_file(path);
        }
    } else if has_pepomote_node(&content) {
        if bak.exists() {
            let _ = std::fs::rename(&bak, path);
        } else {
            let _ = std::fs::write(path, strip_nodes(&content, is_pepomote_named));
        }
    }
}

/// `controllerProfiles/controller{N}.xml` por jugador (fusionado con el mando
/// real del usuario si el móvil es «solo pantalla»); los índices sin jugador
/// se limpian si eran nuestros. Devuelve los avisos para los móviles.
/// ¿Ese mando de Cemu lo tiene puesto el usuario? Un perfil que no es
/// nuestro; o uno nuestro escrito ENCIMA del suyo, que se reconoce por el
/// respaldo. Lo segundo es lo que dejaban las versiones que sí pisaban los
/// mandos de los demás jugadores: así se les devuelve su sitio solo.
fn taken_by_user(dir: &Path, index: u8) -> bool {
    let path = profile_path(dir, index);
    if backup_path(&path).exists() {
        return true;
    }
    std::fs::read_to_string(&path).is_ok_and(|c| !c.trim().is_empty() && !is_ours(&c))
}

/// En qué `controller{N}.xml` va cada jugador EN ESTA instalación.
///
/// El primero manda en el mando 1 porque Cemu exige un Wii U GamePad ahí
/// (sin él el juego ni arranca ni lee los demás mandos). Los demás se colocan
/// en los mandos que estén LIBRES: si el usuario ya tenía a los otros
/// jugadores con mandos de verdad en los mandos 2 y 3, los móviles van detrás
/// en vez de pisarlos. Antes no: dos móviles de Mando Wii se comían los
/// perfiles de los amigos y Cemu dejaba de reconocer sus mandos hasta
/// desconectar los móviles y reiniciarlo.
///
/// `None` = no queda ningún mando libre (Cemu admite 8).
fn placement(dir: &Path, layout: &Layout) -> Vec<(CemuPlayer, Option<u8>)> {
    let mut used = [false; MAX_CONTROLLERS as usize];
    layout
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let index = if i == 0 {
                used[0] = true;
                Some(0)
            } else {
                let libre = (1..MAX_CONTROLLERS).find(|n| !used[*n as usize] && !taken_by_user(dir, *n));
                if let Some(n) = libre {
                    used[n as usize] = true;
                }
                libre
            };
            (*pl, index)
        })
        .collect()
}

pub fn write_profiles(cfg_dir: &Path, layout: &Layout) -> Result<Vec<Warning>, String> {
    let dir = cfg_dir.join("controllerProfiles");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut warnings = Vec::new();
    let places = placement(&dir, layout);
    for (pl, index) in &places {
        let Some(index) = *index else {
            warnings.push(Warning::NoFreeSlot { dsu_slot: pl.dsu_slot.unwrap_or(0) });
            continue;
        };
        let path = profile_path(&dir, index);
        if pl.is_empty_gamepad() {
            write_empty_gamepad(&path)?;
        } else if pl.screen_only {
            if let Some(w) = write_screen_only(&path, pl)? {
                warnings.push(w);
            }
        } else {
            write_if_changed(&path, &profile_xml(pl))?;
        }
    }
    for index in 0..MAX_CONTROLLERS {
        if !places.iter().any(|(_, i)| *i == Some(index)) {
            clean_index(&profile_path(&dir, index));
        }
    }
    Ok(warnings)
}

/// `settings.xml`: `<open_pad>true</open_pad>`, es decir, Cemu abre su ventana
/// «GamePad View» (la segunda pantalla) al arrancar; el receptor la captura y
/// la manda al móvil GamePad. Se conserva todo lo demás del archivo; si Cemu
/// nunca se ha abierto (no hay archivo) se crea uno mínimo que Cemu completa.
pub fn ensure_pad_window(cfg_dir: &Path) -> Result<(), String> {
    let path = cfg_dir.join("settings.xml");
    let original = std::fs::read_to_string(&path).unwrap_or_default();
    let new = if original.is_empty() {
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<content>\n    <open_pad>true</open_pad>\n</content>\n".to_owned()
    } else if original.contains("<open_pad>true</open_pad>") {
        return Ok(());
    } else if original.contains("<open_pad>false</open_pad>") {
        original.replacen("<open_pad>false</open_pad>", "<open_pad>true</open_pad>", 1)
    } else if let Some(i) = original.find("<content>") {
        let at = i + "<content>".len();
        format!("{}\n    <open_pad>true</open_pad>{}", &original[..at], &original[at..])
    } else {
        return Err(tr!("cemu.settings_no_content").to_owned());
    };
    if !original.is_empty() {
        let bak = path.with_extension("xml.pepomote.bak");
        if !bak.exists() {
            let _ = std::fs::copy(&path, bak);
        }
    }
    std::fs::write(&path, new).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Dónde está Cemu
// ---------------------------------------------------------------------------

fn name_has_cemu(p: &Path) -> bool {
    p.file_name()
        .map(|n| n.to_string_lossy().to_lowercase().contains("cemu"))
        .unwrap_or(false)
}

/// ¿Este directorio contiene el ejecutable de Cemu?
#[cfg(windows)]
fn has_exe(dir: &Path) -> bool {
    dir.join("Cemu.exe").is_file()
}

#[cfg(not(windows))]
fn has_exe(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false };
    rd.flatten().any(|e| {
        let n = e.file_name().to_string_lossy().to_lowercase();
        e.path().is_file() && (n == "cemu" || n == "cemu_release" || (n.contains("cemu") && n.ends_with(".appimage")))
    })
}

/// Sitios habituales donde vive un Cemu descomprimido (dos niveles bajo las
/// carpetas del usuario, uno bajo las del sistema).
fn search_roots() -> Vec<(PathBuf, u8)> {
    let mut roots: Vec<(PathBuf, u8)> = Vec::new();
    if let Some(u) = directories::UserDirs::new() {
        for d in [u.desktop_dir(), u.download_dir(), u.document_dir()].into_iter().flatten() {
            roots.push((d.to_path_buf(), 2));
        }
        roots.push((u.home_dir().to_path_buf(), 1));
        #[cfg(not(windows))]
        {
            roots.push((u.home_dir().join("Applications"), 1));
            roots.push((u.home_dir().join(".local/bin"), 1));
            roots.push((u.home_dir().join("Descargas"), 2));
            roots.push((u.home_dir().join("Downloads"), 2));
        }
    }
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            roots.push((PathBuf::from(local).join("Programs"), 1));
        }
        for d in ["C:\\", "C:\\Games", "C:\\Program Files", "C:\\Program Files (x86)", "D:\\", "D:\\Games"] {
            roots.push((PathBuf::from(d), 1));
        }
    }
    #[cfg(target_os = "macos")]
    roots.push((PathBuf::from("/Applications"), 1));
    #[cfg(not(windows))]
    {
        roots.push((PathBuf::from("/opt"), 1));
        if let Some(path) = std::env::var_os("PATH") {
            for d in std::env::split_paths(&path) {
                roots.push((d, 0));
            }
        }
    }
    roots
}

/// macOS: `Cemu.app` → `Cemu.app/Contents/MacOS` si ahí está el ejecutable.
fn app_bundle_exe_dir(p: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if p.extension().is_some_and(|e| e == "app") {
            return crate::procs::bundle_exe_dir(p, has_exe);
        }
    }
    let _ = p;
    None
}

/// Directorios que contienen a Cemu, encontrados por búsqueda.
pub fn find_exe_dirs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut push = |d: PathBuf| {
        if !found.contains(&d) {
            found.push(d);
        }
    };
    for (root, depth) in search_roots() {
        if has_exe(&root) {
            push(root.clone());
        }
        if depth == 0 {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten().take(400) {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if has_exe(&p) {
                push(p.clone());
            } else if let Some(inner) = app_bundle_exe_dir(&p) {
                push(inner);
            } else if depth >= 2 && name_has_cemu(&p) {
                if let Ok(rd2) = std::fs::read_dir(&p) {
                    for e2 in rd2.flatten().take(100) {
                        let p2 = e2.path();
                        if p2.is_dir() && has_exe(&p2) {
                            push(p2);
                        }
                    }
                }
            }
        }
    }
    found
}

/// Carpeta de configuración "roaming" de Cemu para instalaciones normales.
fn roaming_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Cemu"))
    }
    #[cfg(target_os = "macos")]
    {
        directories::BaseDirs::new().map(|b| b.home_dir().join("Library/Application Support/Cemu"))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(x) if !x.is_empty() => Some(PathBuf::from(x).join("Cemu")),
            _ => directories::BaseDirs::new().map(|b| b.home_dir().join(".config").join("Cemu")),
        }
    }
}

#[cfg(target_os = "linux")]
fn flatpak_dir() -> Option<PathBuf> {
    let app = directories::BaseDirs::new()?.home_dir().join(".var/app/info.cemu.Cemu");
    app.is_dir().then(|| app.join("config").join("Cemu"))
}

/// Carpetas de config de Cemu donde escribir los perfiles (todas las
/// instalaciones a la vista), a partir de los directorios del ejecutable.
pub fn config_dirs_from(exe_dirs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    // E2E aislado: nunca recorrer ni modificar las instalaciones reales.
    if let Some(dir) = std::env::var_os("PEPOMOTE_CEMU_DIR") {
        return Ok(vec![PathBuf::from(dir)]);
    }
    fn push(dirs: &mut Vec<PathBuf>, d: PathBuf) {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for d in exe_dirs {
        let portable = d.join("portable");
        if portable.is_dir() {
            push(&mut dirs, portable);
        } else if d.join("settings.xml").is_file() {
            push(&mut dirs, d.clone()); // instalación antigua (anterior a 2.0-89): portable de serie
        }
    }
    let roaming = roaming_dir();
    let mut evidence = !exe_dirs.is_empty() || roaming.as_ref().is_some_and(|r| r.exists());
    #[cfg(target_os = "linux")]
    {
        if let Some(f) = flatpak_dir() {
            push(&mut dirs, f);
            evidence = true;
        }
        if let Some(b) = directories::BaseDirs::new() {
            if b.home_dir().join(".local/share/Cemu").is_dir() {
                evidence = true;
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        evidence |= false;
    }
    // Sin instalación portable a la vista, Cemu usa la carpeta roaming (que
    // puede no existir aún si nunca se ha abierto: se crea)
    if let Some(r) = roaming {
        if evidence && (dirs.is_empty() || r.exists()) {
            push(&mut dirs, r);
        }
    }
    if dirs.is_empty() {
        return Err(tr!("cemu.not_found").to_owned());
    }
    Ok(dirs)
}

/// Directorios del ejecutable: el de Ajustes (si lo hay) y los encontrados.
fn exe_dirs(cfg: &Config) -> Vec<PathBuf> {
    if std::env::var_os("PEPOMOTE_CEMU_DIR").is_some() { return Vec::new(); }
    let mut dirs = Vec::new();
    if !cfg.cemu_dir.trim().is_empty() {
        dirs.push(PathBuf::from(cfg.cemu_dir.trim()));
    }
    for d in find_exe_dirs() {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    dirs
}

// ---------------------------------------------------------------------------
// ¿Está Cemu abierto? (y dónde vive)
// ---------------------------------------------------------------------------

/// (abierto, carpeta del ejecutable si se pudo saber).
#[cfg(windows)]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return (false, None);
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut running = false;
        let mut dir = None;
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let name: String = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len())],
                )
                .to_lowercase();
                // Cemu.exe, Cemu_release.exe…
                if name.starts_with("cemu") && name.ends_with(".exe") {
                    running = true;
                    if dir.is_none() {
                        if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, entry.th32ProcessID) {
                            let mut buf = [0u16; 1024];
                            let mut len = buf.len() as u32;
                            if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok() {
                                let exe = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
                                dir = exe.parent().map(|p| p.to_path_buf());
                            }
                            let _ = CloseHandle(h);
                        }
                    }
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        (running, dir)
    }
}

#[cfg(target_os = "linux")]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return (false, None);
    };
    let mut running = false;
    let mut dir = None;
    for e in entries.flatten() {
        let Ok(comm) = std::fs::read_to_string(e.path().join("comm")) else { continue };
        // Cemu, Cemu_release, cemu (AppImage, Flatpak y paquetes)
        if !comm.trim().to_lowercase().starts_with("cemu") {
            continue;
        }
        running = true;
        if dir.is_some() {
            continue;
        }
        // AppImage: el exe vive en el montaje FUSE; la ruta real está en APPIMAGE
        if let Ok(env) = std::fs::read(e.path().join("environ")) {
            for var in env.split(|b| *b == 0) {
                if let Some(v) = var.strip_prefix(b"APPIMAGE=") {
                    dir = PathBuf::from(String::from_utf8_lossy(v).to_string()).parent().map(|p| p.to_path_buf());
                }
            }
        }
        if dir.is_none() {
            if let Ok(exe) = std::fs::read_link(e.path().join("exe")) {
                if !exe.to_string_lossy().contains("/.mount_") {
                    dir = exe.parent().map(|p| p.to_path_buf());
                }
            }
        }
    }
    (running, dir)
}

/// macOS: `Cemu.app/Contents/MacOS/Cemu` (por libproc, sin bifurcar `ps`).
#[cfg(target_os = "macos")]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    crate::procs::running_with_prefix("cemu")
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn running_exe() -> (bool, Option<PathBuf>) {
    (false, None)
}

// ---------------------------------------------------------------------------
// Orquestación
// ---------------------------------------------------------------------------

fn describe(layout: &Layout) -> String {
    layout
        .iter()
        .map(|p| {
            if p.is_empty_gamepad() {
                return tr!("cemu.gamepad_empty").to_owned();
            }
            let kind = match p.kind {
                PadKind::GamePad if p.screen_only => tr!("cemu.kind_gamepad_screen"),
                PadKind::GamePad => tr!("cemu.kind_gamepad"),
                PadKind::Pro => tr!("cemu.kind_pro"),
                PadKind::Wiimote => {
                    if p.nunchuk_slot.is_some() {
                        tr!("cemu.kind_wiimote_nunchuk")
                    } else {
                        tr!("cemu.kind_wiimote")
                    }
                }
            };
            tr!("cemu.player_kind", p.player, kind)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// La ventana «GamePad View» (segunda pantalla) solo tiene sentido con un
/// móvil de GamePad al que mandarla: no con el GamePad vacío del mando 1.
fn wants_pad_window(layout: &Layout) -> bool {
    layout.iter().any(|p| p.kind == PadKind::GamePad && !p.is_empty_gamepad())
}

/// Escribe los perfiles en todas las instalaciones de Cemu a la vista.
pub fn configure(cfg: &Config, layout: &Layout) -> Result<(String, Vec<Warning>), String> {
    let dirs = config_dirs_from(&exe_dirs(cfg))?;
    let gamepad = wants_pad_window(layout);
    let mut warnings = Vec::new();
    for dir in &dirs {
        warnings.extend(write_profiles(dir, layout)?);
        if gamepad {
            // la segunda pantalla del GamePad vive en la ventana GamePad View
            ensure_pad_window(dir)?;
        }
    }
    Ok((
        format!(
            "{}{}",
            describe(layout),
            if dirs.len() > 1 { tr!("cemu.installs", dirs.len()) } else { String::new() }
        ),
        warnings,
    ))
}

/// Al ver a Cemu abierto se aprende su carpeta (para configurarlo cerrado
/// aunque viva en un sitio raro).
pub(crate) fn learn_dir(shared: &SharedState, dir: Option<PathBuf>) {
    let Some(dir) = dir else { return };
    let dir_s = dir.to_string_lossy().to_string();
    let mut s = shared.lock_tolerant();
    if s.config.cemu_dir != dir_s {
        s.config.cemu_dir = dir_s;
        s.config.save();
    }
}

/// Escribe (o deja pendiente si Cemu está abierto). `after_close`: viene
/// del vigilante, Cemu se acaba de cerrar. `manual`: botón de la ventana
/// (sin móviles escribe un GamePad para el pad 0). El reparto se lee YA con
/// el candado: dos móviles cambiando de mando a la vez disparan dos
/// configuraciones en serie, y la última en escribir tiene que llevar el
/// estado más reciente, no una foto tomada antes de esperar su turno.
fn run_configure(shared: &SharedState, after_close: bool, manual: bool) {
    let _serial = CONFIGURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut layout = cemu_layout(&shared.lock_tolerant().players);
    if layout.is_empty() {
        if !manual {
            shared.lock_tolerant().cemu_pending = false;
            return;
        }
        layout.push(CemuPlayer { order: 0, kind: PadKind::GamePad, player: 1, dsu_slot: Some(0), nunchuk_slot: None, screen_only: false });
    }
    let layout: &Layout = &layout;
    // Solo para los e2e en el propio equipo: escribir aunque el emulador esté abierto
    let (running, dir) = if std::env::var_os("PEPOMOTE_ASSUME_EMULATOR_CLOSED").is_some() {
        (false, None)
    } else {
        running_exe()
    };
    learn_dir(shared, dir);
    // `msg` para la ventana (con detalle); `phone` para los móviles (una línea corta)
    let mut warnings = Vec::new();
    let (ok, msg, phone) = if running {
        // Cemu sobreescribe sus perfiles al salir: se escribe en cuanto se
        // cierre (vigilante de auto_mode)
        shared.lock_tolerant().cemu_pending = true;
        (false, tr!("cemu.open").to_owned(), tr!("cemu.phone_open").to_owned())
    } else {
        shared.lock_tolerant().cemu_pending = false;
        let cfg = shared.lock_tolerant().config.clone();
        match configure(&cfg, layout) {
            Ok((details, warns)) => {
                warnings = warns;
                let prefix = if after_close { tr!("cemu.configured_after_close") } else { tr!("cemu.configured") };
                (true, format!("{prefix} {details}"), tr!("cemu.phone_configured").to_owned())
            }
            Err(e) => {
                let text = tr!("cemu.error", e);
                (false, text.clone(), text)
            }
        }
    };
    shared.lock_tolerant().cemu_cfg_status = Some(CfgStatus { ok, text: msg });
    crate::net::notify_all(&phone);
    // Avisos para un móvil concreto, después del general (queda a la vista)
    for w in warnings {
        match w {
            Warning::NotGamePad { dsu_slot, .. } => {
                crate::net::notify_slot(dsu_slot, tr!("cemu.phone_not_gamepad"));
            }
            Warning::NoFreeSlot { dsu_slot } => {
                crate::net::notify_slot(dsu_slot, tr!("cemu.phone_no_free_slot"));
            }
        }
    }
}

/// Disparo automático (conexión/desconexión/cambio de modo o de tipo de mando).
pub fn maybe_auto_configure(shared: &SharedState) {
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("emu-configure", move || {
        let (auto, mode, empty) = {
            let s = shared.lock_tolerant();
            (s.config.auto_cemu, s.mode, cemu_layout(&s.players).is_empty())
        };
        if auto && mode == Mode::Cemu && !empty {
            run_configure(&shared, false, false);
        }
    });
}

/// Lo pendiente (Cemu estaba abierto) se aplica cuando ya está cerrado:
/// lo llama el vigilante de `auto_mode` al ver a Cemu cerrado.
pub fn apply_pending(shared: &SharedState) {
    let (auto, mode) = {
        let s = shared.lock_tolerant();
        (s.config.auto_cemu, s.mode)
    };
    if auto && mode == Mode::Cemu {
        run_configure(shared, true, false);
    } else {
        shared.lock_tolerant().cemu_pending = false;
    }
}

/// El último móvil se ha ido y era «solo pantalla»: su nodo DSU sobra en el
/// perfil del usuario (Cemu lo enseñaría como desconectado) y el respaldo
/// vuelve. Solo en este caso: en el modo normal el perfil del móvil se queda
/// escrito, como siempre. Con Cemu abierto queda pendiente para el vigilante.
pub fn cleanup_after_screen_only(shared: &SharedState) {
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("emu-configure", move || {
        let (auto, mode) = {
            let s = shared.lock_tolerant();
            (s.config.auto_cemu, s.mode)
        };
        if auto && mode == Mode::Cemu {
            run_cleanup(&shared);
        }
    });
}

/// Lo llama el vigilante de `auto_mode` al ver a Cemu cerrado.
pub fn apply_cleanup_pending(shared: &SharedState) {
    run_cleanup(shared);
}

fn run_cleanup(shared: &SharedState) {
    let _serial = CONFIGURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (running, dir) = if std::env::var_os("PEPOMOTE_ASSUME_EMULATOR_CLOSED").is_some() {
        (false, None)
    } else {
        running_exe()
    };
    learn_dir(shared, dir);
    if running {
        shared.lock_tolerant().cemu_cleanup_pending = true;
        return;
    }
    shared.lock_tolerant().cemu_cleanup_pending = false;
    let cfg = shared.lock_tolerant().config.clone();
    if let Ok(dirs) = config_dirs_from(&exe_dirs(&cfg)) {
        for dir in &dirs {
            if let Err(e) = write_profiles(dir, &[]) {
                crate::log_line!("Cemu: no se pudo restaurar el perfil del usuario en {}: {e}", dir.display());
            }
        }
        crate::log_line!("Cemu: perfil del usuario restaurado tras el móvil «solo pantalla»");
    }
}

/// Botón manual de la ventana.
pub fn configure_now(shared: &SharedState) {
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("emu-configure", move || {
        run_configure(&shared, false, true);
    });
}

/// Botón «Detectar» de Ajustes: busca Cemu y guarda su carpeta.
pub fn detect_now(shared: &SharedState) {
    let shared = shared.clone();
    let _ = crate::threads::spawn_once("emu-detect", move || {
        let (_, dir) = running_exe();
        let found = dir.into_iter().chain(find_exe_dirs()).next();
        let mut s = shared.lock_tolerant();
        match found {
            Some(d) => {
                s.config.cemu_dir = d.to_string_lossy().to_string();
                s.config.save();
                s.cemu_cfg_status = Some(CfgStatus::ok(tr!("cemu.found", d.display())));
            }
            None => {
                s.cemu_cfg_status = Some(CfgStatus::warn(tr!("cemu.not_found_manual").to_owned()));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pepomote-cemu-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn gamepad(order: u8, slot: u8) -> CemuPlayer {
        CemuPlayer { order, kind: PadKind::GamePad, player: order + 1, dsu_slot: Some(slot), nunchuk_slot: None, screen_only: false }
    }

    fn gamepad_screen(order: u8, slot: u8) -> CemuPlayer {
        CemuPlayer { order, kind: PadKind::GamePad, player: order + 1, dsu_slot: Some(slot), nunchuk_slot: None, screen_only: true }
    }

    fn pro(order: u8, slot: u8) -> CemuPlayer {
        CemuPlayer { order, kind: PadKind::Pro, player: order + 1, dsu_slot: Some(slot), nunchuk_slot: None, screen_only: false }
    }

    fn wii(order: u8, slot: u8, nunchuk: Option<u8>) -> CemuPlayer {
        CemuPlayer { order, kind: PadKind::Wiimote, player: order + 1, dsu_slot: Some(slot), nunchuk_slot: nunchuk, screen_only: false }
    }

    /// Mando Wii en el mando `index` con el jugador explícito (tras el GamePad vacío, jugador = índice).
    fn wii_player(order: u8, player: u8, slot: u8) -> CemuPlayer {
        CemuPlayer { player, ..wii(order, slot, None) }
    }

    /// Perfil de un mando real del usuario tal como lo guarda Cemu.
    const AJENO: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<emulated_controller>\n\t<type>Wii U GamePad</type>\n\t<profile>MiMando</profile>\n\t<controller>\n\t\t<api>XInput</api>\n\t\t<uuid>0</uuid>\n\t\t<display_name>Controller 1</display_name>\n\t\t<mappings>\n\t\t\t<entry>\n\t\t\t\t<mapping>1</mapping>\n\t\t\t\t<button>0</button>\n\t\t\t</entry>\n\t\t</mappings>\n\t</controller>\n</emulated_controller>\n";

    /// (mapping → button) de un XML, en orden.
    #[test]
    fn perfil_gamepad_por_teclado_sin_movil() {
        let xml = profile_xml(&CemuPlayer::empty_gamepad());
        assert!(well_formed(&xml));
        assert!(xml.contains("<type>Wii U GamePad</type>"));
        assert!(has_gamepad_type(&xml));
        assert!(xml.contains(PROFILE_MARK), "es nuestro: se limpia cuando ya no hace falta");
        // Cemu da por desconectado un mando emulado sin dispositivo: el teclado
        // del PC (siempre conectado), tal como lo escribe Cemu, con nuestro nombre
        assert_eq!(xml.matches("<controller>").count(), 1);
        assert!(xml.contains("<api>Keyboard</api>\n\t\t<uuid>keyboard</uuid>"), "{xml}");
        assert!(xml.contains(&format!("<display_name>{KEYBOARD_GAMEPAD_NAME}</display_name>")));
        assert!(!xml.contains("DSUController") && !xml.contains("<ip>") && !xml.contains("<port>"));
        assert!(!xml.contains("device_type") && !xml.contains("<motion>"));
        let m = mappings(&xml);
        assert_eq!(m.len(), 27, "todos los botones y ejes del GamePad tienen tecla");
        assert!(m.contains(&(1, 13)), "A → Intro");
        assert!(m.contains(&(2, 8)), "B → Retroceso");
        assert!(m.contains(&(11, 38)) && m.contains(&(12, 40)) && m.contains(&(13, 37)) && m.contains(&(14, 39)), "cruceta → flechas");
        assert!(m.contains(&(17, 87)) && m.contains(&(18, 83)) && m.contains(&(19, 65)) && m.contains(&(20, 68)), "stick izq → WASD");
        assert!(m.contains(&(27, 72)), "Home → H");
        let ids: std::collections::BTreeSet<u32> = m.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), 27, "sin ids repetidos");
        let keys: std::collections::BTreeSet<u32> = m.iter().map(|(_, k)| *k).collect();
        assert_eq!(keys.len(), 27, "sin teclas repetidas");
        // el nodo es nuestro por nombre: se limpia y se sustituye como los DSU
        let (s, e) = controller_blocks(&xml)[0];
        assert!(is_pepomote_node(&xml[s..e]));
    }

    #[test]
    fn dos_mandos_wii_dejan_un_gamepad_en_el_mando_1() {
        // Mario Party 10 con dos móviles como Mando de Wii: el mando 1 no
        // puede ser un Wiimote (Cemu necesita el GamePad): vacío nuestro, y
        // los Mandos Wii en los mandos 2 y 3 con sus pads y sus jugadores
        let dir = tmp_dir("dos-wii");
        let profiles = dir.join("controllerProfiles");
        write_profiles(&dir, &[CemuPlayer::empty_gamepad(), wii_player(1, 1, 0), wii_player(2, 2, 1)]).unwrap();
        let c0 = std::fs::read_to_string(profiles.join("controller0.xml")).unwrap();
        assert!(has_gamepad_type(&c0) && is_ours(&c0) && c0.contains("<api>Keyboard</api>") && !c0.contains("DSUController"), "mando 1 = GamePad por teclado nuestro: {c0}");
        let c1 = std::fs::read_to_string(profiles.join("controller1.xml")).unwrap();
        assert!(c1.contains("<type>Wiimote</type>") && c1.contains("<uuid>0</uuid>") && c1.contains("PepoMote J1 Mando Wii"), "{c1}");
        let c2 = std::fs::read_to_string(profiles.join("controller2.xml")).unwrap();
        assert!(c2.contains("<type>Wiimote</type>") && c2.contains("<uuid>1</uuid>") && c2.contains("PepoMote J2 Mando Wii"), "{c2}");
        let gamepads = (0..MAX_CONTROLLERS)
            .filter_map(|i| std::fs::read_to_string(profile_path(&profiles, i)).ok())
            .filter(|x| has_gamepad_type(x))
            .count();
        assert_eq!(gamepads, 1, "exactamente un GamePad");
        // J1 vuelve a GamePad: su móvil ocupa el mando 1, J2 sube al mando 2 y el 3 se limpia
        write_profiles(&dir, &[gamepad(0, 0), wii_player(1, 2, 1)]).unwrap();
        let c0 = std::fs::read_to_string(profiles.join("controller0.xml")).unwrap();
        assert!(has_gamepad_type(&c0) && c0.contains("<uuid>0</uuid>") && c0.contains("<motion>true</motion>"), "{c0}");
        let c1 = std::fs::read_to_string(profiles.join("controller1.xml")).unwrap();
        assert!(c1.contains("<type>Wiimote</type>") && c1.contains("<uuid>1</uuid>") && c1.contains("PepoMote J2 Mando Wii"), "{c1}");
        assert!(!profiles.join("controller2.xml").exists());
        // todos se van: nada nuestro queda
        write_profiles(&dir, &[]).unwrap();
        assert!(!profiles.join("controller0.xml").exists() && !profiles.join("controller1.xml").exists());
    }

    #[test]
    fn el_gamepad_vacio_respeta_el_gamepad_real_del_usuario() {
        let dir = tmp_dir("vacio-ajeno");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        // el usuario tiene su GamePad real en el mando 1: ese manda, sin copia ni cambios
        std::fs::write(profiles.join("controller0.xml"), AJENO).unwrap();
        write_profiles(&dir, &[CemuPlayer::empty_gamepad(), wii_player(1, 1, 0)]).unwrap();
        assert_eq!(std::fs::read_to_string(profiles.join("controller0.xml")).unwrap(), AJENO);
        assert!(!profiles.join("controller0.xml.pepomote.bak").exists());
        assert!(profiles.join("controller1.xml").is_file());
        // un ajeno que NO es GamePad (un Pro): queda a salvo y entra el vacío nuestro
        let pro_ajeno = AJENO.replace("Wii U GamePad", "Wii U Pro Controller");
        std::fs::write(profiles.join("controller0.xml"), &pro_ajeno).unwrap();
        write_profiles(&dir, &[CemuPlayer::empty_gamepad(), wii_player(1, 1, 0)]).unwrap();
        let c0 = std::fs::read_to_string(profiles.join("controller0.xml")).unwrap();
        assert!(is_ours(&c0) && has_gamepad_type(&c0) && c0.contains("<api>Keyboard</api>"), "{c0}");
        assert_eq!(std::fs::read_to_string(profiles.join("controller0.xml.pepomote.bak")).unwrap(), pro_ajeno);
        // y al irse todos, el Pro del usuario vuelve
        write_profiles(&dir, &[]).unwrap();
        assert_eq!(std::fs::read_to_string(profiles.join("controller0.xml")).unwrap(), pro_ajeno);
        assert!(!profiles.join("controller0.xml.pepomote.bak").exists());
    }

    #[test]
    fn el_gamepad_vacio_devuelve_el_gamepad_real_guardado() {
        let dir = tmp_dir("vacio-bak");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        std::fs::write(profiles.join("controller0.xml"), AJENO).unwrap();
        // J1 entra como GamePad: el real del usuario queda en la copia
        write_profiles(&dir, &[gamepad(0, 0)]).unwrap();
        let bak = profiles.join("controller0.xml.pepomote.bak");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), AJENO);
        // J1 pide Mando Wii: el mando 1 vuelve a ser el GamePad real del usuario
        write_profiles(&dir, &[CemuPlayer::empty_gamepad(), wii_player(1, 1, 0)]).unwrap();
        assert_eq!(std::fs::read_to_string(profiles.join("controller0.xml")).unwrap(), AJENO);
        assert!(!bak.exists());
        assert!(profiles.join("controller1.xml").is_file());
        // y de vuelta a GamePad: otra vez el móvil, con el real a salvo
        write_profiles(&dir, &[gamepad(0, 0)]).unwrap();
        assert!(is_ours(&std::fs::read_to_string(profiles.join("controller0.xml")).unwrap()));
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), AJENO);
        assert!(!profiles.join("controller1.xml").exists());
        // se va: el real vuelve
        write_profiles(&dir, &[]).unwrap();
        assert_eq!(std::fs::read_to_string(profiles.join("controller0.xml")).unwrap(), AJENO);
    }

    #[test]
    fn la_ventana_del_gamepad_solo_con_un_movil_gamepad() {
        assert!(wants_pad_window(&[gamepad(0, 0)]));
        assert!(wants_pad_window(&[gamepad_screen(0, 0), pro(1, 1)]));
        assert!(!wants_pad_window(&[CemuPlayer::empty_gamepad(), wii_player(1, 1, 0), wii_player(2, 2, 1)]));
        assert!(!wants_pad_window(&[]));
    }

    #[test]
    fn descripcion_con_gamepad_vacio() {
        let d = describe(&[CemuPlayer::empty_gamepad(), wii_player(1, 1, 0), wii_player(2, 2, 1)]);
        assert_eq!(d.matches("GamePad").count(), 1, "{d}");
        assert!(d.contains('1') && d.contains('2') && !d.contains('3'), "los jugadores conservan su número: {d}");
    }

    fn mappings(xml: &str) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let mut rest = xml;
        while let Some(i) = rest.find("<mapping>") {
            let m: u32 = rest[i + 9..].split('<').next().unwrap().parse().unwrap();
            let j = rest[i..].find("<button>").unwrap() + i;
            let b: u32 = rest[j + 8..].split('<').next().unwrap().parse().unwrap();
            out.push((m, b));
            rest = &rest[j + 8..];
        }
        out
    }

    /// Comprobación mínima de XML bien formado: etiquetas equilibradas.
    fn well_formed(xml: &str) -> bool {
        let mut stack: Vec<&str> = Vec::new();
        for tag in xml.split('<').skip(1) {
            let tag = tag.split('>').next().unwrap();
            if tag.starts_with('?') {
                continue;
            }
            if let Some(name) = tag.strip_prefix('/') {
                if stack.pop() != Some(name) {
                    return false;
                }
            } else {
                stack.push(tag.split_whitespace().next().unwrap());
            }
        }
        stack.is_empty()
    }

    #[test]
    fn perfil_gamepad_jugador_1() {
        let xml = profile_xml(&gamepad(0, 0));
        assert!(well_formed(&xml));
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<emulated_controller>\n\t<type>Wii U GamePad</type>"));
        assert!(xml.contains(PROFILE_MARK));
        assert!(!xml.contains("device_type"));
        assert!(xml.contains("<api>DSUController</api>\n\t\t<uuid>0</uuid>"));
        assert!(xml.contains("<motion>true</motion>"), "el GamePad lleva giroscopio");
        assert!(xml.contains("<ip>127.0.0.1</ip>\n\t\t<port>26760</port>"));
        let m = mappings(&xml);
        assert_eq!(m.len(), 27, "los 27 mapeos del GamePad");
        assert!(m.contains(&(1, 14)), "A → Cross");
        assert!(m.contains(&(2, 13)), "B → Circle");
        assert!(m.contains(&(3, 15)), "X → Square");
        assert!(m.contains(&(4, 12)), "Y → Triangle");
        assert!(m.contains(&(7, 42)) && m.contains(&(8, 43)), "ZL/ZR → gatillos analógicos");
        assert!(m.contains(&(17, 39)) && m.contains(&(18, 45)) && m.contains(&(19, 44)) && m.contains(&(20, 38)));
        assert!(m.contains(&(21, 41)) && m.contains(&(22, 47)) && m.contains(&(23, 46)) && m.contains(&(24, 40)));
        assert!(m.contains(&(25, 8)) && m.contains(&(26, 9)), "Mic → L2, Pantalla → R2");
        assert!(m.contains(&(27, 16)), "Home → Touch");
        // El motor virtual es un dispositivo separado sin mapeos de entrada,
        // y solo entra cuando el mando virtual existe (en los tests no hay
        // hub, así que no hay nodo de vibración)
        assert_eq!(xml.matches("<api>DSUController</api>").count(), 1);
        assert_eq!(xml.matches("<controller>").count(), 1);
    }

    #[test]
    fn perfil_pro_jugador_2() {
        let xml = profile_xml(&pro(1, 1));
        assert!(well_formed(&xml));
        assert!(xml.contains("<type>Wii U Pro Controller</type>"));
        assert!(xml.contains("<uuid>1</uuid>"));
        assert!(!xml.contains("<motion>"), "un Pro Controller no tiene movimiento");
        let m = mappings(&xml);
        assert_eq!(m.len(), 25);
        assert!(m.contains(&(11, 16)), "Home (11 en el Pro) → Touch");
        assert!(m.contains(&(18, 39)) && m.contains(&(25, 40)));
    }

    #[test]
    fn perfil_mando_wii_con_y_sin_nunchuk() {
        let xml = profile_xml(&wii(0, 0, Some(3)));
        assert!(well_formed(&xml));
        assert!(xml.contains("<type>Wiimote</type>"));
        assert!(xml.contains("<device_type>6</device_type>"), "MotionPlus + Nunchuk");
        assert_eq!(xml.matches("<api>DSUController</api>").count(), 2, "el Nunchuk es el otro móvil");
        assert_eq!(xml.matches("<controller>").count(), 2);
        assert!(xml.contains("<uuid>0</uuid>") && xml.contains("<uuid>3</uuid>"));
        let m = mappings(&xml);
        assert!(m.contains(&(1, 14)) && m.contains(&(2, 13)) && m.contains(&(3, 15)) && m.contains(&(4, 12)));
        assert!(m.contains(&(17, 16)), "Home → Touch");
        assert!(m.contains(&(5, 11)) && m.contains(&(6, 10)), "Z → R1, C → L1 del pad 3");
        assert!(m.contains(&(13, 39)) && m.contains(&(14, 45)) && m.contains(&(15, 44)) && m.contains(&(16, 38)));
        // el movimiento solo en el mando (el Nunchuk de Cemu no lee un segundo IMU)
        assert_eq!(xml.matches("<motion>true</motion>").count(), 1);
        let solo = profile_xml(&wii(1, 1, None));
        assert!(solo.contains("<device_type>5</device_type>"));
        assert_eq!(solo.matches("<api>DSUController</api>").count(), 1);
        assert_eq!(solo.matches("<controller>").count(), 1);
    }

    #[test]
    fn escribe_por_jugador_y_limpia_los_suyos() {
        let dir = tmp_dir("perfiles");
        write_profiles(&dir, &[gamepad(0, 0), pro(1, 1)]).unwrap();
        let profiles = dir.join("controllerProfiles");
        assert!(profiles.join("controller0.xml").is_file());
        assert!(profiles.join("controller1.xml").is_file());
        // se va el jugador 2: su perfil (nuestro) desaparece; el 1 sigue
        write_profiles(&dir, &[gamepad(0, 0)]).unwrap();
        assert!(profiles.join("controller0.xml").is_file());
        assert!(!profiles.join("controller1.xml").exists());
        // un perfil ajeno en un índice sin jugador no se toca
        std::fs::write(profiles.join("controller2.xml"), "<emulated_controller><type>Wii U Pro Controller</type></emulated_controller>").unwrap();
        write_profiles(&dir, &[gamepad(0, 0)]).unwrap();
        assert!(profiles.join("controller2.xml").is_file());
    }

    /// El mando 1 sí se toma aunque sea del usuario: Cemu exige un Wii U
    /// GamePad ahí y es justo el papel que pide el móvil. Con respaldo, y
    /// vuelve al irse.
    #[test]
    fn backup_del_ajeno_y_restauracion_al_irse() {
        let dir = tmp_dir("backup");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        let ajeno = "<emulated_controller><type>Wii U GamePad</type><controller><api>XInput</api></controller></emulated_controller>";
        std::fs::write(profiles.join("controller0.xml"), ajeno).unwrap();
        write_profiles(&dir, &[gamepad(0, 0)]).unwrap();
        let bak = profiles.join("controller0.xml.pepomote.bak");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), ajeno, "el ajeno queda a salvo");
        assert!(is_ours(&std::fs::read_to_string(profiles.join("controller0.xml")).unwrap()));
        // segunda pasada: el backup no se pisa con nuestra versión
        write_profiles(&dir, &[gamepad(0, 0)]).unwrap();
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), ajeno);
        // se va el móvil: vuelve su mando real
        write_profiles(&dir, &[]).unwrap();
        assert_eq!(std::fs::read_to_string(profiles.join("controller0.xml")).unwrap(), ajeno);
        assert!(!bak.exists());
    }

    /// Perfil de un mando de verdad del usuario en el mando `n`.
    fn suyo(n: u8) -> String {
        format!("<emulated_controller><type>Wii U Pro Controller</type><controller><api>XInput</api><uuid>{n}</uuid></controller></emulated_controller>")
    }

    /// Lo que reportó un usuario jugando a Nintendo Land: él con un móvil de
    /// GamePad y los demás con mandos de verdad; al meter dos móviles más
    /// como Mando Wii, Cemu dejaba de reconocer los mandos de los otros. Nos
    /// estábamos comiendo sus perfiles. Ahora los móviles van detrás.
    #[test]
    fn los_mandos_de_los_otros_jugadores_no_se_tocan() {
        let dir = tmp_dir("ajenos");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        std::fs::write(profiles.join("controller1.xml"), suyo(1)).unwrap();
        std::fs::write(profiles.join("controller2.xml"), suyo(2)).unwrap();

        let avisos = write_profiles(&dir, &[gamepad(0, 0), wii_player(1, 2, 1), wii_player(2, 3, 2)]).unwrap();

        assert!(avisos.is_empty(), "hay sitio de sobra: sin avisos");
        assert_eq!(std::fs::read_to_string(profiles.join("controller1.xml")).unwrap(), suyo(1));
        assert_eq!(std::fs::read_to_string(profiles.join("controller2.xml")).unwrap(), suyo(2));
        assert!(!profiles.join("controller1.xml.pepomote.bak").exists(), "no hay nada que respaldar");
        for n in [0, 3, 4] {
            let x = std::fs::read_to_string(profiles.join(format!("controller{n}.xml"))).unwrap();
            assert!(is_ours(&x), "controller{n}.xml debería ser nuestro");
        }
    }

    /// Al actualizar desde una versión que sí los pisaba, el respaldo delata
    /// que ese mando era del usuario: se le devuelve y el móvil se va detrás,
    /// sin que nadie toque nada.
    #[test]
    fn al_actualizar_se_devuelven_los_mandos_pisados() {
        let dir = tmp_dir("rescate");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        std::fs::write(profiles.join("controller1.xml"), profile_xml(&wii_player(1, 2, 1))).unwrap();
        std::fs::write(profiles.join("controller1.xml.pepomote.bak"), suyo(1)).unwrap();

        write_profiles(&dir, &[gamepad(0, 0), wii_player(1, 2, 1)]).unwrap();

        assert_eq!(std::fs::read_to_string(profiles.join("controller1.xml")).unwrap(), suyo(1), "su mando vuelve");
        assert!(!profiles.join("controller1.xml.pepomote.bak").exists());
        assert!(is_ours(&std::fs::read_to_string(profiles.join("controller2.xml")).unwrap()), "el móvil, detrás");
    }

    /// Cemu solo admite 8 mandos: con todos ocupados por el usuario, el móvil
    /// se queda sin perfil y se le avisa en vez de quitarle el sitio a nadie.
    #[test]
    fn sin_mandos_libres_se_avisa_en_vez_de_pisar() {
        let dir = tmp_dir("lleno");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        for n in 1..MAX_CONTROLLERS {
            std::fs::write(profiles.join(format!("controller{n}.xml")), suyo(n)).unwrap();
        }
        let avisos = write_profiles(&dir, &[gamepad(0, 0), wii_player(1, 2, 3)]).unwrap();
        assert_eq!(avisos, vec![Warning::NoFreeSlot { dsu_slot: 3 }]);
        for n in 1..MAX_CONTROLLERS {
            assert_eq!(std::fs::read_to_string(profiles.join(format!("controller{n}.xml"))).unwrap(), suyo(n));
        }
    }

    #[test]
    fn nodo_solo_pantalla_sin_movimiento_ni_botones() {
        let n = screen_only_node(&gamepad_screen(0, 2), 2);
        assert!(n.starts_with("\t<controller>\n\t\t<api>DSUController</api>\n\t\t<uuid>2</uuid>\n"), "{n}");
        assert!(n.contains("<display_name>PepoMote J1 pantalla</display_name>"));
        assert!(!n.contains("<motion>"));
        assert!(!n.contains("<entry>"));
        assert!(n.contains("<ip>127.0.0.1</ip>\n\t\t<port>26760</port>"));
        assert!(n.ends_with("\t</controller>\n"));
    }

    #[test]
    fn fusion_con_el_mando_del_usuario() {
        let node = screen_only_node(&gamepad_screen(0, 0), 0);
        let merged = merge_screen_only(AJENO, &node).unwrap();
        assert_eq!(merged.matches("<controller>").count(), 2);
        let start = AJENO.find("\t<controller>").unwrap();
        let end = AJENO.find("\t</controller>\n").unwrap() + "\t</controller>\n".len();
        assert!(merged.contains(&AJENO[start..end]), "el mando del usuario queda byte a byte");
        assert!(merged.contains("<type>Wii U GamePad</type>") && merged.contains("<profile>MiMando</profile>"));
        assert!(!is_ours(&merged), "sin la marca: es el perfil del usuario con nuestro nodo");
        assert!(merged.trim_end().ends_with("</controller>\n</emulated_controller>"), "{merged}");
        assert!(merged.find("PepoMote J1 pantalla").unwrap() > merged.find("XInput").unwrap(), "nuestro nodo va el último");
        // idempotente
        assert_eq!(merge_screen_only(&merged, &node).unwrap(), merged);
        // re-guardado por Cemu (4 espacios, nuestro nodo renombrado): siguen siendo 2
        let resaved = merged.replace('\t', "    ").replace("PepoMote J1 pantalla", "Controller 2");
        let again = merge_screen_only(&resaved, &node).unwrap();
        assert_eq!(again.matches("<controller>").count(), 2, "{again}");
        assert!(again.contains("PepoMote J1 pantalla") && again.contains("XInput"));
    }

    #[test]
    fn nodos_pepomote_por_nombre_o_por_servidor() {
        let mine_named = "<controller>\n<api>SDLController</api>\n<display_name>PepoMote J1 GamePad</display_name>\n</controller>\n";
        let mine_server = "<controller>\n<api>DSUController</api>\n<display_name>Controller 3</display_name>\n<ip>127.0.0.1</ip>\n<port>26760</port>\n</controller>\n";
        let other_dsu = "<controller>\n<api>DSUController</api>\n<display_name>Otro DSU</display_name>\n<ip>127.0.0.1</ip>\n<port>26761</port>\n</controller>\n";
        let xinput = "<controller>\n<api>XInput</api>\n<display_name>Controller 1</display_name>\n</controller>\n";
        let xml = format!("<emulated_controller>\n{mine_named}{mine_server}{other_dsu}{xinput}</emulated_controller>\n");
        let stripped = strip_pepomote_nodes(&xml);
        assert!(!stripped.contains("PepoMote") && !stripped.contains("Controller 3"), "{stripped}");
        assert!(stripped.contains("Otro DSU") && stripped.contains("XInput"));
        assert!(has_pepomote_node(&xml) && !has_pepomote_node(&stripped));
        assert!(has_gamepad_type(AJENO) && !has_gamepad_type(&xml));
        assert!(merge_screen_only("no es un perfil", "<controller></controller>\n").is_none());
    }

    #[test]
    fn solo_pantalla_con_copia_y_restauracion() {
        let dir = tmp_dir("screen-only");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        let path = profiles.join("controller0.xml");
        let bak = profiles.join("controller0.xml.pepomote.bak");
        std::fs::write(&path, AJENO).unwrap();
        assert!(write_profiles(&dir, &[gamepad_screen(0, 0)]).unwrap().is_empty(), "GamePad: sin aviso");
        let merged = std::fs::read_to_string(&path).unwrap();
        assert_eq!(merged.matches("<controller>").count(), 2);
        assert!(!is_ours(&merged));
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), AJENO, "copia del ajeno tal cual");
        // segunda pasada: nada cambia
        write_profiles(&dir, &[gamepad_screen(0, 0)]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), merged);
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), AJENO);
        // el móvil pasa a GamePad normal: perfil completo nuestro, la copia sigue
        write_profiles(&dir, &[gamepad(0, 0)]).unwrap();
        assert!(is_ours(&std::fs::read_to_string(&path).unwrap()));
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), AJENO);
        // y vuelve a solo pantalla: se fusiona desde la copia
        write_profiles(&dir, &[gamepad_screen(0, 0)]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), merged);
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), AJENO);
        // se va: el mando del usuario vuelve tal cual
        write_profiles(&dir, &[]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), AJENO);
        assert!(!bak.exists());
    }

    #[test]
    fn solo_pantalla_sin_perfil_ajeno_escribe_el_completo() {
        let dir = tmp_dir("screen-only-nuevo");
        let profiles = dir.join("controllerProfiles");
        write_profiles(&dir, &[gamepad_screen(0, 0)]).unwrap();
        let path = profiles.join("controller0.xml");
        assert!(is_ours(&std::fs::read_to_string(&path).unwrap()));
        assert!(!profiles.join("controller0.xml.pepomote.bak").exists());
        write_profiles(&dir, &[]).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn solo_pantalla_tras_reguardado_de_cemu() {
        let dir = tmp_dir("screen-only-resaved");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        let path = profiles.join("controller0.xml");
        std::fs::write(&path, AJENO).unwrap();
        write_profiles(&dir, &[gamepad_screen(0, 0)]).unwrap();
        let resaved = std::fs::read_to_string(&path).unwrap().replace('\t', "    ").replace("PepoMote J1 pantalla", "Controller 2");
        std::fs::write(&path, &resaved).unwrap();
        write_profiles(&dir, &[gamepad_screen(0, 0)]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap().matches("<controller>").count(), 2, "no se acumulan nodos");
        write_profiles(&dir, &[]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), AJENO);
    }

    #[test]
    fn solo_pantalla_con_tipo_no_gamepad_avisa() {
        let dir = tmp_dir("screen-only-pro");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        std::fs::write(profiles.join("controller0.xml"), AJENO.replace("Wii U GamePad", "Wii U Pro Controller")).unwrap();
        let w = write_profiles(&dir, &[gamepad_screen(0, 0)]).unwrap();
        assert_eq!(w, vec![Warning::NotGamePad { index: 0, dsu_slot: 0 }]);
        let out = std::fs::read_to_string(profiles.join("controller0.xml")).unwrap();
        assert_eq!(out.matches("<controller>").count(), 2);
        assert!(out.contains("Wii U Pro Controller"));
    }

    #[test]
    fn perfil_ajeno_con_nodo_nuestro_y_sin_copia_solo_pierde_el_nodo() {
        let dir = tmp_dir("screen-only-nobak");
        let profiles = dir.join("controllerProfiles");
        std::fs::create_dir_all(&profiles).unwrap();
        let path = profiles.join("controller0.xml");
        let merged = merge_screen_only(AJENO, &screen_only_node(&gamepad_screen(0, 0), 0)).unwrap();
        std::fs::write(&path, &merged).unwrap();
        write_profiles(&dir, &[]).unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(!out.contains("PepoMote") && out.contains("XInput"), "{out}");
    }

    #[test]
    fn sin_cambios_no_se_reescribe() {
        let dir = tmp_dir("nochange");
        let path = dir.join("controller0.xml");
        let xml = profile_xml(&gamepad(0, 0));
        assert!(write_if_changed(&path, &xml).unwrap());
        assert!(!write_if_changed(&path, &xml).unwrap());
        assert!(!backup_path(&path).exists(), "lo nuestro no se respalda");
    }

    #[test]
    fn ventana_del_gamepad_en_settings_xml() {
        let dir = tmp_dir("openpad");
        let path = dir.join("settings.xml");
        // sin archivo: uno mínimo
        ensure_pad_window(&dir).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("<content>\n    <open_pad>true</open_pad>\n</content>"));
        // false → true, conservando el resto y con backup del original
        let cfg = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<content>\n    <logflag>0</logflag>\n    <open_pad>false</open_pad>\n    <pad_size>\n        <x>-1</x>\n    </pad_size>\n</content>\n";
        std::fs::write(&path, cfg).unwrap();
        let _ = std::fs::remove_file(path.with_extension("xml.pepomote.bak"));
        ensure_pad_window(&dir).unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains("<open_pad>true</open_pad>") && out.contains("<logflag>0</logflag>") && out.contains("<pad_size>"));
        assert_eq!(std::fs::read_to_string(path.with_extension("xml.pepomote.bak")).unwrap(), cfg);
        // ya true: no se toca
        ensure_pad_window(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), out);
        // sin la clave: se inserta tras <content>
        std::fs::write(&path, "<content>\n    <logflag>0</logflag>\n</content>\n").unwrap();
        ensure_pad_window(&dir).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("<content>\n    <open_pad>true</open_pad>\n    <logflag>0</logflag>"));
    }

    #[test]
    fn carpetas_de_config_segun_instalacion() {
        let base = tmp_dir("dirs");
        // portable: manda portable/
        let portable = base.join("cemu-portable");
        std::fs::create_dir_all(portable.join("portable")).unwrap();
        let dirs = config_dirs_from(&[portable.clone()]).unwrap();
        assert_eq!(dirs[0], portable.join("portable"));
        // instalación antigua: settings.xml junto al exe
        let old = base.join("cemu-old");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("settings.xml"), "<content/>").unwrap();
        let dirs = config_dirs_from(&[old.clone()]).unwrap();
        assert_eq!(dirs[0], old);
        // instalación normal (2.0-89+): carpeta roaming, aunque no exista aún
        let normal = base.join("Cemu_2.6");
        std::fs::create_dir_all(&normal).unwrap();
        let dirs = config_dirs_from(&[normal]).unwrap();
        let roaming = roaming_dir().unwrap();
        assert!(dirs.contains(&roaming), "{dirs:?}");
    }
}
