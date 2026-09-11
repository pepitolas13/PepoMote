//! Conversión de unidades y MATRIZ DE SIGNOS móvil→DSU. Este es el ÚNICO
//! sitio donde se tocan ejes/signos si la calibración de h3 (6 poses + 3
//! rotaciones contra las barras vivas de Dolphin) revela alguno invertido.
//!
//! En el cable DSU: accel en g, gyro en °/s (Dolphin convierte internamente;
//! ver protocol/DSU.md). Android entrega m/s² y rad/s.
//!
//! Postura de referencia: móvil en mano como mando, pantalla arriba, borde
//! superior apuntando a la TV. Ejes Android: X = derecha, Y = hacia la TV,
//! Z = perpendicular a la pantalla (arriba). En modo Wii U el móvil apaisado
//! ya viene remapeado a ese mismo marco (un DS4 tumbado), así que la matriz
//! es la misma.

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

/// Botones de un PadData (bytes 36-39 y los analógicos 44-55).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DsuButtons {
    /// byte 36: bit0 Share, bit1 L3, bit2 R3, bit3 Options, bit4 Up, bit5 Right, bit6 Down, bit7 Left
    pub b1: u8,
    /// byte 37: bit0 L2, bit1 R2, bit2 L1, bit3 R1, bit4 Triangle, bit5 Circle, bit6 Cross, bit7 Square
    pub b2: u8,
    /// byte 38: botón PS/Home
    pub ps: u8,
    /// byte 39: botón Touch (en el perfil Wii lo pone el pulso de recentrado)
    pub touch: u8,
    /// bytes 44-47: cruceta analógica Left, Down, Right, Up
    pub dpad: [u8; 4],
    /// bytes 48-51: caras analógicas square, cross, circle, triangle
    pub face: [u8; 4],
    /// bytes 52-55: R1, L1, R2, L2 analógicos
    pub shoulders: [u8; 4],
}

fn on(c: bool) -> u8 {
    if c {
        0xFF
    } else {
        0
    }
}

/// Bits PMP (PROTOCOL.md §4.2) → botones DSU, perfil Wii (Dolphin).
///
/// Mapeo: A→Cross, B→Circle, 1→Square, 2→Triangle, +→Options, −→Share, Home→PS.
/// Nunchuk (bits 17/18): C→Cross, Z→Circle (mismos bytes que A/B: cada
/// Nunchuk va en su propio pad DSU y Dolphin lee de ahí sus C/Z).
pub fn buttons_to_dsu(pmp: u32) -> DsuButtons {
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
    if bit(1) || bit(18) {
        b2 |= 1 << 5; // B (o Z del Nunchuk) → Circle
    }
    if bit(0) || bit(17) {
        b2 |= 1 << 6; // A (o C del Nunchuk) → Cross
    }
    if bit(9) {
        b2 |= 1 << 7; // Uno → Square
    }

    DsuButtons {
        b1,
        b2,
        ps: on(bit(8)), // Home → PS
        touch: 0,
        // dpad analógico en el orden del struct de Dolphin: Left, Down, Right, Up
        // (entradas "Pad W", "Pad S", "Pad E", "Pad N")
        dpad: [on(bit(4)), on(bit(3)), on(bit(5)), on(bit(2))],
        // caras analógicas en el orden del struct de Dolphin (PadDataResponse):
        // square, cross, circle, triangle — Dolphin lee los botones de cara de
        // AQUÍ (los bits de button_states2 los ignora)
        face: [on(bit(9)), on(bit(0) || bit(17)), on(bit(1) || bit(18)), on(bit(10))],
        shoulders: [0; 4],
    }
}

