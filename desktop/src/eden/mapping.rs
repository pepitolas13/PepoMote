//! Valores de QSettings que consume el motor cemuhookudp de Eden.
use crate::ini::{
    self, parse_ini, qt_effective, quoted, serialize_ini, set_qt_key, set_section, Ini,
};
use crate::state::SwitchPlayer;
use std::net::Ipv4Addr;

pub(super) const DSU_IP: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);
pub(super) const MAX_PLAYERS: u8 = 8;
pub(super) const GLOBALS: [&str; 2] = ["udp_input_servers", "enable_udp_controller"];

pub(super) const BUTTONS: [(&str, u32); 22] = [
    ("a", 8192),
    ("b", 16384),
    ("x", 4096),
    ("y", 32768),
    ("lstick", 2),
    ("rstick", 4),
    ("l", 1024),
    ("r", 2048),
    ("zl", 256),
    ("zr", 512),
    ("plus", 8),
    ("minus", 1),
    ("dleft", 128),
    ("dup", 16),
    ("dright", 32),
    ("ddown", 64),
    ("slleft", 0),
    ("srleft", 0),
    ("home", 262144),
    ("screenshot", 524288),
    ("slright", 0),
    ("srright", 0),
];

pub(super) fn dsu_guid(ip: Ipv4Addr) -> String {
    format!("{:032x}", u32::from(ip))
}

pub(super) fn controls(ini: &Ini) -> &[String] {
    ini.sections
        .iter()
        .find(|(n, _)| n == "Controls")
        .map(|(_, b)| b.as_slice())
        .unwrap_or(&[])
}

pub(super) fn managed_keys(index: u8) -> Vec<String> {
    let mut keys = vec!["type".to_owned(), "connected".to_owned()];
    keys.extend(BUTTONS.iter().map(|(n, _)| format!("button_{n}")));
    keys.extend(
        ["lstick", "rstick", "motionleft", "motionright"]
            .iter()
            .map(|s| (*s).to_owned()),
    );
    keys.into_iter()
        .map(|k| format!("player_{index}_{k}"))
        .collect()
}

pub(super) fn player_values(pl: &SwitchPlayer, guid: &str, port: u16) -> Vec<(String, String)> {
    let bind = |fields: &str, slot: u8| {
        quoted(&format!(
            "{fields},engine:cemuhookudp,guid:{guid},pad:{slot},port:{port}"
        ))
    };
    let axis =
        |fields: &str, slot: u8| bind(&format!("{fields},deadzone:0.050000,range:1.000000"), slot);
    let button = |id: u32, slot: u8| {
        if id == 0 {
            "[empty]".to_owned()
        } else {
            bind(&format!("button:{id}"), slot)
        }
    };
    let slot = pl.dsu_slot;
    let mut values = vec![
        ("type".into(), "0".into()),
        ("connected".into(), "true".into()),
    ];
    for (name, id) in BUTTONS {
        values.push((format!("button_{name}"), button(id, slot)));
    }
    values.extend([
        ("lstick".into(), axis("axis_x:0,axis_y:1", slot)),
        ("rstick".into(), axis("axis_x:2,axis_y:3", slot)),
        ("motionleft".into(), bind("motion:0", slot)),
        ("motionright".into(), bind("motion:0", slot)),
    ]);
    values
        .into_iter()
        .map(|(k, v)| (format!("player_{}_{k}", pl.index), v))
        .collect()
}

pub(super) fn is_ours(value: &str, guid: &str, port: u16) -> bool {
    let mut engine = false;
    let mut ip = false;
    let mut p = false;
    for field in ini::unquoted(value).split(',') {
        if let Some((k, v)) = field.split_once(':') {
            match k {
                "engine" => engine = v == "cemuhookudp",
                "guid" => ip = v == guid,
                "port" => p = v.parse::<u16>() == Ok(port),
                _ => {}
            }
        }
    }
    engine && ip && p
}

