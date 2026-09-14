import Foundation

/// Cruceta de una sola pieza (como la del Mando de Wii): el dedo se sigue
/// mientras está apoyado y la dirección sale de dónde está respecto al
/// centro, así se puede deslizar de → a ↓ sin levantarlo y las esquinas
/// entre dos brazos son diagonales. Medidas en fracción de `half` (media
/// anchura de la cruceta): los brazos miden un tercio (media anchura
/// `a = half / 3`, igual que antes) y el centro muerto es un círculo de radio
/// `dead`·a, bastante más pequeño que el cuadrado central de antes. Puro:
/// mismos casos que Android (DpadModel.kt).
enum DpadModel {
    static let up = 1
    static let down = 2
    static let left = 4
    static let right = 8

    /// Radio del centro muerto, en medias anchuras de brazo.
    static let dead: Float = 0.6

    /// Direcciones en pantalla para un dedo en (`dx`, `dy`) desde el centro
    /// (+y abajo): dentro del centro muerto, ninguna; en el cuadrado central,
    /// la del eje dominante; sobre un brazo, solo la suya (aunque el dedo se
    /// salga por la punta); en una esquina entre dos brazos, las dos.
    static func dirs(_ dx: Float, _ dy: Float, half: Float) -> Int {
        let a = half / 3
        let deadR = a * dead
        if dx * dx + dy * dy <= deadR * deadR { return 0 }
        let ax = abs(dx)
        let ay = abs(dy)
        let horizontal = dx > 0 ? right : left
        let vertical = dy > 0 ? down : up
        if ax <= a && ay <= a { return ax > ay ? horizontal : vertical }
        if ay <= a { return horizontal }
        if ax <= a { return vertical }
        return horizontal | vertical
    }

    /// Bits del mando (`Btn`) de esas direcciones; con `sideways`, los del mando girado (`Btn.sideways`).
    static func buttons(_ dirs: Int, sideways: Bool) -> UInt32 {
        var bits: UInt32 = 0
        for (dir, bit) in [(up, Btn.dpadUp), (down, Btn.dpadDown), (left, Btn.dpadLeft), (right, Btn.dpadRight)] where dirs & dir != 0 {
            bits |= sideways ? Btn.sideways(bit) : bit
        }
        return bits
    }

    /// Los cuatro bits de cruceta del mando.
    static let buttonBits: [UInt32] = [Btn.dpadUp, Btn.dpadDown, Btn.dpadLeft, Btn.dpadRight]
}
