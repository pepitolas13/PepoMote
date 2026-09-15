use std::path::Path;

/// INI partido en (preámbulo, secciones ordenadas). Conserva líneas tal cual.
pub(crate) struct Ini {
    pub(crate) preamble: Vec<String>,
    pub(crate) sections: Vec<(String, Vec<String>)>,
    pub(crate) crlf: bool,
    headers: Vec<String>,
    bom: bool,
    trailing_newline: bool,
}

pub(crate) fn parse_ini(text: &str) -> Ini {
    let mut ini = Ini {
        preamble: Vec::new(),
        sections: Vec::new(),
        crlf: text.contains("\r\n"),
        headers: Vec::new(),
        bom: text.starts_with('\u{feff}'),
        trailing_newline: text.is_empty() || text.ends_with('\n'),
    };
    for line in text.trim_start_matches('\u{feff}').lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            ini.sections
                .push((t[1..t.len() - 1].to_owned(), Vec::new()));
            ini.headers.push(line.to_owned());
        } else if let Some((_, body)) = ini.sections.last_mut() {
            body.push(line.to_owned());
        } else {
            ini.preamble.push(line.to_owned());
        }
    }
    ini
}

pub(crate) fn serialize_ini(ini: &Ini) -> String {
    let nl = if ini.crlf { "\r\n" } else { "\n" };
    let mut lines = ini.preamble.clone();
    for (i, (name, body)) in ini.sections.iter().enumerate() {
        lines.push(
            ini.headers
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("[{name}]")),
        );
        lines.extend(body.iter().cloned());
    }
    let mut out = if ini.bom {
        "\u{feff}".to_owned()
    } else {
        String::new()
    };
    out.push_str(&lines.join(nl));
    if ini.trailing_newline && !lines.is_empty() {
        out.push_str(nl);
    }
    out
}

pub(crate) fn set_section(ini: &mut Ini, name: &str, body: Vec<String>) {
    if let Some((_, b)) = ini.sections.iter_mut().find(|(n, _)| n == name) {
        *b = body;
    } else {
        ini.sections.push((name.to_owned(), body));
        ini.headers.push(format!("[{name}]"));
    }
}

/// Clave de una línea `Clave = valor` de INI (None para comentarios/vacías).
pub(crate) fn ini_key(line: &str) -> Option<&str> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
        return None;
    }
    Some(t.split_once('=')?.0.trim()).filter(|k| !k.is_empty())
}

/// Escribe el INI solo si cambia algo. El backup `.pepomote.bak` se hace UNA
/// vez (el archivo tal como estaba antes de que PepoMote lo tocara nunca):
/// re-copiarlo en cada pasada lo habría sustituido por nuestra propia versión.
pub(crate) fn write_if_changed(path: &Path, original: &str, new: String) -> Result<(), String> {
    if original == new {
        return Ok(());
    }
    if path.exists() {
        let bak = path.with_extension("ini.pepomote.bak");
        if !bak.exists() {
            std::fs::copy(path, bak).map_err(|e| e.to_string())?;
        }
    }
    atomic_write(path, &new)
}

/// Pone `key = value` en la sección `section` del INI (creando la sección si
/// no existe; una clave nueva va la primera); el resto de la sección y del
/// archivo se conservan. Solo escribe si cambia algo.
pub(crate) fn set_ini_key(
    path: &Path,
    section: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let original = std::fs::read_to_string(path).unwrap_or_default();
    let mut ini = parse_ini(&original);
    let mut body: Vec<String> = ini
        .sections
        .iter()
        .find(|(n, _)| n == section)
        .map(|(_, b)| b.clone())
        .unwrap_or_default();
    let mut found = false;
    for l in body.iter_mut() {
        if ini_key(l) == Some(key) {
            *l = format!("{key} = {value}");
            found = true;
        }
    }
    if !found {
        body.insert(0, format!("{key} = {value}"));
    }
    set_section(&mut ini, section, body);
    write_if_changed(path, &original, serialize_ini(&ini))
}

pub(crate) fn raw_value<'a>(body: &'a [String], key: &str) -> Option<&'a str> {
    body.iter()
        .rev()
        .find(|l| ini_key(l) == Some(key))
        .and_then(|l| l.split_once('=').map(|(_, v)| v.trim()))
}

pub(crate) fn unquoted(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
}

/// Qt ignora el valor si `key\default` falta o es true.
pub(crate) fn qt_effective<'a>(body: &'a [String], key: &str, default: &'a str) -> &'a str {
    if raw_value(body, &format!("{key}\\default")) != Some("false") {
        return default;
    }
    raw_value(body, key).map(unquoted).unwrap_or(default)
}

pub(crate) fn quoted(value: &str) -> String {
    if value.contains(',') {
        format!("\"{value}\"")
    } else {
        value.to_owned()
    }
}

pub(crate) fn remove_qt_key(body: &mut Vec<String>, key: &str) {
    let flag = format!("{key}\\default");
    body.retain(|l| ini_key(l).is_none_or(|k| k != key && k != flag));
}

pub(crate) fn set_qt_key(body: &mut Vec<String>, key: &str, value: &str) {
    let flag = format!("{key}\\default");
    // Si Qt las ha reordenado pero ya son efectivas, no reescribir.
    if raw_value(body, &flag) == Some("false")
        && raw_value(body, key) == Some(value)
        && body.iter().filter(|l| ini_key(l) == Some(key)).count() == 1
        && body
            .iter()
            .filter(|l| ini_key(l) == Some(flag.as_str()))
            .count()
            == 1
    {
        return;
    }
    let at = body
        .iter()
        .position(|l| ini_key(l).is_some_and(|k| k == key || k == flag))
        .unwrap_or(body.len());
    remove_qt_key(body, key);
    let at = at.min(body.len());
    body.splice(at..at, [format!("{flag}=false"), format!("{key}={value}")]);
}

/// Completar y sincronizar el archivo antes de sustituir el original.
pub(crate) fn atomic_write(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let temp = std::path::PathBuf::from(name);
    let result = (|| {
        let mut f = std::fs::File::create(&temp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crlf_roundtrip_preserves_untouched_bytes() {
        let original = "[Audio]\r\nvolume=80\r\n\r\n[Controls]\r\nkey=value\r\n";
        assert_eq!(serialize_ini(&parse_ini(original)), original);
    }
    #[test]
    fn qt_flags_quotes_and_removal() {
        let mut body = vec![
            ";key=comment".into(),
            "key=ignored".into(),
            "other=true".into(),
        ];
        assert_eq!(qt_effective(&body, "key", "default"), "default");
        set_qt_key(&mut body, "key", &quoted("a:1,b:2"));
        assert_eq!(qt_effective(&body, "key", "default"), "a:1,b:2");
        assert_eq!(&body[1..3], &["key\\default=false", "key=\"a:1,b:2\""]);
        let before = body.clone();
        set_qt_key(&mut body, "key", &quoted("a:1,b:2"));
        assert_eq!(body, before);
        remove_qt_key(&mut body, "key");
        assert_eq!(body, vec![";key=comment", "other=true"]);
    }
    #[test]
    fn bom_headers_and_no_trailing_newline() {
        let original = "\u{feff} [Audio] \nvolume = 80";
        assert_eq!(serialize_ini(&parse_ini(original)), original);
    }
}