pub(super) fn player_is_ours(body: &[String], index: u8, guid: &str, port: u16) -> bool {
    managed_keys(index)
        .iter()
        .any(|key| ini::raw_value(body, key).is_some_and(|v| is_ours(v, guid, port)))
}

pub(super) fn server_list(body: &[String], port: u16) -> Vec<String> {
    let own_server = format!("{DSU_IP}:{port}");
    let mut servers: Vec<String> = Vec::new();
    for server in qt_effective(body, GLOBALS[0], "127.0.0.1:26760")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !servers.iter().any(|s| s == server) {
            servers.push(server.to_owned());
        }
    }
    if !servers.contains(&own_server) {
        servers.push(own_server);
    }
    servers
}

pub(super) fn apply_layout(
    original: &str,
    layout: &[SwitchPlayer],
    guid: &str,
    port: u16,
) -> String {
    let mut ini = parse_ini(original);
    let mut body = controls(&ini).to_vec();
    // Eden usa índices globales (servidor * 4 + slot), incluso con GUID.
    // Añadir al final conserva las asignaciones de los servidores ajenos.
    let servers = server_list(&body, port);
    let own_server = format!("{DSU_IP}:{port}");
    let offset = 4 * servers.iter().position(|s| s == &own_server).unwrap_or(0) as u8;
    for pl in layout {
        let global = SwitchPlayer {
            dsu_slot: pl.dsu_slot + offset,
            ..*pl
        };
        for (key, value) in player_values(&global, guid, port) {
            set_qt_key(&mut body, &key, &value);
        }
    }
    for i in 0..MAX_PLAYERS {
        if !layout.iter().any(|p| p.index == i) && player_is_ours(&body, i, guid, port) {
            set_qt_key(&mut body, &format!("player_{i}_connected"), "false");
        }
    }
    set_qt_key(&mut body, GLOBALS[0], &quoted(&servers.join(",")));
    set_qt_key(&mut body, GLOBALS[1], "true");
    set_section(&mut ini, "Controls", body);
    serialize_ini(&ini)
}

/// Copia solo las claves pedidas, con sus flags originales (incluso ausentes).
pub(super) fn copy_keys(body: &mut Vec<String>, source: &[String], keys: &[String]) {
    for key in keys {
        let flag = format!("{key}\\default");
        let at = body
            .iter()
            .position(|l| ini::ini_key(l).is_some_and(|k| k == key || k == flag))
            .unwrap_or(body.len());
        let saved: Vec<String> = source
            .iter()
            .filter(|l| ini::ini_key(l).is_some_and(|k| k == key || k == flag))
            .cloned()
            .collect();
        ini::remove_qt_key(body, key);
        let at = at.min(body.len());
        body.splice(at..at, saved);
    }
}

pub(super) fn restore_keys(current: &str, backup: &str, guid: &str, port: u16) -> String {
    let mut ini = parse_ini(current);
    let saved = parse_ini(backup);
    let mut body = controls(&ini).to_vec();
    let ours: Vec<u8> = (0..MAX_PLAYERS)
        .filter(|i| player_is_ours(&body, *i, guid, port))
        .collect();
    if ours.is_empty() {
        return current.to_owned();
    }
    for i in ours {
        copy_keys(&mut body, controls(&saved), &managed_keys(i));
    }
    let expected = server_list(controls(&saved), port).join(",");
    // Si el usuario ha cambiado la lista mientras usaba PepoMote, conservarla
    // entera: quitar nuestro servidor renumeraría los mandos añadidos después.
    // El servidor extra sin bindings es inocuo; perder esos bindings no lo es.
    if qt_effective(&body, GLOBALS[0], "127.0.0.1:26760") == expected {
        copy_keys(&mut body, controls(&saved), &[GLOBALS[0].to_owned()]);
        if qt_effective(&body, GLOBALS[1], "false") == "true" {
            copy_keys(&mut body, controls(&saved), &[GLOBALS[1].to_owned()]);
        }
    }
    set_section(&mut ini, "Controls", body);
    serialize_ini(&ini)
}
