//! Idioma de la interfaz: español o inglés. Compartido con el móvil Linux
//! por `#[path]`; cada app trae su tabla de textos en `crate::strings::TABLE`
//! (clave, español, inglés). Por defecto, el idioma del sistema; si no es
//! inglés (o no se sabe), español. Se traduce lo que ve el usuario; el
//! protocolo, el log y `--diag` van siempre en español.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    /// Código guardado en la configuración ("es" / "en").
    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "es",
            Lang::En => "en",
        }
    }

    /// Nombre del idioma, en el propio idioma (para el botón de cambio).
    pub fn name(self) -> &'static str {
        match self {
            Lang::Es => "Español",
            Lang::En => "English",
        }
    }

    pub fn parse(s: &str) -> Option<Lang> {
        match s.trim().to_ascii_lowercase().as_str() {
            "es" => Some(Lang::Es),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    pub fn other(self) -> Lang {
        match self {
            Lang::Es => Lang::En,
            Lang::En => Lang::Es,
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 {
        Lang::En
    } else {
        Lang::Es
    }
}

pub fn set(l: Lang) {
    LANG.store(if l == Lang::En { 1 } else { 0 }, Ordering::Relaxed);
}

/// Cambia al otro idioma y lo devuelve.
pub fn toggle() -> Lang {
    let n = current().other();
    set(n);
    n
}

/// Idioma del sistema: inglés si lo es; español si es español o no se sabe.
pub fn detect_system() -> Lang {
    #[cfg(windows)]
    {
        // Idioma de la interfaz de Windows (LANGID): 0x09 = inglés. Directo
        // de kernel32, que este archivo también lo compila el móvil Linux
        // (sin el crate `windows`) cuando se prueba en Windows.
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        let id = unsafe { GetUserDefaultUILanguage() };
        return if id & 0x3FF == 0x09 { Lang::En } else { Lang::Es };
    }
    #[cfg(not(windows))]
    {
        let vars: Vec<Option<String>> = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .map(|v| std::env::var(v).ok())
            .collect();
        from_env_values(&vars.iter().map(|v| v.as_deref()).collect::<Vec<_>>())
    }
}

/// Como `detect_system` a partir de las variables de entorno (la primera
/// que diga algo): "en_US.UTF-8" → inglés; "es_ES", "de_DE", "C" o nada → español.
pub fn from_env_values(vars: &[Option<&str>]) -> Lang {
    for v in vars.iter().flatten() {
        let v = v.trim();
        if v.is_empty() || v == "C" || v == "POSIX" {
            continue;
        }
        return if v.to_ascii_lowercase().starts_with("en") { Lang::En } else { Lang::Es };
    }
    Lang::Es
}

fn table() -> &'static HashMap<&'static str, (&'static str, &'static str)> {
    static TABLE: OnceLock<HashMap<&'static str, (&'static str, &'static str)>> = OnceLock::new();
    TABLE.get_or_init(|| crate::strings::TABLE.iter().map(|(k, es, en)| (*k, (*es, *en))).collect())
}

/// El texto de `key` en el idioma activo. Una clave desconocida (un error
/// de programación) se enseña entre corchetes, sin pánico.
pub fn t(key: &str) -> &'static str {
    match table().get(key) {
        Some((es, en)) => {
            if current() == Lang::En {
                en
            } else {
                es
            }
        }
        None => Box::leak(format!("[{key}]").into_boxed_str()),
    }
}

/// Rellena `{0}`, `{1}`… de una plantilla.
pub fn fill(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = template.to_owned();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), &a.to_string());
    }
    out
}

/// `tr!("clave")` → `&'static str`; `tr!("clave", a, b)` → `String` con
/// `{0}`, `{1}` rellenados.
#[macro_export]
macro_rules! tr {
    ($k:expr) => {
        $crate::i18n::t($k)
    };
    ($k:expr, $($a:expr),+ $(,)?) => {
        $crate::i18n::fill($crate::i18n::t($k), &[$(&$a as &dyn ::std::fmt::Display),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn el_idioma_del_sistema_por_variables() {
        assert_eq!(from_env_values(&[None, None, None]), Lang::Es, "sin nada: español");
        assert_eq!(from_env_values(&[Some("C"), Some(""), Some("POSIX")]), Lang::Es);
        assert_eq!(from_env_values(&[None, Some("en_US.UTF-8")]), Lang::En);
        assert_eq!(from_env_values(&[Some("es_ES.UTF-8"), Some("en_GB")]), Lang::Es, "la primera manda");
        assert_eq!(from_env_values(&[Some("C"), Some("en")]), Lang::En, "C no cuenta");
        assert_eq!(from_env_values(&[Some("de_DE")]), Lang::Es, "desconocido: español");
    }

    #[test]
    fn codigos_y_nombres() {
        assert_eq!(Lang::parse("EN "), Some(Lang::En));
        assert_eq!(Lang::parse("es"), Some(Lang::Es));
        assert_eq!(Lang::parse("fr"), None);
        assert_eq!(Lang::Es.other(), Lang::En);
        assert_eq!(Lang::En.other().code(), "es");
        assert_eq!(Lang::En.name(), "English");
    }

    #[test]
    fn rellenar_plantillas() {
        assert_eq!(fill("{0} de {1}", &[&2, &"tres"]), "2 de tres");
        assert_eq!(fill("sin huecos", &[&1]), "sin huecos");
        assert_eq!(fill("{0}{0}", &[&"ab"]), "abab");
    }

    #[test]
    fn la_tabla_esta_completa_y_sin_repetidos() {
        let mut keys = HashSet::new();
        for (k, es, en) in crate::strings::TABLE {
            assert!(keys.insert(*k), "clave repetida: {k}");
            assert!(!es.is_empty() && !en.is_empty(), "{k}: texto vacío");
            let holes = |s: &str| -> Vec<String> {
                let mut v: Vec<String> = (0..9).map(|i| format!("{{{i}}}")).filter(|h| s.contains(h.as_str())).collect();
                v.sort();
                v
            };
            assert_eq!(holes(es), holes(en), "{k}: los {{n}} no coinciden entre idiomas");
        }
        // toda clave usada en el código existe en la tabla
        let mut used = HashSet::new();
        for src in crate::strings::SOURCES {
            for pat in ["tr!(", "i18n::t("] {
                let mut rest = *src;
                while let Some(i) = rest.find(pat) {
                    // `include_str!(` también acaba en `tr!(`: solo cuenta si
                    // no viene pegado a un identificador
                    let glued = rest[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_');
                    rest = rest[i + pat.len()..].trim_start();
                    if glued {
                        continue;
                    }
                    if let Some(body) = rest.strip_prefix('"') {
                        if let Some(j) = body.find('"') {
                            used.insert(body[..j].to_owned());
                        }
                    }
                }
            }
        }
        assert!(!used.is_empty(), "el escaneo de fuentes no encontró ninguna clave");
        let missing: Vec<&String> = used.iter().filter(|k| !keys.contains(k.as_str())).collect();
        assert!(missing.is_empty(), "claves usadas sin traducción: {missing:?}");
        let unused: Vec<&&str> = keys.iter().filter(|k| !used.contains(**k)).collect();
        assert!(unused.is_empty(), "traducciones sin usar: {unused:?}");
    }

    #[test]
    fn una_clave_desconocida_no_revienta() {
        assert_eq!(t("no.existe"), "[no.existe]");
    }
}
