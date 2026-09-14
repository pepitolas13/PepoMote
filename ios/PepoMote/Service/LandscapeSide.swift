import UIKit

/// Hacia qué lado se gira el mando + Nunchuk (apaisado fijo). El sensor de
/// giro de algunos móviles le da la vuelta al mando (180°) con muy poco o
/// con falsos positivos, así que el lado se elige una vez y se queda:
/// `left` = el móvil girado a la izquierda (el borde superior, el de la
/// cámara, queda a la izquierda), `right` = girado a la derecha. `unset` =
/// todavía no se ha elegido: la primera vez iOS gira el mando como siempre y
/// se pregunta encima si está bien. `sensor` = que decida el sensor, como
/// antes (Ajustes). Puro: testeable (mismos casos que Android).
enum LandscapeSide: String {
    case unset = ""
    case sensor = "sensor"
    case left = "left"
    case right = "right"

    static func fromPref(_ s: String) -> LandscapeSide { LandscapeSide(rawValue: s) ?? .unset }

    /// Máscara de orientación para este lado. Ojo con UIKit: `left` (borde
    /// superior del móvil a la izquierda) es `landscapeRight` (el botón Home
    /// a la derecha), y `right` es `landscapeLeft`.
    var mask: UIInterfaceOrientationMask {
        switch self {
        case .left: return .landscapeRight
        case .right: return .landscapeLeft
        case .unset, .sensor: return .landscape
        }
    }

    /// Darle la vuelta (180°); lo que no es un lado, tal cual.
    var flipped: LandscapeSide {
        switch self {
        case .left: return .right
        case .right: return .left
        default: return self
        }
    }

    /// Lado que se está viendo, con la interfaz ya apaisada.
    static func current(_ o: UIInterfaceOrientation) -> LandscapeSide {
        o == .landscapeLeft ? .right : .left
    }

    /// El que manda: el provisional («Darle la vuelta» sin confirmar) si lo hay; si no, el guardado.
    static func effective(saved: LandscapeSide, provisional: LandscapeSide?) -> LandscapeSide {
        provisional ?? saved
    }
}
