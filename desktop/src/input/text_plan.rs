//! Plan de tecleo: del texto que manda el móvil a lo que cada backend sabe
//! hacer, y de lo que NO se pudo hacer al aviso que vuelve al móvil.
//!
//! Todo lo de aquí es puro (sin `cfg` de sistema operativo ni llamadas al
//! SO), como `macos_keys`: así se compila y se prueba en los tres jobs de la
//! CI, y la parte de cada plataforma queda reducida a ejecutar el plan.
//!
//! Regla de oro: **nada se pierde en silencio**. Lo que un backend no puede
//! teclear vuelve en un [`TypeReport`] y el móvil lo enseña.

use super::KeyCode;
use crate::tr;
use std::collections::BTreeMap;

/// Tope de caracteres por mensaje. Un teclado de móvil no manda más; el tope
/// existe para que un pegado enorme no deje el hilo de telemetría (que
/// también mueve el puntero y alimenta el DSU) tecleando durante segundos.
pub const MAX_TEXT: usize = 1024;

/// Corta el texto a [`MAX_TEXT`] caracteres. Corta por frontera de CARÁCTER,
/// nunca de byte: partir un UTF-8 a la mitad haría pánico al rebanar.
pub fn clamp_text(text: &str) -> (&str, bool) {
    match text.char_indices().nth(MAX_TEXT) {
        Some((end, _)) => (&text[..end], true),
        None => (text, false),
    }
}

/// Una operación de tecleo. `Key` es una tecla de nuestro teclado virtual
/// (con Shift si el bool es true); `Unicode` es un carácter sin tecla propia,
/// que cada backend produce como pueda (o no puede, y lo dice).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextOp {
    Key(KeyCode, bool),
    Unicode(char),
}

/// Texto → operaciones. **Aquí no se pierde nada**: lo que no tiene tecla
/// ASCII sale como `Unicode` en vez de desaparecer. Los controles que no son
/// Intro ni retroceso sí se descartan (no son texto). `\r` se descarta para
/// que un CRLF sea un solo Intro.
pub fn text_ops(text: &str) -> Vec<TextOp> {
    let mut out = Vec::new();
    for c in text.chars() {
        let op = match c {
            '\n' => TextOp::Key(KeyCode::Enter, false),
            '\r' => continue,
            '\u{8}' | '\u{7f}' => TextOp::Key(KeyCode::Backspace, false),
            ' ' => TextOp::Key(KeyCode::Space, false),
            'a'..='z' | '0'..='9' => TextOp::Key(KeyCode::Char(c), false),
            'A'..='Z' => TextOp::Key(KeyCode::Char(c.to_ascii_lowercase()), true),
            '-' | '=' | '.' | ',' | '\'' | ';' | '/' | '[' | ']' | '`' | '\\' => {
                TextOp::Key(KeyCode::Char(c), false)
            }
            '_' => TextOp::Key(KeyCode::Char('-'), true),
            '+' => TextOp::Key(KeyCode::Char('='), true),
            '!' => TextOp::Key(KeyCode::Char('1'), true),
            '?' => TextOp::Key(KeyCode::Char('/'), true),
            ':' => TextOp::Key(KeyCode::Char(';'), true),
            '"' => TextOp::Key(KeyCode::Char('\''), true),
            _ if is_control(c) => continue,
            _ => TextOp::Unicode(c),
        };
        out.push(op);
    }
    out
}

/// Control C0/C1: ni es texto ni tiene keysym Unicode válido.
fn is_control(c: char) -> bool {
    let cp = c as u32;
    cp < 0x20 || (0x7f..=0x9f).contains(&cp)
}

// ---------------------------------------------------------------------------
// Lo que no se pudo teclear
// ---------------------------------------------------------------------------

