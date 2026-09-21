//! Servidor DSU (cemuhook) — verificado contra la spec comunitaria y el
//! código de Dolphin (protocol/DSU.md). Hasta 4 mandos, uno por slot.
//!
//! Este hilo SOLO atiende las peticiones de Dolphin (version/PortInfo/registro
//! PadData, ~1/s). El streaming de PadData sale inline del hilo de telemetría
//! (dsu::Dsu::push) para no añadir latencia.

use crate::state::LockTolerant;
use super::{mapping, Client, Clients, DsuProfile, MotionSample, SlotSamples};
use crate::net::MAX_PLAYERS;
use crate::state::SharedState;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

pub const DSU_PORT: u16 = 26760;
const PROTOCOL_VERSION: u16 = 1001;
const SERVER_ID: u32 = 0x50455030; // "0EPP"

const MSG_VERSION: u32 = 0x100000;
const MSG_PORT_INFO: u32 = 0x100001;
const MSG_PAD_DATA: u32 = 0x100002;
/// Extensión «rumble» no oficial (protocol/DSU.md): cuántos motores tiene
/// un mando, y órdenes de vibración. Ningún emulador la habla hoy (el DSU
/// oficial no lleva salidas); se atiende por si algún cliente sí.
const MSG_MOTORS: u32 = 0x110001;
const MSG_RUMBLE: u32 = 0x110002;
/// Motores que declaramos por mando: uno, como el Mando de Wii.
const MOTOR_COUNT: u8 = 1;

use super::CLIENT_TTL;

/// Con más de esto sin muestras del móvil, ese slot se reporta desconectado.
const PAD_TTL: Duration = Duration::from_secs(1);

/// Cada cuánto se le manda a un cliente el PadData de un slot que NO tiene
/// muestras frescas (el móvil se ha callado o todavía no ha empezado).
///
/// Cemu solo vuelve a pedir un pad cuando recibe ESE pad: pide los suyos una
/// vez al arrancar y, a partir de ahí, su única forma de seguir pidiéndolo es
/// que le contestemos (medido: 1.218.620 peticiones en 43 s para un pad que
/// contesta y exactamente 2, las del arranque, para uno que no). Si el móvil
/// se calla más de [`super::CLIENT_TTL`], la suscripción caduca, Cemu no
/// vuelve a pedirlo nunca y ese mando queda muerto hasta reiniciar Cemu:
/// bastaba con apagar la pantalla del móvil tres segundos. Con el latido la
/// conversación no se corta y el mando vuelve solo en cuanto el móvil emite.
const BEAT_EVERY: Duration = Duration::from_millis(500);

/// MAC estable por slot: "PMP1" + 0x00 + slot.
fn mac(slot: u8) -> [u8; 6] {
    [0x50, 0x4D, 0x50, 0x31, 0x00, slot]
}

const ALL_SLOTS: u8 = (1 << MAX_PLAYERS) - 1;

/// Bitmask de slots que pide un PadData request (tras el tipo: flags u8,
/// pad_id u8, mac [6]). flags 0 = todos; bit0 = por pad_id; bit1 = por MAC
/// (combinables). Dolphin usa bit0 con el índice de cada uno de sus mandos.
fn subscribed_slots(payload: &[u8]) -> u8 {
    let flags = payload.first().copied().unwrap_or(0);
    if flags == 0 {
        return ALL_SLOTS;
    }
    let mut mask = 0u8;
    if flags & 1 != 0 {
        if let Some(&id) = payload.get(1) {
            if (id as usize) < MAX_PLAYERS {
                mask |= 1 << id;
            }
        }
    }
    if flags & 2 != 0 {
        if let Some(m) = payload.get(2..8) {
            if m[..5] == mac(0)[..5] && (m[5] as usize) < MAX_PLAYERS {
                mask |= 1 << m[5];
            }
        }
    }
    mask
}