/// Bits PMP → botones DSU, perfil Wii U (Cemu). Cemu lee los bits de los
/// bytes 36-37 (botón i = bit i, y 8+i), el Touch (byte 39) como botón 16 y
/// los gatillos analógicos l2/r2 (bytes 55/54) como ejes; el PS (38) lo
/// ignora. Con 17 botones digitales para 19 del GamePad, ZL/ZR van por los
/// gatillos analógicos y así L2/R2 quedan para soplar al micro y TV↔Pad.
///
/// A→Cross · B→Circle · X/1→Square · Y/2→Triangle · L→L1 · R→R1 ·
/// ZL/ZR→l2/r2 analógicos · Mic→L2 · Pantalla→R2 · click sticks→L3/R3 ·
/// +→Options · −→Share · Home→Touch (y PS) · cruceta igual · C→Cross · Z→Circle.
pub fn buttons_to_dsu_wiiu(pmp: u32) -> DsuButtons {
    let bit = |b: u32| pmp & (1 << b) != 0;
    let mut d = buttons_to_dsu(pmp);
    // X / Y del GamePad comparten destino con 1 / 2 del Mando Wii
    if bit(19) {
        d.b2 |= 1 << 7; // X → Square
        d.face[0] = 0xFF;
    }
    if bit(20) {
        d.b2 |= 1 << 4; // Y → Triangle
        d.face[3] = 0xFF;
    }
    if bit(21) {
        d.b2 |= 1 << 2; // L → L1
    }
    if bit(22) {
        d.b2 |= 1 << 3; // R → R1
    }
    if bit(27) {
        d.b2 |= 1 << 0; // Mic → L2 (bit; el analógico l2 es ZL)
    }
    if bit(28) {
        d.b2 |= 1 << 1; // Pantalla TV↔Pad → R2 (bit; el analógico r2 es ZR)
    }
    if bit(25) {
        d.b1 |= 1 << 1; // click stick izquierdo → L3
    }
    if bit(26) {
        d.b1 |= 1 << 2; // click stick derecho → R3
    }
    // R1, L1, R2, L2 analógicos: R2/L2 son los gatillos ZR/ZL
    d.shoulders = [on(bit(22)), on(bit(21)), on(bit(24)), on(bit(23))];
    // Home → Touch (Cemu no lee el PS)
    d.touch = on(bit(8));
    d
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
        let d = buttons_to_dsu(pmp);
        assert_eq!(d.b1, (1 << 3) | (1 << 4)); // Options + Up
        assert_eq!(d.b2, 1 << 6); // Cross
        assert_eq!(d.ps, 0xFF);
        assert_eq!(d.touch, 0, "en el perfil Wii el Touch es el pulso de recentrado");
        assert_eq!(d.dpad, [0, 0, 0, 0xFF]); // L D R U → solo Up
        assert_eq!(d.face, [0, 0xFF, 0, 0]); // square CROSS circle triangle → A
        assert_eq!(d.shoulders, [0; 4]);
    }

    #[test]
    fn botones_wii_u() {
        use pmp::*;
        // Todo el GamePad pulsado a la vez
        let pmp = BTN_A | BTN_B | BTN_X | BTN_Y | BTN_L | BTN_R | BTN_ZL | BTN_ZR
            | BTN_STICK_L | BTN_STICK_R | BTN_MIC | BTN_SCREEN | BTN_PLUS | BTN_MINUS | BTN_HOME
            | BTN_DPAD_UP | BTN_DPAD_DOWN | BTN_DPAD_LEFT | BTN_DPAD_RIGHT;
        let d = buttons_to_dsu_wiiu(pmp);
        assert_eq!(d.b1, 0xFF, "Share L3 R3 Options Up Right Down Left");
        assert_eq!(d.b2, 0xFF, "L2 R2 L1 R1 Triangle Circle Cross Square");
        assert_eq!(d.touch, 0xFF, "Home → Touch (botón 16 de Cemu)");
        assert_eq!(d.ps, 0xFF);
        assert_eq!(d.face, [0xFF; 4]);
        assert_eq!(d.dpad, [0xFF; 4]);
        assert_eq!(d.shoulders, [0xFF; 4], "R1 L1 R2 L2 analógicos");

        // Solo ZL: gatillo analógico l2 (byte 55) SIN el bit L2 (que es el micro)
        let d = buttons_to_dsu_wiiu(BTN_ZL);
        assert_eq!(d.b2, 0);
        assert_eq!(d.shoulders, [0, 0, 0, 0xFF]);
        // Solo ZR: r2 analógico (byte 54)
        let d = buttons_to_dsu_wiiu(BTN_ZR);
        assert_eq!(d.b2, 0);
        assert_eq!(d.shoulders, [0, 0, 0xFF, 0]);
        // Mic → bit L2, Pantalla → bit R2, sin analógicos
        let d = buttons_to_dsu_wiiu(BTN_MIC | BTN_SCREEN);
        assert_eq!(d.b2, 0b11);
        assert_eq!(d.shoulders, [0; 4]);
        // X/Y = Square/Triangle como 1/2; A/B = Cross/Circle
        let d = buttons_to_dsu_wiiu(BTN_X | BTN_Y);
        assert_eq!(d.b2, (1 << 7) | (1 << 4));
        assert_eq!(d.face, [0xFF, 0, 0, 0xFF]);
        let d = buttons_to_dsu_wiiu(BTN_A | BTN_B);
        assert_eq!(d.b2, (1 << 6) | (1 << 5));
        assert_eq!(d.face, [0, 0xFF, 0xFF, 0]);
        // clicks de stick
        let d = buttons_to_dsu_wiiu(BTN_STICK_L | BTN_STICK_R);
        assert_eq!(d.b1, 0b110);
        // C/Z del Nunchuk como en Wii
        let d = buttons_to_dsu_wiiu(BTN_C | BTN_Z);
        assert_eq!(d.b2, (1 << 6) | (1 << 5));
        // Mando Wii dentro de Cemu: 1/2 y Home
        let d = buttons_to_dsu_wiiu(BTN_ONE | BTN_TWO | BTN_HOME);
        assert_eq!(d.b2, (1 << 7) | (1 << 4));
        assert_eq!(d.touch, 0xFF);
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