/// Qué se quedó fuera al teclear. Vacío = el texto entró entero y exacto.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeReport {
    /// Caracteres que no se pudieron escribir de ninguna forma (sin repetir).
    pub dropped: Vec<char>,
    /// Caracteres escritos con un sustituto ASCII (`ñ`→`n`), sin repetir.
    pub folded: Vec<(char, char)>,
    /// El SO rechazó la inyección (Windows: ventana elevada, sesión bloqueada).
    pub rejected_by_os: bool,
}

/// Cuántos caracteres se nombran en el aviso antes de poner «…».
const NOTICE_MAX: usize = 8;

impl TypeReport {
    pub fn drop_char(&mut self, c: char) {
        if !self.dropped.contains(&c) {
            self.dropped.push(c);
        }
    }

    pub fn fold_char(&mut self, from: char, to: char) {
        if !self.folded.iter().any(|(f, _)| *f == from) {
            self.folded.push((from, to));
        }
    }

    /// Entero y exacto.
    pub fn is_clean(&self) -> bool {
        self.dropped.is_empty() && self.folded.is_empty() && !self.rejected_by_os
    }
}

/// Aviso para el móvil que escribió, o None si el texto entró entero.
/// Nombra los caracteres concretos: «se perdió algo» obliga a adivinar.
pub fn type_notice(rep: &TypeReport) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if rep.rejected_by_os {
        parts.push(tr!("text.os_refused").to_owned());
    }
    if !rep.folded.is_empty() {
        let list: Vec<String> = rep
            .folded
            .iter()
            .take(NOTICE_MAX)
            .map(|(f, t)| format!("{f}→{t}"))
            .collect();
        parts.push(tr!("text.folded", with_ellipsis(list.join(" "), rep.folded.len())));
    }
    if !rep.dropped.is_empty() {
        let list: Vec<String> = rep.dropped.iter().take(NOTICE_MAX).map(|c| c.to_string()).collect();
        parts.push(tr!("text.dropped", with_ellipsis(list.join(" "), rep.dropped.len())));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn with_ellipsis(list: String, total: usize) -> String {
    if total > NOTICE_MAX {
        format!("{list} …")
    } else {
        list
    }
}

// ---------------------------------------------------------------------------
// Unicode en el teclado virtual de Wayland
// ---------------------------------------------------------------------------

/// Keysym xkb de un carácter: `U00F1` (ñ), `U20AC` (€), `U1F600` (emoji).
/// None para los controles, que xkb no admite.
pub fn unicode_keysym(c: char) -> Option<String> {
    (!is_control(c)).then(|| format!("U{:04X}", c as u32))
}

/// Primer código evdev de repuesto para los caracteres Unicode. Los 64
/// códigos fijos no pasan de `KEY_PREVIOUSSONG` (165), así que de 200 en
/// adelante no hay colisión.
pub const SPARE_FIRST_EVDEV: u16 = 200;

/// Cuántas teclas de repuesto hay. El techo lo pone X11: un keycode > 255 no
/// se le puede entregar a un cliente X11, y Xwayland traduce NUESTRO keymap
/// para los clientes X11 (RetroArch, Cemu, juegos SDL bajo Sway). Con
/// 200 + 8 = 208 como primer código xkb, caben 48 hasta 255.
pub const SPARE_COUNT: usize = 48;

/// Código xkb (evdev + 8) de la tecla de repuesto número `i`.
pub fn spare_xkb(i: usize) -> u16 {
    SPARE_FIRST_EVDEV + 8 + i as u16
}

/// Un tramo de tecleo: si trae `keymap`, hay que subirlo antes de teclear
/// sus `ops`; si es None, sirve el reparto que ya está puesto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub keymap: Option<BTreeMap<char, u16>>,
    pub ops: Vec<TextOp>,
}

