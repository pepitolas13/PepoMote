//! `retroarch.cfg`: texto `clave = "valor"` (una por línea, `#` comenta).
//! Solo se tocan las claves de PepoMote; el resto del archivo se conserva
//! byte a byte (RetroArch lo reescribe entero al salir, ordenado, y vuelve a
//! guardar estas mismas claves: son ajustes suyos de siempre).
use super::link::Ports;

/// Jugadores que se habilitan en RetroArch (uno por slot de PepoMote).
pub const USERS: u8 = crate::net::MAX_PLAYERS as u8;

/// Las claves con los valores que PepoMote necesita.
pub fn managed_values(ports: Ports) -> Vec<(String, String)> {
    let mut v = vec![
        ("network_remote_enable".to_owned(), "true".to_owned()),
        ("network_remote_base_port".to_owned(), ports.base.to_string()),
        ("network_cmd_enable".to_owned(), "true".to_owned()),
        ("network_cmd_port".to_owned(), ports.cmd.to_string()),
        ("menu_mouse_enable".to_owned(), "true".to_owned()),
    ];
    for u in 1..=USERS {
        v.push((format!("network_remote_enable_user_p{u}"), "true".to_owned()));
    }
    v
}

pub fn managed_keys(ports: Ports) -> Vec<String> {
    managed_values(ports).into_iter().map(|(k, _)| k).collect()
}

/// Clave de una línea (`clave = "valor"`); None en comentarios y vacías.
fn key_of(line: &str) -> Option<&str> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    Some(t.split_once('=')?.0.trim()).filter(|k| !k.is_empty())
}

/// Valor de una línea, sin comillas.
fn value_of(line: &str) -> Option<&str> {
    let (_, v) = line.trim().split_once('=')?;
    Some(v.trim().trim_matches('"'))
}

pub fn get(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find(|l| key_of(l) == Some(key))
        .and_then(value_of)
        .map(str::to_owned)
}

fn newline(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Pone `values` en el texto: sustituye la línea de cada clave que exista
/// (la primera; las repetidas se quitan) y añade al final las que no.
pub fn apply(text: &str, values: &[(String, String)]) -> String {
    let nl = newline(text);
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + values.len());
    let mut done = vec![false; values.len()];
    for line in lines.drain(..) {
        match key_of(&line).and_then(|k| values.iter().position(|(vk, _)| vk == k)) {
            Some(i) if !done[i] => {
                done[i] = true;
                out.push(format!("{} = \"{}\"", values[i].0, values[i].1));
            }
            Some(_) => {} // repetida: fuera
            None => out.push(line),
        }
    }
    for (i, (k, v)) in values.iter().enumerate() {
        if !done[i] {
            out.push(format!("{k} = \"{v}\""));
        }
    }
    let mut s = out.join(nl);
    s.push_str(nl);
    s
}

/// ¿Ya está todo como PepoMote lo quiere?
pub fn needs_change(text: &str, values: &[(String, String)]) -> bool {
    values.iter().any(|(k, v)| get(text, k).as_deref() != Some(v.as_str()))
}