pub fn run(
    shared: SharedState,
    socket: UdpSocket,
    clients: Clients,
    last: SlotSamples,
    counter: std::sync::Arc<std::sync::atomic::AtomicU32>,
) {
    let _ = socket.set_read_timeout(Some(Duration::from_millis(250)));
    let mut buf = [0u8; 128];
    let mut last_sweep = Instant::now();
    let mut last_beat = Instant::now();
    // PEPOMOTE_DEBUG=1: traza de cada petición DSU (¿Dolphin nos habla?)
    let debug = std::env::var_os("PEPOMOTE_DEBUG").is_some();

    loop {
        if let Ok((len, from)) = socket.recv_from(&mut buf) {
            let parsed = parse_request(&buf[..len]);
            if debug {
                match parsed {
                    Some((t, p)) => eprintln!("[dsu] {from} tipo={t:#x} payload={:02x?}", &p[..p.len().min(8)]),
                    None => eprintln!("[dsu] {from} paquete no válido ({len} bytes): {:02x?}", &buf[..len.min(20)]),
                }
            }
            if let Some((msg_type, payload)) = parsed {
                match msg_type {
                    MSG_VERSION => {
                        let mut out = Vec::with_capacity(22);
                        out.extend_from_slice(&MSG_VERSION.to_le_bytes());
                        out.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
                        let _ = socket.send_to(&finish(out), from);
                    }
                    MSG_PORT_INFO => {
                        let samples = last.lock_tolerant();
                        let count = payload
                            .first_chunk::<4>()
                            .map(|c| i32::from_le_bytes(*c))
                            .unwrap_or(0)
                            .clamp(0, 4) as usize;
                        for i in 0..count {
                            let slot = payload.get(4 + i).copied().unwrap_or(i as u8);
                            let info = slot_info(slot, &samples);
                            let mut out = Vec::with_capacity(32);
                            out.extend_from_slice(&MSG_PORT_INFO.to_le_bytes());
                            out.extend_from_slice(&info);
                            out.push(0);
                            let _ = socket.send_to(&finish(out), from);
                        }
                    }
                    MSG_PAD_DATA => {
                        // Se ACUMULA con lo que ese cliente ya pidió (Cemu pide
                        // un pad por petición, desde un solo socket)
                        let mask = subscribed_slots(payload);
                        clients.lock_tolerant().entry(from).or_insert_with(Client::new).register(mask, Instant::now());
                    }
                    MSG_MOTORS => {
                        let samples = last.lock_tolerant();
                        for slot in slots_of(subscribed_slots(payload)) {
                            let _ = socket.send_to(&motors_packet(slot, &samples), from);
                        }
                    }
                    MSG_RUMBLE => {
                        if let Some((mask, intensity)) = parse_rumble(payload) {
                            for slot in slots_of(mask) {
                                crate::rumble::set_from_dsu(slot, intensity);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if last_beat.elapsed() >= BEAT_EVERY {
            last_beat = Instant::now();
            beat(&socket, &clients, &last, &counter);
        }

        if last_sweep.elapsed() > Duration::from_secs(1) {
            last_sweep = Instant::now();
            let now = Instant::now();
            // Un candado cada vez: con los dos cogidos, un hilo esperando el
            // estado bloqueaba también el PadData de la telemetría
            let vivos = {
                let mut c = clients.lock_tolerant();
                c.retain(|_, cl| cl.alive(CLIENT_TTL, now));
                c.len()
            };
            shared.lock_tolerant().dsu_clients = vivos;
        }
    }
}

/// Un PadData «ahí no hay nadie» para cada slot pedido cuyo móvil no está
/// emitiendo: mantiene viva la conversación con el emulador (ver
/// [`BEAT_EVERY`]) y de paso le dice la verdad, que ese mando no está.
/// Los slots que SÍ emiten no se tocan: de esos ya se encarga el hilo de
/// telemetría, que es el que manda rápido y sin colas.
fn beat(
    socket: &UdpSocket,
    clients: &Clients,
    last: &SlotSamples,
    counter: &std::sync::atomic::AtomicU32,
) {
    let now = Instant::now();
    // El perfil de la ULTIMA muestra de cada slot: lo que decide donde esta
    // el neutro de los sticks del latido (y si Right X es el centinela del
    // puntero IR). Sin muestra nunca, el perfil Wii, que es el de Dolphin.
    let (fresh, perfil) = {
        let samples = last.lock_tolerant();
        let mut out = [false; MAX_PLAYERS];
        let mut perfil = [DsuProfile::default(); MAX_PLAYERS];
        for (i, s) in samples.iter().enumerate() {
            out[i] = matches!(s, Some((_, t)) if t.elapsed() < PAD_TTL);
            if let Some((m, _)) = s {
                perfil[i] = m.profile;
            }
        }
        (out, perfil)
    };
    if fresh.iter().all(|f| *f) {
        return;
    }
    let clients = clients.lock_tolerant();
    for (addr, c) in clients.iter() {
        for (slot, emitiendo) in fresh.iter().enumerate() {
            if *emitiendo || !c.wants(slot, CLIENT_TTL, now) {
                continue;
            }
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed).wrapping_add(1);
            let _ = socket.send_to(&pad_absent_packet(slot as u8, n, perfil[slot]), addr);
        }
    }
}

/// PadData de 100 bytes de un mando que no está: la cabecera dice
/// «desconectado», los botones y el movimiento van a cero y los sticks al
/// NEUTRO del perfil.
///
/// Los sticks no pueden ir a cero aunque el paquete diga «desconectado»:
/// Eden/yuzu no mira ese campo y calcula `(v − 127) / 127`, así que un 0 es
/// el stick a tope; Dolphin copia el paquete entero antes de que su hotplug
/// retire el mando, y sus ejes son `(v − 128) / ±128`. Con un móvil callado
/// un segundo, el personaje se iba solo a la esquina.
///
/// En el perfil Wii, Right X = 0 es el centinela DELIBERADO de «el puntero
/// IR está fuera de la cámara» (protocol/DSU.md), así que ahí se deja a cero.
fn pad_absent_packet(slot: u8, counter: u32, profile: DsuProfile) -> Vec<u8> {
    let centro = profile.stick_center().clamp(0, 255) as u8;
    let rx = if profile == DsuProfile::Wii { 0 } else { centro };
    let mut p = Vec::with_capacity(84);
    p.extend_from_slice(&MSG_PAD_DATA.to_le_bytes());
    p.extend_from_slice(&pad_info(slot, false, 0));
    p.push(0); // connected = no
    p.extend_from_slice(&counter.to_le_bytes());
    p.extend_from_slice(&[0u8; 4]); // botones 1 y 2, PS, Touch
    p.extend_from_slice(&[centro, centro, rx, centro]); // LX LY RX RY
    p.extend_from_slice(&[0u8; 12]); // cruceta analógica, caras, gatillos
    p.extend_from_slice(&[0u8; 12]); // los dos toques del touchpad
    p.extend_from_slice(&[0u8; 8]); // timestamp del sensor
    p.extend_from_slice(&[0u8; 24]); // accel y gyro
    let out = finish(p);
    debug_assert_eq!(out.len(), 100);
    out
}

/// Slots de una máscara (bit i = slot i), en orden.
fn slots_of(mask: u8) -> impl Iterator<Item = u8> {
    (0..MAX_PLAYERS as u8).filter(move |s| mask & (1 << s) != 0)
}

/// Respuesta a MotorsInfo (0x110001): la info del slot + motores.
fn motors_packet(slot: u8, samples: &[Option<(MotionSample, Instant)>; MAX_PLAYERS]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(&MSG_MOTORS.to_le_bytes());
    out.extend_from_slice(&slot_info(slot, samples));
    out.push(MOTOR_COUNT);
    finish(out)
}

/// Orden Rumble (0x110002): tras el tipo, la cabecera de mando de 8 bytes
/// (flags, pad_id, mac) como en PadData, el motor y la intensidad 0..255.
/// Devuelve (máscara de slots, intensidad); el motor se ignora (solo hay uno).
fn parse_rumble(payload: &[u8]) -> Option<(u8, u8)> {
    let intensity = *payload.get(9)?;
    let mask = subscribed_slots(payload);
    (mask != 0).then_some((mask, intensity))
}

/// Los 11 bytes de info de mando para PortInfo, con el estado real del slot.
fn slot_info(slot: u8, samples: &[Option<(MotionSample, Instant)>; MAX_PLAYERS]) -> [u8; 11] {
    let fresh = (slot as usize) < MAX_PLAYERS
        && matches!(&samples[slot as usize], Some((_, t)) if t.elapsed() < PAD_TTL);
    let battery = if fresh {
        samples[slot as usize]
            .as_ref()
            .map(|(s, _)| mapping::battery_to_dsu(s.battery_pct))
            .unwrap_or(0)
    } else {
        0
    };
    pad_info(slot, fresh, battery)
}

/// Los 11 bytes de info de mando (compartidos por PortInfo y PadData).
fn pad_info(slot: u8, connected: bool, battery: u8) -> [u8; 11] {
    let mut out = [0u8; 11];
    out[0] = slot;
    if connected {
        out[1] = 2; // conectado
        out[2] = 2; // gyro completo
        out[3] = 2; // "bluetooth"
        out[4..10].copy_from_slice(&mac(slot));
        out[10] = battery;
    }
    out
}

/// Header DSUS + CRC sobre el paquete completo.
fn finish(payload: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + payload.len());
    out.extend_from_slice(b"DSUS");
    out.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // CRC a cero
    out.extend_from_slice(&SERVER_ID.to_le_bytes());
    out.extend_from_slice(&payload);
    let crc = crc32fast::hash(&out);
    out[8..12].copy_from_slice(&crc.to_le_bytes());
    out
}

/// Valida header DSUC y devuelve (tipo, payload tras el tipo).
fn parse_request(buf: &[u8]) -> Option<(u32, &[u8])> {
    if buf.len() < 20 || &buf[0..4] != b"DSUC" {
        return None;
    }
    let version = u16::from_le_bytes(buf[4..6].try_into().ok()?);
    if version != PROTOCOL_VERSION {
        return None;
    }
    let msg_type = u32::from_le_bytes(buf[16..20].try_into().ok()?);
    Some((msg_type, &buf[20..]))
}

/// PadData de 100 bytes (spec en protocol/DSU.md), para el slot dado.
pub fn pad_data_packet(
    slot: u8,
    sample: &MotionSample,
    touch_pressed: bool,
    counter: u32,
) -> Vec<u8> {
    let (accel, mut gyro) = mapping::to_dsu(sample.accel_ms2, sample.gyro_rads);
    // La matriz de signos vive en `mapping`; aquí solo la escala del perfil
    // (Eden espera vueltas/s × 312)
    let k = sample.profile.gyro_scale();
    for g in gyro.iter_mut() {
        *g *= k;
    }
    let b = match sample.profile {
        DsuProfile::Wii => mapping::buttons_to_dsu(sample.buttons),
        DsuProfile::WiiU => mapping::buttons_to_dsu_wiiu(sample.buttons),
        DsuProfile::Switch => mapping::buttons_to_dsu_switch(sample.buttons),
    };
    // Botón Touch: en Wii es el pulso de recentrado (IMUIR/Recenter);
    // en Wii U es Home (Cemu lo lee como botón 16 e ignora el PS); en Switch
    // es Capturar (Eden: TouchHardPress). El pulso nunca llega a Cemu ni a Eden.
    let touch_btn = match sample.profile {
        DsuProfile::Wii => {
            if touch_pressed {
                0xFF
            } else {
                0
            }
        }
        DsuProfile::WiiU | DsuProfile::Switch => b.touch,
    };

    // Sticks LX LY RX RY (0-255, neutro 128; Y: 255 = arriba, "Left Y+" en
    // Dolphin y AxisY+ en Cemu; Eden lee (v − 127) / 127, neutro 127):
    // izquierdo = Nunchuk / stick izquierdo del GamePad, derecho = stick
    // derecho del GamePad (neutro si no hay)
    let center = sample.profile.stick_center();
    let axis = |v: i8| (center + v as i32).clamp(0, 255) as u8;
    let mut b1 = b.b1;
    let mut sticks = [axis(sample.stick_x), axis(sample.stick_y), axis(sample.stick_rx), axis(sample.stick_ry)];
    let mut shoulders = b.shoulders;
    // Perfil Wii: el puntero IR que genera el receptor viaja en los bytes que
    // ese perfil deja libres (protocol/DSU.md, «Puntero IR»): X del punto
    // medio en Right X (mitad alta, 7 bits) + L2 (8 bits), Y en Right Y + R2,
    // nivel de distancia en los bits L3/R3 y roll en Left X. Sin puntero
    // (fuera de pantalla, sin quaternion) Right X = 0 es el centinela con el
    // que el perfil apaga los dos puntos.
    if sample.profile == DsuProfile::Wii {
        match sample.wii_ir {
            Some(ir) => {
                sticks[2] = 128 + (ir.x >> 8).min(127) as u8;
                sticks[3] = 128 + (ir.y >> 8).min(127) as u8;
                shoulders[3] = (ir.x & 0xFF) as u8; // L2
                shoulders[2] = (ir.y & 0xFF) as u8; // R2
                b1 |= (ir.level & 0b11) << 1; // L3 = bit 1, R3 = bit 2
                if let Some(r) = ir.roll {
                    sticks[0] = r;
                }
            }
            None => sticks[2] = 0,
        }
    }

    let mut p = Vec::with_capacity(84);
    p.extend_from_slice(&MSG_PAD_DATA.to_le_bytes());
    p.extend_from_slice(&pad_info(
        slot,
        true,
        mapping::battery_to_dsu(sample.battery_pct),
    ));
    p.push(1); // connected
    p.extend_from_slice(&counter.to_le_bytes());
    p.push(b1);
    p.push(b.b2);
    p.push(b.ps);
    p.push(touch_btn);
    p.extend_from_slice(&sticks);
    p.extend_from_slice(&b.dpad); // analógico L D R U ("Pad W/S/E/N")
    p.extend_from_slice(&b.face); // analógico square cross circle triangle
    p.extend_from_slice(&shoulders); // analógico R1 L1 R2 L2 (Wii U: ZR/ZL en R2/L2)
    // Touch 1: activo, id, x u16, y u16 (Cemu: pantalla táctil / puntero IR)
    match sample.touch {
        Some((x, y)) => {
            p.extend_from_slice(&[1, 0]);
            p.extend_from_slice(&x.to_le_bytes());
            p.extend_from_slice(&y.to_le_bytes());
        }
        None => p.extend_from_slice(&[0u8; 6]),
    }
    p.extend_from_slice(&[0u8; 6]); // touch 2 inactivo
    p.extend_from_slice(&sample.t_us.to_le_bytes()); // timestamp del SENSOR
    for v in accel {
        p.extend_from_slice(&v.to_le_bytes());
    }
    for v in gyro {
        p.extend_from_slice(&v.to_le_bytes());
    }

    let out = finish(p);
    debug_assert_eq!(out.len(), 100);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsu::WiiIr;

    fn sample() -> MotionSample {
        MotionSample {
            t_us: 123_456_789,
            accel_ms2: [0.0, 0.0, 9.80665],
            gyro_rads: [1.0, 0.0, 0.0],
            buttons: 1, // A
            battery_pct: 100,
            recenter_count: 0,
            stick_x: 0,
            stick_y: 0,
            stick_rx: 0,
            stick_ry: 0,
            touch: None,
            wii_ir: None,
            profile: DsuProfile::Wii,
        }
    }

    #[test]
    fn pad_data_wii_u_gamepad() {
        use pmp::*;
        let mut s = sample();
        s.profile = DsuProfile::WiiU;
        s.buttons = BTN_HOME | BTN_ZL | BTN_ZR | BTN_L | BTN_R | BTN_X;
        s.stick_x = 100;
        s.stick_y = -50;
        s.stick_rx = -30;
        s.stick_ry = 120;
        s.touch = Some((960, 471));
        let out = pad_data_packet(0, &s, true, 1);
        assert_eq!(out[38], 0xFF, "Home también en PS (a Dolphin no le estorba)");
        assert_eq!(out[39], 0xFF, "Home → Touch aunque no haya pulso de recentrado");
        assert_eq!(&out[40..44], &[228, 78, 98, 248], "LX LY RX RY");
        assert_eq!(out[48], 0xFF, "X → Square analógico");
        assert_eq!(&out[52..56], &[0xFF, 0xFF, 0xFF, 0xFF], "R1 L1 R2(ZR) L2(ZL)");
        assert_eq!(out[37] & 0b11, 0, "ZL/ZR no tocan los bits L2/R2 (micro y pantalla)");
        assert_eq!(out[37] & 0b1100, 0b1100, "L/R → L1/R1");
        assert_eq!(&out[56..62], &[1, 0, 0xC0, 0x03, 0xD7, 0x01], "touch1 activo en (960, 471)");
        assert_eq!(&out[62..68], &[0u8; 6], "touch2 inactivo");
        // Sin dedo: touch1 inactivo; sin Home: Touch a 0 aunque haya pulso
        s.touch = None;
        s.buttons = 0;
        let out = pad_data_packet(0, &s, true, 2);
        assert_eq!(out[39], 0);
        assert_eq!(&out[56..62], &[0u8; 6]);
    }

    #[test]
    fn pad_data_switch() {
        use pmp::*;
        let mut s = sample();
        s.profile = DsuProfile::Switch;
        s.buttons = BTN_A | BTN_HOME | BTN_SCREEN | BTN_ZL;
        s.stick_x = 100;
        s.stick_y = -50;
        s.stick_rx = -30;
        s.stick_ry = 120;
        // con pulso de recentrado: a Eden no le llega (el Touch es Capturar)
        let out = pad_data_packet(0, &s, true, 1);
        assert_eq!(out[37] & (1 << 5), 1 << 5, "A → Circle");
        assert_eq!(out[38], 0xFF, "Home → PS");
        assert_eq!(out[39], 0xFF, "Capturar → Touch");
        assert_eq!(&out[40..44], &[227, 77, 97, 247], "sticks centrados en 127");
        assert_eq!(out[55], 0xFF, "ZL → l2 analógico");
        assert_eq!(out[37] & 1, 1, "y el bit L2");
        assert_eq!(&out[56..62], &[0u8; 6], "sin táctil en Switch");
        // gyro: 1 rad/s de pitch = 57,296 °/s × 312/360 = 49,656 en el cable
        let pitch = f32::from_le_bytes(out[88..92].try_into().unwrap());
        assert!((pitch - 49.6563).abs() < 1e-3, "pitch = {pitch}");
        // accel plano: DSU y = −1 g (Eden lo lee como {x, −z, y} = {0, 0, −1})
        let ay = f32::from_le_bytes(out[80..84].try_into().unwrap());
        assert!((ay + 1.0).abs() < 1e-6, "ay = {ay}");
        // sin Capturar ni Home, con pulso: nada
        s.buttons = 0;
        let out = pad_data_packet(0, &s, true, 2);
        assert_eq!((out[38], out[39]), (0, 0));
        // sticks en reposo: exactamente 127 (Eden → 0,0)
        s.stick_x = 0;
        s.stick_y = 0;
        s.stick_rx = 0;
        s.stick_ry = 0;
        let out = pad_data_packet(0, &s, false, 3);
        assert_eq!(&out[40..44], &[127; 4]);
        // extremos: 127 ± 127 = 0 y 254 (nunca 255)
        s.stick_x = -127;
        s.stick_y = 127;
        let out = pad_data_packet(0, &s, false, 4);
        assert_eq!(&out[40..42], &[0, 254]);
        // en los perfiles Wii/Wii U el gyro sigue en °/s y el centro en 128
        let mut w = sample();
        w.profile = DsuProfile::WiiU;
        let out = pad_data_packet(0, &w, false, 5);
        let pitch = f32::from_le_bytes(out[88..92].try_into().unwrap());
        assert!((pitch - 57.29578).abs() < 1e-3);
        assert_eq!(&out[40..44], &[128; 4]);
    }

    #[test]
    fn pad_data_stick_del_nunchuk() {
        let mut s = sample();
        s.buttons = pmp::BTN_C | pmp::BTN_Z;
        s.stick_x = 100;
        s.stick_y = -50;
        let out = pad_data_packet(3, &s, false, 1);
        assert_eq!(out[20], 3); // slot del Nunchuk
        assert_eq!(&out[40..44], &[228, 78, 0, 128], "LX/LY = 128 + stick; Right X = 0 (sin puntero IR), RY neutro");
        assert_eq!(out[53], 0xFF, "C → L1 analógico");
        assert_eq!(out[52], 0xFF, "Z → R1 analógico");
        assert_eq!(&out[48..52], &[0u8; 4], "A/B intactos");
        assert_eq!(out[37] & (3 << 2), 3 << 2, "C/Z también en el bitmask (L1/R1)");
        // extremos recortados a 0..255
        s.stick_x = -127;
        s.stick_y = 127;
        let out = pad_data_packet(3, &s, false, 2);
        assert_eq!(&out[40..42], &[1, 255]);
    }

    #[test]
    fn pad_data_puntero_ir_del_perfil_wii() {
        let mut s = sample();
        s.wii_ir = Some(WiiIr { x: 0x2ABC, y: 0x1234, level: 3, roll: Some(200) });
        let out = pad_data_packet(0, &s, false, 1);
        assert_eq!(&out[40..44], &[200, 128, 128 + 0x2A, 128 + 0x12], "LX = roll, RX/RY = 128 + byte alto");
        assert_eq!((out[55], out[54]), (0xBC, 0x34), "L2/R2 = byte bajo de X/Y");
        assert_eq!(out[36] & 0b110, 0b110, "nivel 3 = L3 + R3");
        // nivel 1: solo L3; sin roll, Left X sigue siendo el stick (neutro)
        s.wii_ir = Some(WiiIr { x: 0, y: 32767, level: 1, roll: None });
        let out = pad_data_packet(0, &s, false, 2);
        assert_eq!(&out[40..44], &[128, 128, 128, 255]);
        assert_eq!((out[55], out[54]), (0, 0xFF));
        assert_eq!(out[36] & 0b110, 0b010);
        // sin puntero: centinela Right X = 0 y lo demás neutro
        s.wii_ir = None;
        let out = pad_data_packet(0, &s, false, 3);
        assert_eq!(&out[40..44], &[128, 128, 0, 128]);
        assert_eq!((out[55], out[54]), (0, 0));
        assert_eq!(out[36] & 0b110, 0);
        // en Wii U el puntero IR va por el táctil: estos bytes no se tocan
        s.profile = DsuProfile::WiiU;
        s.wii_ir = Some(WiiIr { x: 0x2ABC, y: 0x1234, level: 3, roll: Some(200) });
        let out = pad_data_packet(0, &s, false, 4);
        assert_eq!(&out[40..44], &[128; 4]);
        assert_eq!((out[55], out[54]), (0, 0));
        assert_eq!(out[36] & 0b110, 0);
    }

    /// El perfil reconstruye X con `Right X+`·32512 + `L2`·255 sobre 32767
    /// (Dolphin: `Right X+` = (byte − 128)/127 con los negativos a 0, `L2` =
    /// byte/255): tiene que salir la misma X con error muy por debajo de un
    /// píxel de cámara (1023 × 767).
    #[test]
    fn el_puntero_ir_se_reconstruye_en_dolphin_sin_perder_pixeles() {
        for x in [0u16, 1, 255, 256, 12345, 32511, 32512, 32766, 32767] {
            let y = 32767 - x;
            let mut s = sample();
            s.wii_ir = Some(WiiIr { x, y, level: 0, roll: None });
            let out = pad_data_packet(0, &s, false, 1);
            let plus = |byte: u8| ((byte as f64 - 128.0) / 127.0).max(0.0);
            let mx = (plus(out[42]) * 32512.0 + out[55] as f64 / 255.0 * 255.0) / 32767.0;
            let my = (plus(out[43]) * 32512.0 + out[54] as f64 / 255.0 * 255.0) / 32767.0;
            assert!((mx - x as f64 / 32767.0).abs() * 1023.0 < 0.01, "x={x} mx={mx}");
            assert!((my - y as f64 / 32767.0).abs() * 767.0 < 0.01, "y={y} my={my}");
            assert!(out[42] != 0, "un punto visible nunca manda el centinela");
        }
    }

    #[test]
    fn pad_data_mide_100_bytes_y_crc_valido() {
        let s = sample();
        let out = pad_data_packet(0, &s, false, 7);
        assert_eq!(out.len(), 100);
        assert_eq!(&out[0..4], b"DSUS");
        assert_eq!(u16::from_le_bytes(out[4..6].try_into().unwrap()), 1001);
        assert_eq!(u16::from_le_bytes(out[6..8].try_into().unwrap()), 84);
        let mut copy = out.clone();
        let crc_in = u32::from_le_bytes(copy[8..12].try_into().unwrap());
        copy[8..12].copy_from_slice(&[0; 4]);
        assert_eq!(crc32fast::hash(&copy), crc_in);
    }

    #[test]
    fn pad_data_campos_clave() {
        let s = sample();
        let out = pad_data_packet(0, &s, true, 42);
        assert_eq!(u32::from_le_bytes(out[16..20].try_into().unwrap()), MSG_PAD_DATA);
        assert_eq!(out[20], 0); // slot
        assert_eq!(out[21], 2); // conectado
        assert_eq!(out[22], 2); // gyro completo
        assert_eq!(out[31], 1);
        assert_eq!(u32::from_le_bytes(out[32..36].try_into().unwrap()), 42);
        assert_eq!(out[37] & (1 << 6), 1 << 6); // A → Cross (bitmask)
        assert_eq!(out[39], 0xFF); // pulso de recentrado en Touch
        assert_eq!(&out[40..44], &[128, 128, 0, 128], "sin puntero IR: Right X = 0 (centinela)");
        assert_eq!(out[49], 0xFF); // Cross analógico
        assert_eq!(out[48], 0);
        assert_eq!(
            u64::from_le_bytes(out[68..76].try_into().unwrap()),
            123_456_789
        );
        let ay = f32::from_le_bytes(out[80..84].try_into().unwrap());
        assert!((ay + 1.0).abs() < 1e-5, "ay={ay}");
        let pitch = f32::from_le_bytes(out[88..92].try_into().unwrap());
        assert!((pitch - 57.29578).abs() < 1e-3, "pitch={pitch}");
    }

    /// El latido de un slot sin móvil: mismo tamaño y mismo CRC que un
    /// PadData normal (si no, el emulador lo tira), pero diciendo que ahí no
    /// hay mando. Sin él, Cemu deja de pedir ese pad para siempre.
    #[test]
    fn el_latido_es_un_paddata_valido_que_dice_desconectado() {
        let out = pad_absent_packet(2, 9, DsuProfile::WiiU);
        assert_eq!(out.len(), 100);
        assert_eq!(u16::from_le_bytes(out[6..8].try_into().unwrap()), 84);
        let mut copy = out.clone();
        let crc_in = u32::from_le_bytes(copy[8..12].try_into().unwrap());
        copy[8..12].copy_from_slice(&[0; 4]);
        assert_eq!(crc32fast::hash(&copy), crc_in, "CRC bueno");
        assert_eq!(u32::from_le_bytes(out[16..20].try_into().unwrap()), MSG_PAD_DATA);
        assert_eq!(out[20], 2, "el slot que toca");
        assert_eq!(out[21], 0, "estado: desconectado");
        assert_eq!(out[31], 0, "connected = no");
        assert_eq!(u32::from_le_bytes(out[32..36].try_into().unwrap()), 9, "contador");
        assert!(out[36..40].iter().all(|b| *b == 0), "sin botones");
        assert_eq!(&out[40..44], &[128, 128, 128, 128], "sticks al NEUTRO, no a cero");
        assert!(out[44..].iter().all(|b| *b == 0), "sin gatillos, táctil ni movimiento");
        // Perfil Wii: Right X = 0 es el centinela de «puntero fuera de cámara»
        let wii = pad_absent_packet(0, 1, DsuProfile::Wii);
        assert_eq!(&wii[40..44], &[128, 128, 0, 128], "el centinela del puntero IR se respeta");
        // Eden lee (v − 127) / 127: con 127 el reposo es exacto
        let switch = pad_absent_packet(0, 1, DsuProfile::Switch);
        assert_eq!(&switch[40..44], &[127, 127, 127, 127]);
    }

    #[test]
    fn slots_distintos_llevan_mac_y_slot_distintos() {
        let s = sample();
        let p0 = pad_data_packet(0, &s, false, 1);
        let p1 = pad_data_packet(1, &s, false, 2);
        assert_eq!(p0[20], 0);
        assert_eq!(p1[20], 1);
        // MAC en los bytes 24..30 del paquete (pad_info offset 4..10 + header 20)
        assert_eq!(&p0[24..30], &[0x50, 0x4D, 0x50, 0x31, 0x00, 0x00]);
        assert_eq!(&p1[24..30], &[0x50, 0x4D, 0x50, 0x31, 0x00, 0x01]);
    }

    #[test]
    fn portinfo_por_slot_vivo_y_muerto() {
        let s = sample();
        let mut samples: [Option<(MotionSample, Instant)>; MAX_PLAYERS] = [None; MAX_PLAYERS];
        samples[1] = Some((s, Instant::now()));
        let dead = slot_info(0, &samples);
        let live = slot_info(1, &samples);
        assert_eq!(dead[1], 0, "slot 0 sin muestras = desconectado");
        assert_eq!(live[1], 2, "slot 1 con muestra fresca = conectado");
        assert_eq!(live[0], 1);
        assert_eq!(&live[4..10], &[0x50, 0x4D, 0x50, 0x31, 0x00, 0x01]);
    }

    #[test]
    fn las_peticiones_por_slot_de_un_mismo_cliente_se_acumulan() {
        // Cemu: un socket, un pad por petición, y en cada respuesta vuelve a
        // pedir ESE pad. Sustituir la suscripción dejaba al otro mando sin
        // datos (dos Mandos Wii en Cemu: uno «desconectado»).
        let t0 = Instant::now();
        let mut c = Client::new();
        c.register(subscribed_slots(&[1, 0, 0, 0, 0, 0, 0, 0]), t0);
        c.register(subscribed_slots(&[1, 1, 0, 0, 0, 0, 0, 0]), t0);
        assert!(c.wants(0, CLIENT_TTL, t0) && c.wants(1, CLIENT_TTL, t0), "los dos pads pedidos siguen los dos");
        assert!(!c.wants(2, CLIENT_TTL, t0) && !c.wants(9, CLIENT_TTL, t0));
        // cada slot caduca por su cuenta: el 1 se sigue pidiendo, el 0 no
        let t1 = t0 + Duration::from_secs(2);
        c.register(0b0010, t1);
        let t2 = t0 + Duration::from_millis(3500);
        assert!(!c.wants(0, CLIENT_TTL, t2), "el slot 0 caducó a los 3 s");
        assert!(c.wants(1, CLIENT_TTL, t2) && c.alive(CLIENT_TTL, t2));
        // «todos» (flags 0, Dolphin) cuenta para todos los slots
        c.register(subscribed_slots(&[0]), t2);
        assert!((0..MAX_PLAYERS).all(|s| c.wants(s, CLIENT_TTL, t2)));
        // sin peticiones en 3 s, el cliente se da por ido
        let t3 = t2 + Duration::from_secs(4);
        assert!(!c.alive(CLIENT_TTL, t3));
    }

    #[test]
    fn suscripcion_por_slot_como_dolphin() {
        // flags 0: todos los pads
        assert_eq!(subscribed_slots(&[0, 0, 0, 0, 0, 0, 0, 0]), 0b1111);
        // Dolphin: flags 1 (PadID) + índice del mando
        assert_eq!(subscribed_slots(&[1, 0, 0, 0, 0, 0, 0, 0]), 0b0001);
        assert_eq!(subscribed_slots(&[1, 2, 0, 0, 0, 0, 0, 0]), 0b0100);
        assert_eq!(subscribed_slots(&[1, 7, 0, 0, 0, 0, 0, 0]), 0, "pad fuera de rango");
        // por MAC (la nuestra) y combinado
        assert_eq!(subscribed_slots(&[2, 0, 0x50, 0x4D, 0x50, 0x31, 0x00, 0x01]), 0b0010);
        assert_eq!(subscribed_slots(&[3, 0, 0x50, 0x4D, 0x50, 0x31, 0x00, 0x03]), 0b1001);
        assert_eq!(subscribed_slots(&[2, 0, 1, 2, 3, 4, 5, 6]), 0, "MAC ajena");
        // petición truncada: nada
        assert_eq!(subscribed_slots(&[1]), 0);
    }

    #[test]
    fn version_y_portinfo_bien_formados() {
        let mut v = Vec::new();
        v.extend_from_slice(&MSG_VERSION.to_le_bytes());
        v.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        let out = finish(v);
        assert_eq!(out.len(), 22);

        let mut p = Vec::new();
        p.extend_from_slice(&MSG_PORT_INFO.to_le_bytes());
        p.extend_from_slice(&pad_info(0, true, 5));
        p.push(0);
        let out = finish(p);
        assert_eq!(out.len(), 32);
    }
}