/// Trocea el tecleo en tramos, reutilizando el reparto de teclas que ya
/// tiene el compositor siempre que puede.
///
/// - Texto ASCII ⇒ **un solo tramo sin keymap**: el camino de hoy no cambia.
/// - Texto que repite los caracteres del mensaje anterior ⇒ tampoco sube
///   keymap (los idiomas reales reciclan los mismos 8-10 caracteres).
/// - Texto con más de [`SPARE_COUNT`] caracteres distintos ⇒ se parte, y
///   cada tramo estrena reparto. Como lo del tramo anterior ya se tecleó,
///   reasignar sus teclas no rompe nada.
pub fn plan_text(ops: &[TextOp], current: &BTreeMap<char, u16>) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut map = current.clone();
    let mut changed = false;
    let mut buf: Vec<TextOp> = Vec::new();
    for op in ops {
        if let TextOp::Unicode(c) = *op {
            if unicode_keysym(c).is_none() {
                continue; // no debería llegar (text_ops filtra), pero nunca se cuela un control
            }
            if !map.contains_key(&c) {
                match free_spare(&map) {
                    Some(code) => {
                        map.insert(c, code);
                        changed = true;
                    }
                    None => {
                        // Sin teclas libres: se cierra el tramo y se estrena reparto
                        if !buf.is_empty() {
                            chunks.push(Chunk {
                                keymap: changed.then(|| map.clone()),
                                ops: std::mem::take(&mut buf),
                            });
                        }
                        map = BTreeMap::new();
                        map.insert(c, spare_xkb(0));
                        changed = true;
                    }
                }
            }
        }
        buf.push(*op);
    }
    if !buf.is_empty() || chunks.is_empty() {
        chunks.push(Chunk {
            keymap: changed.then(|| map.clone()),
            ops: buf,
        });
    }
    chunks
}

/// La primera tecla de repuesto que no esté asignada en `map`.
fn free_spare(map: &BTreeMap<char, u16>) -> Option<u16> {
    (0..SPARE_COUNT).map(spare_xkb).find(|code| !map.values().any(|v| v == code))
}

// ---------------------------------------------------------------------------
// Sustituto ASCII (uinput sin camino X11: GNOME/KDE en Wayland)
// ---------------------------------------------------------------------------

/// Caracteres con sustituto ASCII 1:1 y su sustituto, en el mismo orden.
const FOLD_FROM: &str = "áàâäãåÁÀÂÄÃÅéèêëÉÈÊËíìîïÍÌÎÏóòôöõÓÒÔÖÕúùûüÚÙÛÜñÑçÇýÿÝšŠžŽ";
const FOLD_TO: &str = "aaaaaaAAAAAAeeeeEEEEiiiiIIIIoooooOOOOOuuuuUUUUnNcCyyYsSzZ";

/// Signos tipográficos con equivalente ASCII.
const FOLD_EXTRA: &[(char, char)] = &[
    ('\u{a0}', ' '),      // espacio duro
    ('\u{2018}', '\''),   // ‘
    ('\u{2019}', '\''),   // ’
    ('\u{201c}', '"'),    // “
    ('\u{201d}', '"'),    // ”
    ('\u{ab}', '"'),      // «
    ('\u{bb}', '"'),      // »
    ('\u{2013}', '-'),    // –
    ('\u{2014}', '-'),    // —
];

/// Sustituto ASCII de un carácter, o None si no hay uno 1:1 honrado
/// (`€`, emoji, kanji: esos se dicen, no se inventan).
pub fn ascii_fold(c: char) -> Option<char> {
    if let Some((_, to)) = FOLD_EXTRA.iter().find(|(from, _)| *from == c) {
        return Some(*to);
    }
    FOLD_FROM.chars().position(|f| f == c).and_then(|i| FOLD_TO.chars().nth(i))
}

/// `PEPOMOTE_TEXT_FALLBACK=skip` deja el hueco en vez de plegar a ASCII.
/// De serie se pliega: `canon` avisado es utilizable; `caón` no es ni una
/// cosa ni la otra. Callarse no es una opción en ningún caso.
pub fn folding_enabled(env: Option<&str>) -> bool {
    !matches!(
        env.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("skip") | Some("none")
    )
}