/// Devuelve las claves `keys` a como estaban en `backup`: con su valor de
/// entonces, o fuera si no existían. Lo demás se queda como está ahora.
pub fn restore(current: &str, backup: &str, keys: &[String]) -> String {
    let nl = newline(current);
    let mut out: Vec<String> = Vec::new();
    let mut restored: Vec<(String, String)> = Vec::new();
    for k in keys {
        if let Some(v) = get(backup, k) {
            restored.push((k.clone(), v));
        }
    }
    let mut done = vec![false; restored.len()];
    for line in current.lines() {
        match key_of(line) {
            Some(k) if keys.iter().any(|m| m == k) => {
                if let Some(i) = restored.iter().position(|(rk, _)| rk == k) {
                    if !done[i] {
                        done[i] = true;
                        out.push(format!("{} = \"{}\"", restored[i].0, restored[i].1));
                    }
                }
                // no estaba en la copia: fuera
            }
            _ => out.push(line.to_owned()),
        }
    }
    for (i, (k, v)) in restored.iter().enumerate() {
        if !done[i] {
            out.push(format!("{k} = \"{v}\""));
        }
    }
    if out.is_empty() {
        return String::new();
    }
    let mut s = out.join(nl);
    s.push_str(nl);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORTS: Ports = Ports { base: 55400, cmd: 55355 };

    fn vals() -> Vec<(String, String)> {
        managed_values(PORTS)
    }

    #[test]
    fn claves_gestionadas() {
        let v = vals();
        assert_eq!(v.len(), 5 + USERS as usize);
        assert!(v.contains(&("network_remote_enable".into(), "true".into())));
        assert!(v.contains(&("network_remote_base_port".into(), "55400".into())));
        assert!(v.contains(&("network_cmd_port".into(), "55355".into())));
        assert!(v.contains(&("network_remote_enable_user_p1".into(), "true".into())));
        assert!(v.contains(&("network_remote_enable_user_p4".into(), "true".into())));
        assert!(!managed_keys(PORTS).iter().any(|k| k == "network_remote_enable_user_p5"));
    }

    #[test]
    fn el_raton_del_menu_se_activa_y_se_restaura() {
        for original in ["", "menu_mouse_enable = \"false\"\n"] {
            let configured = apply(original, &vals());
            assert_eq!(get(&configured, "menu_mouse_enable").as_deref(), Some("true"));
            assert_eq!(restore(&configured, original, &managed_keys(PORTS)), original);
        }
    }

    #[test]
    fn sustituye_en_sitio_y_anade_lo_que_falta() {
        let original = "audio_enable = \"true\"\nnetwork_remote_enable = \"false\"\n# comentario\nvideo_fullscreen = \"false\"\n";
        let new = apply(original, &vals());
        assert!(new.starts_with("audio_enable = \"true\"\nnetwork_remote_enable = \"true\"\n# comentario\nvideo_fullscreen = \"false\"\n"));
        assert!(new.contains("network_remote_base_port = \"55400\"\n"));
        assert!(new.contains("network_cmd_enable = \"true\"\n"));
        assert!(new.ends_with("network_remote_enable_user_p4 = \"true\"\n"));
        assert_eq!(new.matches("network_remote_enable =").count(), 1);
        assert!(!needs_change(&new, &vals()));
        assert!(needs_change(original, &vals()));
        // idempotente
        assert_eq!(apply(&new, &vals()), new);
    }

    #[test]
    fn conserva_crlf_y_quita_repetidas() {
        let original = "a = \"1\"\r\nnetwork_cmd_enable = false\r\nnetwork_cmd_enable = \"true\"\r\n";
        let new = apply(original, &vals());
        assert!(new.contains("\r\n"));
        assert!(!new.contains("\n\n"));
        assert_eq!(new.matches("network_cmd_enable").count(), 1);
        assert_eq!(get(&new, "network_cmd_enable").as_deref(), Some("true"));
        assert_eq!(get(&new, "a").as_deref(), Some("1"));
    }

    #[test]
    fn un_archivo_vacio_queda_solo_con_lo_nuestro() {
        let new = apply("", &vals());
        assert_eq!(new.lines().count(), vals().len());
        assert!(!needs_change(&new, &vals()));
    }

    #[test]
    fn lee_valores_con_y_sin_comillas() {
        let t = "x = \"a b\"\ny=c\n z = \"\"\n";
        assert_eq!(get(t, "x").as_deref(), Some("a b"));
        assert_eq!(get(t, "y").as_deref(), Some("c"));
        assert_eq!(get(t, "z").as_deref(), Some(""));
        assert_eq!(get(t, "w"), None);
    }

    #[test]
    fn restaurar_devuelve_lo_de_antes_y_quita_lo_que_no_existia() {
        let backup = "audio_enable = \"true\"\nnetwork_remote_enable = \"false\"\nnetwork_cmd_port = \"55355\"\n";
        let current = apply(backup, &vals());
        let restored = restore(&current, backup, &managed_keys(PORTS));
        assert_eq!(get(&restored, "network_remote_enable").as_deref(), Some("false"));
        assert_eq!(get(&restored, "network_cmd_port").as_deref(), Some("55355"));
        assert_eq!(get(&restored, "network_cmd_enable"), None, "no estaba: fuera");
        assert_eq!(get(&restored, "network_remote_enable_user_p1"), None);
        assert_eq!(get(&restored, "audio_enable").as_deref(), Some("true"));
        // otras claves que RetroArch añadió después se conservan
        let current2 = format!("{current}video_fullscreen = \"true\"\n");
        let restored2 = restore(&current2, backup, &managed_keys(PORTS));
        assert_eq!(get(&restored2, "video_fullscreen").as_deref(), Some("true"));
        // sin copia con esas claves y sin nuestras claves: idéntico
        assert_eq!(restore(backup, backup, &managed_keys(PORTS)), backup);
    }
}
