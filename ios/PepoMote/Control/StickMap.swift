import Foundation

/// Del desplazamiento del pulgar sobre el stick virtual (pt, y hacia ABAJO como
/// en pantalla) al stick del Nunchuk (−127..127, +y ARRIBA). Zona muerta
/// pequeña en el centro, reescalada para que el borde de la zona muerta sea el
/// 0 y el radio el tope: sin escalón al salir del centro. Puro: testeable.
enum StickMap {
    /// Fracción del radio que se ignora en el centro.
    static let deadZone: Float = 0.08

    /// (x, y) del stick; (0, 0) dentro de la zona muerta.
    static func map(_ dx: Float, _ dy: Float, radius: Float) -> (x: Int, y: Int) {
        let len = (dx * dx + dy * dy).squareRoot()
        if radius <= 0 || len <= 0 { return (0, 0) }
        let m = len / radius
        if m < deadZone { return (0, 0) }
        let scaled = (min(m, 1) - deadZone) / (1 - deadZone) // 0..1
        let x = clamp(roundHalfUp(dx / len * scaled * 127))
        let y = clamp(roundHalfUp(-dy / len * scaled * 127))
        return (x, y)
    }

    /// Como `roundToInt` de Kotlin: al más cercano, los empates hacia +∞.
    static func roundHalfUp(_ v: Float) -> Int {
        Int((v + 0.5).rounded(.down))
    }

    private static func clamp(_ v: Int) -> Int { min(max(v, -127), 127) }
}