/// Keymap xkb en texto, en el formato que usa `wtype`: tipos y
/// compatibilidad del sistema (`include "complete"`) y símbolos propios.
///
/// `keys` va de código xkb (evdev + 8) a (keysym base, keysym con Shift). Una
/// tecla con un solo símbolo recibe el tipo `ONE_LEVEL`, así que sale igual
/// haya o no un modificador pulsado: justo lo que hace falta para las teclas
/// de repuesto de Unicode.
///
/// Está aquí, y no en el backend de Wayland, para poder probar el texto
/// exacto en los tres sistemas: es lo único que ve el compositor.
pub fn render_keymap(keys: &BTreeMap<u16, (String, Option<String>)>, shift_code: u16) -> String {
    let max = keys.keys().last().copied().unwrap_or(8);
    let mut s = String::new();
    s.push_str("xkb_keymap {\n\txkb_keycodes \"(unnamed)\" {\n\t\tminimum = 8;\n");
    s.push_str(&format!("\t\tmaximum = {max};\n"));
    for n in keys.keys() {
        s.push_str(&format!("\t\t<K{n}> = {n};\n"));
    }
    s.push_str("\t};\n");
    s.push_str("\txkb_types \"(unnamed)\" { include \"complete\" };\n");
    s.push_str("\txkb_compatibility \"(unnamed)\" { include \"complete\" };\n");
    s.push_str("\txkb_symbols \"(unnamed)\" {\n");
    for (n, (base, shifted)) in keys {
        match shifted {
            Some(sh) => s.push_str(&format!("\t\tkey <K{n}> {{ [ {base}, {sh} ] }};\n")),
            None => s.push_str(&format!("\t\tkey <K{n}> {{ [ {base} ] }};\n")),
        }
    }
    s.push_str(&format!("\t\tmodifier_map Shift {{ <K{shift_code}> }};\n"));
    s.push_str("\t};\n};\n");
    s
}

// ---------------------------------------------------------------------------
// Unicode por XTEST (sesiones X11)
// ---------------------------------------------------------------------------

/// ¿Es texto que se puede teclear? `\r` y los demás controles no lo son;
/// Intro, retroceso y tabulador sí.
pub fn keyable(c: char) -> bool {
    match c {
        '\n' | '\u{8}' | '\u{7f}' | '\t' => true,
        '\r' => false,
        _ => !is_control(c),
    }
}

/// X11 **de verdad**: hay `DISPLAY` y no estamos en una sesión Wayland.
///
/// Bajo GNOME o KDE en Wayland también hay `DISPLAY` (Xwayland), pero ahí
/// XTEST solo llegaría a las ventanas X11: teclear en unas aplicaciones sí y
/// en otras no, en silencio, sería peor que el agujero que arreglamos.
pub fn x11_available(display: Option<&str>, wayland: Option<&str>, session: Option<&str>) -> bool {
    let hay_display = display.is_some_and(|d| !d.trim().is_empty());
    let en_wayland = wayland.is_some_and(|w| !w.trim().is_empty())
        || session.is_some_and(|s| s.trim().eq_ignore_ascii_case("wayland"));
    hay_display && !en_wayland
}

/// Keysym X11 de un carácter. Latin-1 va tal cual (herencia del protocolo);
/// el resto, con el prefijo Unicode de X11.
pub fn x11_keysym(c: char) -> u32 {
    match c {
        '\n' => 0xff0d,              // XK_Return
        '\u{8}' | '\u{7f}' => 0xff08, // XK_BackSpace
        '\t' => 0xff09,              // XK_Tab
        _ => {
            let cp = c as u32;
            if (0x20..=0xff).contains(&cp) {
                cp
            } else {
                0x0100_0000 + cp
            }
        }
    }
}

