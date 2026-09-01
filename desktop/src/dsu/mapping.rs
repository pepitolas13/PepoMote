//! Conversión de unidades y MATRIZ DE SIGNOS móvil→DSU. Este es el ÚNICO
//! sitio donde se tocan ejes/signos si la calibración de h3 (6 poses + 3
//! rotaciones contra las barras vivas de Dolphin) revela alguno invertido.
//!
//! En el cable DSU: accel en g, gyro en °/s (Dolphin convierte internamente;
//! ver protocol/DSU.md). Android entrega m/s² y rad/s.
//!
//! Postura de referencia: móvil en mano como mando, pantalla arriba, borde
//! superior apuntando a la TV. Ejes Android: X = derecha, Y = hacia la TV,
//! Z = perpendicular a la pantalla (arriba).

const G: f32 = 9.80665;
const RAD_TO_DEG: f32 = 180.0 / std::f32::consts::PI;

/// (accel_g, gyro_degs) en la convención DSU:
/// accel (x, y, z) y gyro (pitch, yaw, roll).
pub fn to_dsu(accel_ms2: [f32; 3], gyro_rads: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let [ax, ay, az] = accel_ms2;
    let [gx, gy, gz] = gyro_rads;

    // Convención verificada contra el código de Dolphin (DualShockUDPClient):
    //   Accel Up = -y_dsu · Accel Right = -x_dsu · Accel Forward = +z_dsu
    //   Gyro Pitch Up = +pitch · Yaw Right = +yaw · Roll Right = +roll
    // Móvil plano: android a=(0,0,+g) → dsu (0,-1,0) = Accel Up ✓
    // Apuntando arriba θ: android a=(0, g·sinθ, g·cosθ) → dsu_z=+sinθ =
    // Accel Forward ✓ (con -ay salía invertido: el bug de h3 en el real).
    let accel = [
        -ax / G, // DSU X ← -X android
        -az / G, // DSU Y ← -Z android
        ay / G,  // DSU Z ← +Y android
    ];
    let gyro = [
        gx * RAD_TO_DEG,  // pitch ← +X android (muñeca arriba = Pitch Up)
        -gz * RAD_TO_DEG, // yaw   ← -Z android (girar a la derecha = Yaw Right)
        gy * RAD_TO_DEG,  // roll  ← +Y android (rolar a la derecha = Roll Right)
    ];
    (accel, gyro)
}

/// Bits PMP (PROTOCOL.md §4.2) → botones DSU.
/// Devuelve (buttons1, buttons2, ps, dpad_analog LDRU, face_analog YBAX).
///
/// buttons1: bit0 Share, bit3 Options, bit4 Up, bit5 Right, bit6 Down, bit7 Left
/// buttons2: bit4 Triangle, bit5 Circle, bit6 Cross, bit7 Square
/// Mapeo: A→Cross, B→Circle, 1→Square, 2→Triangle, +→Options, −→Share, Home→PS
pub fn buttons_to_dsu(pmp: u32) -> (u8, u8, u8, [u8; 4], [u8; 4]) {
    let bit = |b: u32| pmp & (1 << b) != 0;

    let mut b1 = 0u8;
    if bit(7) {
        b1 |= 1 << 0; // Minus → Share
    }
    if bit(6) {
        b1 |= 1 << 3; // Plus → Options
    }
    if bit(2) {
        b1 |= 1 << 4; // Up
    }
    if bit(5) {
        b1 |= 1 << 5; // Right
    }
    if bit(3) {
        b1 |= 1 << 6; // Down
    }
    if bit(4) {
        b1 |= 1 << 7; // Left
    }

    let mut b2 = 0u8;
    if bit(10) {
        b2 |= 1 << 4; // Dos → Triangle
    }
    if bit(1) {
        b2 |= 1 << 5; // B → Circle
    }
    if bit(0) {
        b2 |= 1 << 6; // A → Cross
    }
    if bit(9) {
        b2 |= 1 << 7; // Uno → Square
    }

    let ps = if bit(8) { 0xFF } else { 0 }; // Home → PS

    let on = |c: bool| if c { 0xFFu8 } else { 0 };
    // dpad analógico en orden DSU: Left, Down, Right, Up
    let dpad = [on(bit(4)), on(bit(3)), on(bit(5)), on(bit(2))];
    // caras analógicas en orden DSU: Y(Triangle), B(Circle), A(Cross), X(Square)
    let face = [on(bit(10)), on(bit(1)), on(bit(0)), on(bit(9))];

    (b1, b2, ps, dpad, face)
}

/// % de batería → niveles DSU.
pub fn battery_to_dsu(pct: u8) -> u8 {
    match pct {
        88..=100 => 0x05,
        63..=87 => 0x04,
        38..=62 => 0x03,
        13..=37 => 0x02,
        _ => 0x01,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accel_en_reposo_plano() {
        // Móvil plano sobre la mesa: android accel = (0, 0, +9.80665)
        let (a, g) = to_dsu([0.0, 0.0, 9.80665], [0.0; 3]);
        assert!((a[0]).abs() < 1e-6);
        assert!((a[1] + 1.0).abs() < 1e-6, "DSU Y = -1 g plano: {}", a[1]);
        assert!((a[2]).abs() < 1e-6);
        assert_eq!(g, [0.0; 3]);
    }

    #[test]
    fn gyro_a_grados() {
        let (_, g) = to_dsu([0.0; 3], [1.0, 0.5, -2.0]);
        assert!((g[0] - 57.29578).abs() < 1e-3); // pitch = +gx
        assert!((g[1] - 114.59156).abs() < 1e-3); // yaw = -gz = +2 rad/s
        assert!((g[2] - 28.64789).abs() < 1e-3); // roll = +gy
    }

    #[test]
    fn apuntar_arriba_da_accel_forward_positivo() {
        // Móvil con el morro 30° arriba: android a = (0, g·sin30, g·cos30)
        let g30 = 9.80665f32;
        let (a, _) = to_dsu([0.0, g30 * 0.5, g30 * 0.866025], [0.0; 3]);
        assert!((a[2] - 0.5).abs() < 1e-4, "dsu_z={} (Accel Forward)", a[2]);
        assert!((a[1] + 0.866025).abs() < 1e-4, "dsu_y={}", a[1]);
    }

    #[test]
    fn botones() {
        // A + Plus + dpad Up + Home
        let pmp = (1 << 0) | (1 << 6) | (1 << 2) | (1 << 8);
        let (b1, b2, ps, dpad, face) = buttons_to_dsu(pmp);
        assert_eq!(b1, (1 << 3) | (1 << 4)); // Options + Up
        assert_eq!(b2, 1 << 6); // Cross
        assert_eq!(ps, 0xFF);
        assert_eq!(dpad, [0, 0, 0, 0xFF]);
        assert_eq!(face, [0, 0, 0xFF, 0]);
    }

    #[test]
    fn bateria() {
        assert_eq!(battery_to_dsu(100), 0x05);
        assert_eq!(battery_to_dsu(70), 0x04);
        assert_eq!(battery_to_dsu(50), 0x03);
        assert_eq!(battery_to_dsu(20), 0x02);
        assert_eq!(battery_to_dsu(5), 0x01);
    }
}
