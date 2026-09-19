//! Texto con CUALQUIER carácter en sesiones X11 (Mint, XFCE, Cinnamon, MATE,
//! i3…), donde el backend de inyección es uinput.
//!
//! uinput manda *keycodes*: lo que salga de cada uno lo decide la
//! disposición del usuario, así que `ñ` o `€` solo aparecerían si su teclado
//! los tuviera donde nosotros creemos. En X11 sí se puede hacer bien: se
//! toma prestado un keycode que no use nadie, se le pone el keysym exacto
//! del carácter y se pulsa por XTEST (la receta de `xdotool`). Al soltar el
//! inyector se devuelven los keycodes a como estaban.
//!
//! Dos mejoras sobre `xdotool`: el keysym se escribe en TODOS los niveles de
//! la tecla, así que sale igual aunque el usuario tenga Shift pulsado (sin
//! tener que soltarle los modificadores), y el remapeo es uno por mensaje,
//! no uno por carácter.
//!
//! `x11rb` ya era dependencia (captura de pantalla en X11) y es Rust puro:
//! la feature `xtest` no añade ni una biblioteca dinámica, así que la
//! comprobación de dependencias de la CI de Linux sigue pasando.

use super::text_plan::{keyable, x11_free_keycodes, x11_keysym, TypeReport};
use std::collections::BTreeMap;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{ConnectionExt as _, Keycode, KEY_PRESS_EVENT, KEY_RELEASE_EVENT};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

/// Cuántos keycodes se toman prestados como mucho. Un mensaje real usa 6-10
/// caracteres distintos sin tecla; con 48 sobra y el resto se trocea.
const MAX_SPARES: usize = 48;

/// Espera tras remapear (ms): GTK y Qt releen el mapa de forma asíncrona al
/// recibir el MappingNotify. Se paga una vez por mensaje, y solo si el mapa
/// cambió. `PEPOMOTE_X11_SETTLE_MS=0` la quita.
fn settle_ms() -> u64 {
    std::env::var("PEPOMOTE_X11_SETTLE_MS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(30)
}

/// ¿Estamos en una sesión X11 de verdad? (mira el entorno, no abre nada)
pub fn available() -> bool {
    super::text_plan::x11_available(
        std::env::var("DISPLAY").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
    )
}

/// Conexión X propia para teclear texto, con sus keycodes prestados.
pub struct X11Text {
    conn: RustConnection,
    root: u32,
    /// Símbolos por keycode que usa este servidor (se rellenan todos).
    per_code: u8,
    /// Keycodes prestados, en orden.
    spares: Vec<Keycode>,
    /// Reparto vivo carácter → keycode. Se conserva entre mensajes: repetir
    /// los mismos caracteres no vuelve a remapear (ni a esperar).
    live: BTreeMap<char, Keycode>,
}

impl X11Text {
    /// Abre la conexión y busca keycodes libres. None si no hay servidor X,
    /// si le falta XTEST o si no queda ni un keycode sin usar.
    pub fn open() -> Option<Self> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        // XTEST está en cualquier X.Org, pero se comprueba antes de contar con él
        conn.xtest_get_version(2, 2).ok()?.reply().ok()?;
        let (root, first, count) = {
            let setup = conn.setup();
            let first = setup.min_keycode;
            let count = setup.max_keycode.checked_sub(first)?.checked_add(1)?;
            (setup.roots.get(screen)?.root, first, count)
        };
        let map = conn.get_keyboard_mapping(first, count).ok()?.reply().ok()?;
        let spares = x11_free_keycodes(first, map.keysyms_per_keycode, &map.keysyms, MAX_SPARES);
        if spares.is_empty() {
            return None;
        }
        Some(Self {
            conn,
            root,
            per_code: map.keysyms_per_keycode,
            spares,
            live: BTreeMap::new(),
        })
    }

    /// Teclea el texto entero. Un error aquí es la conexión X caída: el que
    /// llama tira este camino y sigue por donde pueda.
    pub fn type_text(&mut self, text: &str) -> Result<TypeReport, ReplyError> {
        let chars: Vec<char> = text.chars().filter(|c| keyable(*c)).collect();
        let mut i = 0;
        while i < chars.len() {
            let (map, changed, end) = self.plan_chunk(&chars, i);
            if changed {
                self.remap(&map)?;
            }
            for c in &chars[i..end] {
                if let Some(code) = map.get(c).copied() {
                    self.conn.xtest_fake_input(KEY_PRESS_EVENT, code, 0, self.root, 0, 0, 0)?;
                    self.conn.xtest_fake_input(KEY_RELEASE_EVENT, code, 0, self.root, 0, 0, 0)?;
                }
            }
            self.conn.flush()?;
            self.live = map;
            i = end;
        }
        // Por XTEST entra cualquier carácter: no hay nada que decir
        Ok(TypeReport::default())
    }

    /// Reparto de teclas para el tramo que empieza en `from`: se hereda lo
    /// que ya está puesto y solo se asigna lo nuevo. Devuelve (reparto, si
    /// hay que subirlo, dónde acaba el tramo).
    fn plan_chunk(&self, chars: &[char], from: usize) -> (BTreeMap<char, Keycode>, bool, usize) {
        let mut map = self.live.clone();
        let mut changed = false;
        let mut end = from;
        while end < chars.len() {
            let c = chars[end];
            if !map.contains_key(&c) {
                match self.free_spare(&map) {
                    Some(code) => {
                        map.insert(c, code);
                        changed = true;
                    }
                    None if end == from => {
                        // El reparto heredado está lleno y no sirve: se estrena
                        map.clear();
                        changed = true;
                        continue;
                    }
                    None => break,
                }
            }
            end += 1;
        }
        (map, changed, end)
    }

    /// El primer keycode prestado que no esté ya asignado.
    fn free_spare(&self, map: &BTreeMap<char, Keycode>) -> Option<Keycode> {
        self.spares.iter().copied().find(|kc| !map.values().any(|v| v == kc))
    }

    fn remap(&mut self, map: &BTreeMap<char, Keycode>) -> Result<(), ReplyError> {
        for (c, code) in map {
            // El mismo keysym en todos los niveles: inmune a un Shift pulsado
            let syms = vec![x11_keysym(*c); self.per_code as usize];
            self.conn.change_keyboard_mapping(1, *code, self.per_code, &syms)?;
        }
        // Ida y vuelta: el servidor ya ha procesado el remapeo y emitido su
        // MappingNotify antes de que llegue la primera tecla
        self.conn.get_input_focus()?.reply()?;
        let settle = settle_ms();
        if settle > 0 {
            std::thread::sleep(Duration::from_millis(settle));
        }
        Ok(())
    }
}

impl Drop for X11Text {
    /// Devuelve a NoSymbol TODOS los keycodes prestados —no solo los del
    /// último tramo, que se van reasignando— para que el teclado del usuario
    /// quede exactamente como estaba.
    fn drop(&mut self) {
        let syms = vec![0u32; self.per_code as usize];
        for code in &self.spares {
            let _ = self.conn.change_keyboard_mapping(1, *code, self.per_code, &syms);
        }
        let _ = self.conn.flush();
    }
}