/// Keycodes SIN ningún keysym en el mapa actual: los que se pueden tomar
/// prestados sin pisarle nada al usuario. `keysyms` es la respuesta de
/// `GetKeyboardMapping` tal cual (`per_code` símbolos por keycode).
pub fn x11_free_keycodes(first: u8, per_code: u8, keysyms: &[u32], want: usize) -> Vec<u8> {
    if per_code == 0 || want == 0 {
        return Vec::new();
    }
    let per = per_code as usize;
    let mut out = Vec::new();
    for (i, syms) in keysyms.chunks(per).enumerate() {
        let Some(code) = u8::try_from(i).ok().and_then(|i| first.checked_add(i)) else {
            break;
        };
        if syms.len() == per && syms.iter().all(|s| *s == 0) {
            out.push(code);
            if out.len() == want {
                break;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Lotes de SendInput (Windows)
// ---------------------------------------------------------------------------

/// Tecla de un lote de Windows: unidad UTF-16 (`KEYEVENTF_UNICODE`) o tecla
/// virtual (Intro y retroceso, que no son texto).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WinKey {
    Unicode(u16),
    Vk(u16),
}

pub const WIN_VK_RETURN: u16 = 0x0D;
pub const WIN_VK_BACK: u16 = 0x08;

/// Eventos por llamada a `SendInput`.
pub const WIN_BATCH: usize = 256;

/// Texto → lotes de `(tecla, soltar)` para `SendInput`.
///
/// Un lote por mensaje (troceado) en vez de una llamada por flanco: Windows
/// garantiza que los eventos de UNA llamada no se intercalan con las teclas
/// reales del usuario, y las dos unidades del par suplente de un emoji
/// llegan juntas (sueltas, algunas apps las parten). Nunca se parte un
/// carácter entre dos lotes.
pub fn win_text_plan(text: &str) -> Vec<Vec<(WinKey, bool)>> {
    let mut out: Vec<Vec<(WinKey, bool)>> = Vec::new();
    let mut batch: Vec<(WinKey, bool)> = Vec::new();
    for c in text.chars() {
        let mut unit: Vec<(WinKey, bool)> = Vec::new();
        match c {
            '\n' => press(&mut unit, WinKey::Vk(WIN_VK_RETURN)),
            '\r' => continue,
            '\u{8}' | '\u{7f}' => press(&mut unit, WinKey::Vk(WIN_VK_BACK)),
            _ => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    press(&mut unit, WinKey::Unicode(*u));
                }
            }
        }
        if !batch.is_empty() && batch.len() + unit.len() > WIN_BATCH {
            out.push(std::mem::take(&mut batch));
        }
        batch.extend(unit);
    }
    if !batch.is_empty() {
        out.push(batch);
    }
    out
}

fn press(unit: &mut Vec<(WinKey, bool)>, key: WinKey) {
    unit.push((key, false));
    unit.push((key, true));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unicodes(ops: &[TextOp]) -> Vec<char> {
        ops.iter()
            .filter_map(|o| match o {
                TextOp::Unicode(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn texto_a_operaciones_no_pierde_nada() {
        assert_eq!(
            text_ops("Link 2\n"),
            vec![
                TextOp::Key(KeyCode::Char('l'), true),
                TextOp::Key(KeyCode::Char('i'), false),
                TextOp::Key(KeyCode::Char('n'), false),
                TextOp::Key(KeyCode::Char('k'), false),
                TextOp::Key(KeyCode::Space, false),
                TextOp::Key(KeyCode::Char('2'), false),
                TextOp::Key(KeyCode::Enter, false),
            ]
        );
        assert_eq!(text_ops("\u{8}"), vec![TextOp::Key(KeyCode::Backspace, false)]);
        // lo que antes se tiraba en silencio ahora sale como Unicode
        assert_eq!(unicodes(&text_ops("ñá€@#😀")), vec!['ñ', 'á', '€', '@', '#', '😀']);
        // CRLF = UN Intro
        assert_eq!(text_ops("a\r\n").len(), 2);
        assert_eq!(text_ops("\r"), vec![]);
        // controles: ni tecla ni Unicode
        assert_eq!(text_ops("\u{1}\u{1b}\u{9f}"), vec![]);
        assert_eq!(
            text_ops("!?"),
            vec![TextOp::Key(KeyCode::Char('1'), true), TextOp::Key(KeyCode::Char('/'), true)]
        );
    }

    #[test]
    fn el_tope_de_longitud_corta_por_caracter_entero() {
        let corto = "ñ".repeat(10);
        assert_eq!(clamp_text(&corto), (corto.as_str(), false));
        let largo = "ñ".repeat(MAX_TEXT + 5);
        let (cut, truncado) = clamp_text(&largo);
        assert!(truncado);
        assert_eq!(cut.chars().count(), MAX_TEXT);
        assert!(cut.ends_with('ñ'), "no parte un carácter multibyte");
    }

    #[test]
    fn keysym_unicode_de_bmp_y_astral() {
        assert_eq!(unicode_keysym('ñ').as_deref(), Some("U00F1"));
        assert_eq!(unicode_keysym('€').as_deref(), Some("U20AC"));
        assert_eq!(unicode_keysym('😀').as_deref(), Some("U1F600"));
        assert_eq!(unicode_keysym('#').as_deref(), Some("U0023"));
        assert_eq!(unicode_keysym('\u{1}'), None);
        assert_eq!(unicode_keysym('\u{9f}'), None);
    }

    #[test]
    fn las_teclas_de_repuesto_caben_en_x11_y_no_chocan() {
        assert_eq!(spare_xkb(0), 208);
        assert_eq!(spare_xkb(SPARE_COUNT - 1), 255, "el techo de X11 es 255");
        // los códigos fijos más altos (KEY_PREVIOUSSONG = 165) quedan lejos
        assert!(SPARE_FIRST_EVDEV > 165);
    }

    #[test]
    fn el_plan_no_toca_el_keymap_para_ascii() {
        let plan = plan_text(&text_ops("abc\n"), &BTreeMap::new());
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].keymap, None, "ASCII no sube keymap");
        assert_eq!(plan[0].ops.len(), 4);
    }

    #[test]
    fn el_plan_reutiliza_las_teclas_ya_puestas() {
        let current = BTreeMap::from([('ñ', 208u16)]);
        let plan = plan_text(&text_ops("añb"), &current);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].keymap, None, "ya estaba mapeada: nada que subir");
    }

    #[test]
    fn el_plan_asigna_solo_lo_nuevo() {
        let current = BTreeMap::from([('ñ', 208u16)]);
        let plan = plan_text(&text_ops("ñá"), &current);
        assert_eq!(plan.len(), 1);
        let map = plan[0].keymap.clone().expect("hay que subir keymap");
        assert_eq!(map[&'ñ'], 208, "la vieja conserva su tecla");
        assert_eq!(map[&'á'], 209, "la nueva coge la siguiente libre");
    }

    #[test]
    fn el_plan_trocea_cuando_no_caben_los_distintos() {
        // 100 caracteres Unicode distintos, fuera del ASCII
        let texto: String = (0..100).map(|i| char::from_u32(0x100 + i).unwrap()).collect();
        let ops = text_ops(&texto);
        let plan = plan_text(&ops, &BTreeMap::new());
        assert!(plan.len() >= 3, "{} tramos", plan.len());
        // el texto sale entero y en orden
        let vuelta: Vec<TextOp> = plan.iter().flat_map(|c| c.ops.clone()).collect();
        assert_eq!(vuelta, ops);
        // y cada tramo lleva en su mapa todo lo que teclea
        let mut vivo: BTreeMap<char, u16> = BTreeMap::new();
        for chunk in &plan {
            if let Some(m) = &chunk.keymap {
                assert!(m.len() <= SPARE_COUNT);
                let codigos: Vec<u16> = m.values().copied().collect();
                let unicos: std::collections::BTreeSet<u16> = codigos.iter().copied().collect();
                assert_eq!(codigos.len(), unicos.len(), "dos caracteres con la misma tecla");
                vivo = m.clone();
            }
            for op in &chunk.ops {
                if let TextOp::Unicode(c) = op {
                    assert!(vivo.contains_key(c), "{c} sin tecla en su tramo");
                }
            }
        }
    }

    #[test]
    fn plegado_ascii_de_acentos_y_signos() {
        assert_eq!(FOLD_FROM.chars().count(), FOLD_TO.chars().count());
        assert_eq!(ascii_fold('ñ'), Some('n'));
        assert_eq!(ascii_fold('Ñ'), Some('N'));
        assert_eq!(ascii_fold('á'), Some('a'));
        assert_eq!(ascii_fold('ü'), Some('u'));
        assert_eq!(ascii_fold('\u{201c}'), Some('"'));
        assert_eq!(ascii_fold('€'), None, "no hay sustituto honrado");
        assert_eq!(ascii_fold('😀'), None);
        assert_eq!(ascii_fold('a'), None, "el ASCII no se pliega");
    }

    #[test]
    fn el_plegado_se_puede_apagar() {
        assert!(folding_enabled(None));
        assert!(folding_enabled(Some("")));
        assert!(!folding_enabled(Some("skip")));
        assert!(!folding_enabled(Some(" SKIP ")));
        assert!(!folding_enabled(Some("none")));
    }

    #[test]
    fn el_informe_no_repite_y_el_aviso_nombra_los_caracteres() {
        let mut rep = TypeReport::default();
        assert!(rep.is_clean());
        assert_eq!(type_notice(&rep), None);
        rep.drop_char('😀');
        rep.drop_char('😀');
        rep.fold_char('ñ', 'n');
        rep.fold_char('ñ', 'n');
        assert_eq!(rep.dropped, vec!['😀']);
        assert_eq!(rep.folded, vec![('ñ', 'n')]);
        assert!(!rep.is_clean());
        let aviso = type_notice(&rep).unwrap();
        assert!(aviso.contains('😀'), "{aviso}");
        assert!(aviso.contains("ñ→n"), "{aviso}");
    }

    #[test]
    fn el_aviso_corta_la_lista_larga() {
        let mut rep = TypeReport::default();
        for i in 0..20 {
            rep.drop_char(char::from_u32(0x4e00 + i).unwrap());
        }
        let aviso = type_notice(&rep).unwrap();
        assert!(aviso.contains('…'), "{aviso}");
    }

    #[test]
    fn el_keymap_declara_cada_tecla_y_sus_simbolos() {
        let keys = BTreeMap::from([
            (38u16, ("a".to_owned(), Some("A".to_owned()))),
            (spare_xkb(0), (unicode_keysym('ñ').unwrap(), None)),
            (spare_xkb(1), (unicode_keysym('😀').unwrap(), None)),
        ]);
        let text = render_keymap(&keys, 50);
        assert!(text.starts_with("xkb_keymap {\n\txkb_keycodes \"(unnamed)\" {\n\t\tminimum = 8;\n"));
        assert!(text.contains("maximum = 209;"), "{text}");
        for needle in [
            "\t\t<K38> = 38;\n",
            "\t\t<K208> = 208;\n",
            "\t\t<K209> = 209;\n",
            "\t\tkey <K38> { [ a, A ] };\n",
            // un solo nivel: inmune a un Shift pulsado
            "\t\tkey <K208> { [ U00F1 ] };\n",
            "\t\tkey <K209> { [ U1F600 ] };\n",
            "\t\tmodifier_map Shift { <K50> };\n",
        ] {
            assert_eq!(text.matches(needle).count(), 1, "{needle}");
        }
        assert_eq!(text.matches("include \"complete\"").count(), 2);
        assert_eq!(text.matches("\t\tkey <").count(), 3);
        assert!(text.ends_with("};\n};\n"));
    }

    #[test]
    fn solo_es_x11_de_verdad_si_no_hay_sesion_wayland() {
        assert!(x11_available(Some(":0"), None, None));
        assert!(x11_available(Some(":0"), Some(""), Some("x11")));
        assert!(!x11_available(Some(":0"), Some("wayland-1"), None), "Xwayland no cuenta");
        assert!(!x11_available(Some(":0"), None, Some("wayland")), "ni la sesión Wayland");
        assert!(!x11_available(Some(":0"), None, Some(" WAYLAND ")));
        assert!(!x11_available(None, None, None));
        assert!(!x11_available(Some(""), None, None));
    }

    #[test]
    fn keysym_x11_de_un_caracter() {
        assert_eq!(x11_keysym('a'), 0x61);
        assert_eq!(x11_keysym('ñ'), 0xf1, "Latin-1 va tal cual");
        assert_eq!(x11_keysym('€'), 0x0100_20ac);
        assert_eq!(x11_keysym('😀'), 0x0101_f600);
        assert_eq!(x11_keysym('\n'), 0xff0d);
        assert_eq!(x11_keysym('\u{8}'), 0xff08);
    }

    #[test]
    fn keycodes_libres_del_mapa_actual() {
        // 4 keycodes desde el 8, 2 símbolos cada uno: libres el 9 y el 11
        let syms = [0x61, 0x41, 0, 0, 0x62, 0x42, 0, 0];
        assert_eq!(x11_free_keycodes(8, 2, &syms, 4), vec![9, 11]);
        assert_eq!(x11_free_keycodes(8, 2, &syms, 1), vec![9], "respeta el tope");
        assert_eq!(x11_free_keycodes(8, 0, &syms, 4), Vec::<u8>::new(), "sin símbolos por tecla");
        assert_eq!(x11_free_keycodes(8, 2, &[], 4), Vec::<u8>::new());
    }

    #[test]
    fn lo_que_se_puede_teclear() {
        assert!(keyable('a') && keyable('ñ') && keyable('😀') && keyable('\n') && keyable('\u{8}'));
        assert!(!keyable('\r') && !keyable('\u{1}') && !keyable('\u{9f}'));
    }

    #[test]
    fn el_plan_de_windows_manda_el_par_suplente_de_un_emoji_en_un_solo_lote() {
        let lotes = win_text_plan("😀");
        assert_eq!(lotes.len(), 1);
        let (alto, bajo) = {
            let mut buf = [0u16; 2];
            let u = '😀'.encode_utf16(&mut buf);
            (u[0], u[1])
        };
        assert_eq!(
            lotes[0],
            vec![
                (WinKey::Unicode(alto), false),
                (WinKey::Unicode(alto), true),
                (WinKey::Unicode(bajo), false),
                (WinKey::Unicode(bajo), true),
            ]
        );
    }

    #[test]
    fn el_plan_de_windows_usa_teclas_virtuales_para_intro_y_retroceso() {
        let lotes = win_text_plan("a\n\u{8}\r");
        assert_eq!(lotes.len(), 1);
        assert_eq!(
            lotes[0],
            vec![
                (WinKey::Unicode(b'a' as u16), false),
                (WinKey::Unicode(b'a' as u16), true),
                (WinKey::Vk(WIN_VK_RETURN), false),
                (WinKey::Vk(WIN_VK_RETURN), true),
                (WinKey::Vk(WIN_VK_BACK), false),
                (WinKey::Vk(WIN_VK_BACK), true),
            ],
            "\\r no genera nada"
        );
    }

    #[test]
    fn el_plan_de_windows_trocea_sin_partir_un_caracter() {
        let lotes = win_text_plan(&"😀".repeat(200));
        assert!(lotes.len() > 1);
        for lote in &lotes {
            assert!(lote.len() <= WIN_BATCH);
            assert_eq!(lote.len() % 4, 0, "un emoji entero son 4 eventos: no se parte");
        }
        let total: usize = lotes.iter().map(|l| l.len()).sum();
        assert_eq!(total, 200 * 4);
    }
}
