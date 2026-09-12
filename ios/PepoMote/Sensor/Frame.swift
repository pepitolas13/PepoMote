import Foundation

/// Remapeo de los sensores para el GamePad (móvil apaisado). El receptor y
/// Cemu esperan el marco de un DS4 tumbado: X = derecha del mando, Y = hacia
/// el borde superior del mando (lejos del jugador), Z = saliendo de la
/// pantalla. En vertical (Wiimote) coincide con los ejes del móvil; apaisado
/// hay que girar los vectores ANTES de escribir el paquete, según hacia dónde
/// quede el borde superior del móvil. Puro: testeable (FrameTests, los mismos
/// casos que Android).
///
/// Las rotaciones son las de `android.view.Surface` (mismos valores):
/// ROTATION_90 = borde superior a la IZQUIERDA; ROTATION_270 = a la DERECHA.
enum Frame {
    static let rotation0 = 0
    static let rotation90 = 1
    static let rotation180 = 2
    static let rotation270 = 3

    private static let halfSqrt2: Float = 0.70710678

    /// Vector (accel o gyro): borde superior a la izquierda (ROTATION_90) →
    /// (−y, x, z); a la derecha (ROTATION_270) → (y, −x, z); cualquier otra
    /// rotación, tal cual.
    private static func remapVector(_ v: [Float], _ rotation: Int) -> [Float] {
        switch rotation {
        case rotation90: return [-v[1], v[0], v[2]]
        case rotation270: return [v[1], -v[0], v[2]]
        default: return [v[0], v[1], v[2]]
        }
    }

    /// Acelerómetro (m/s²) al marco del GamePad.
    static func remapAccel(_ accel: [Float], _ rotation: Int) -> [Float] { remapVector(accel, rotation) }

    /// Giroscopio (rad/s) al marco del GamePad.
    static func remapGyro(_ gyro: [Float], _ rotation: Int) -> [Float] { remapVector(gyro, rotation) }

    /// Quaternion (w, x, y, z) al marco del GamePad: `quat ⊗ r` (producto de
    /// Hamilton, r a la derecha) con r = (√½, 0, 0, −√½) si el borde superior
    /// queda a la izquierda y r = (√½, 0, 0, +√½) si queda a la derecha.
    static func remapQuat(_ q: [Float], _ rotation: Int) -> [Float] {
        let rz: Float
        switch rotation {
        case rotation90: rz = -halfSqrt2
        case rotation270: rz = halfSqrt2
        default: return [q[0], q[1], q[2], q[3]]
        }
        let rw = halfSqrt2
        let (qw, qx, qy, qz) = (q[0], q[1], q[2], q[3])
        return [
            qw * rw - qz * rz,
            qx * rw + qy * rz,
            qy * rw - qx * rz,
            qw * rz + qz * rw,
        ]
    }
}
