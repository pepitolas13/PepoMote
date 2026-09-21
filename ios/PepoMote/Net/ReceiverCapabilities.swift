import Foundation

/// Capacidades del `ok`; los receptores de PC antiguos conservan sus funciones.
struct ReceiverCapabilities: Equatable {
    let platform: String
    let textInput: Bool
    let frameRotation: Bool
    /// `ok.rumble`: qué puede hacer el receptor con la vibración de los juegos
    /// ("ready", "driver", "denied", "unsupported"); se guarda tal cual y
    /// Ajustes lo traduce. nil = receptor sin vibración, que no lo manda.
    let rumble: String?

    init(ok: [String: Any] = [:]) {
        platform = (ok["platform"] as? String)?.lowercased() == "android" ? "android" : "desktop"
        textInput = platform != "android" && (ok["text_input"] as? Bool ?? true)
        frameRotation = ok["frame_rotation"] as? Bool ?? false
        rumble = ok["rumble"] as? String
    }

    var retroPads: [String] {
        platform == "android" ? [LinkState.padRetroPad, LinkState.padNes] : LinkState.retroPads
    }

    var supportsPointer: Bool { platform != "android" }

    func acceptsPad(_ pad: String) -> Bool {
        pad != LinkState.padGun || platform != "android"
    }

    /// Fallback de sesión, sin escribir la preferencia que se eligió en el PC.
    func restoredPad(_ pad: String) -> String {
        acceptsPad(pad) ? pad : LinkState.padRetroPad
    }
}
