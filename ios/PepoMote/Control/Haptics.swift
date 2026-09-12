import UIKit

/// Háptica de los botones: equivalentes de KEYBOARD_TAP, CLOCK_TICK y
/// LONG_PRESS de Android. Los generadores se preparan una vez.
enum Haptics {
    private static let tapGen = UIImpactFeedbackGenerator(style: .light)
    private static let pressGen = UIImpactFeedbackGenerator(style: .medium)
    private static let tickGen = UISelectionFeedbackGenerator()

    static func prepare() {
        tapGen.prepare()
        tickGen.prepare()
    }

    /// Toque de botón.
    static func tap() {
        tapGen.impactOccurred()
        tapGen.prepare()
    }

    /// Tic suave (stick al salir del centro, táctil del GamePad).
    static func tick() {
        tickGen.selectionChanged()
        tickGen.prepare()
    }

    /// Pulsación larga (recentrado).
    static func longPress() {
        pressGen.impactOccurred()
    }
}
