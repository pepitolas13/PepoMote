import Foundation

/// Pantalla del mando que toca enseñar.
enum PadScreen {
    case gamePad
    case wii
    case nunchuk
}

/// Routing del mando: función pura de (estado del enlace, intención pendiente),
/// testeada en RouteTests (mismos casos que Android).
enum Route {
    /// Claves de los avisos (Localizable.strings).
    static let warnNeeds13 = "warn_needs_13"
    static let warnPlayer1 = "warn_player_1"

    /// Modo Wii U activo como GamePad/Pro: el receptor confirmó `cemu` y este
    /// móvil (mando) no ha elegido ser Mando de Wii. Solo entonces se emiten
    /// paquetes de 80 bytes.
    static func isGamePad(_ link: UiLink) -> Bool {
        guard let c = link.connected else { return false }
        return c.mode == LinkState.modeCemu && c.role == LinkState.roleWiimote && c.pad != LinkState.padWiimote
    }

    /// - Nunchuk si el enlace es un Nunchuk (nunca es GamePad, pida lo que pida);
    /// - GamePad si el receptor confirmó `cemu` y este mando no es Mando de Wii;
    /// - GamePad si hay intención Wii U pendiente y hay enlace;
    /// - si no, el layout Wii (vertical / NES apaisado según orientación).
    static func route(_ link: UiLink, _ intent: PadIntent) -> PadScreen {
        if let c = link.connected, c.role == LinkState.roleNunchuk { return .nunchuk }
        if isGamePad(link) { return .gamePad }
        if intent == .wiiU, link.alive { return .gamePad }
        return .wii
    }

    /// Layout Wii apaisado = mando + Nunchuk en un solo móvil: modo Dolphin,
    /// este móvil es mando y el receptor ha confirmado el Nunchuk propio (un
    /// receptor antiguo no lo confirma y se queda el NES de siempre).
    static func wiiLandscapeNunchuk(_ link: UiLink) -> Bool {
        guard let c = link.connected else { return false }
        return c.mode == LinkState.modeDolphin && c.role == LinkState.roleWiimote && c.ownNunchuk
    }

    /// Con el mando de lado (NES) el móvil ES un Mando de Wii girado: en
    /// Dolphin y en Wii U como Mando de Wii el juego espera el mando con el
    /// extremo IR a la izquierda y aplica él mismo el giro (cruceta y
    /// acelerómetro). Solo entonces: en modo puntero las flechas siguen
    /// siendo flechas del PC.
    static func sidewaysDpad(_ link: UiLink) -> Bool {
        guard let c = link.connected else { return false }
        return c.mode == LinkState.modeDolphin || (c.mode == LinkState.modeCemu && c.pad == LinkState.padWiimote)
    }

    /// Giro de los sensores con el mando de lado (NES). Como mando girado
    /// (Dolphin, Wii U-Mando): el móvil con el borde superior a la izquierda
    /// (rotation90) ya es el mando con el IR a la izquierda, y con el borde a
    /// la derecha (rotation270) se gira 180° para que dé igual hacia dónde se
    /// gire. En modo puntero el móvil de lado apunta con su borde largo, como
    /// el GamePad: los sensores se remapean con la rotación de la pantalla.
    static func sidewaysRotation(_ link: UiLink, displayRotation: Int) -> Int {
        if sidewaysDpad(link) {
            return displayRotation == Frame.rotation270 ? Frame.rotation180 : Frame.rotation0
        }
        return displayRotation
    }

    /// Intención resultante y, si toca, el aviso a enseñar (clave de texto).
    struct Outcome: Equatable {
        let intent: PadIntent
        let warning: String?
    }

    /// Llega un eco/difusión de `mode`: consume la intención Wii U. Si el modo
    /// no es `cemu`, el layout Wii vuelve con un aviso: el PC no soporta Wii U
    /// o, si sí lo soporta pero este móvil no es el Jugador 1, que solo él
    /// cambia el modo. Si lo decidió el PC (`byPc`) la intención se descarta
    /// sin aviso: el notice del PC ya lo explica.
    static func afterModeEcho(_ intent: PadIntent, _ mode: String, _ link: UiLink, byPc: Bool = false) -> Outcome {
        if intent != .wiiU { return Outcome(intent: intent, warning: nil) }
        if mode == LinkState.modeCemu || byPc { return Outcome(intent: .none, warning: nil) }
        let c = link.connected
        let warning = (c != nil && c!.supportsCemu && c!.slot != 0) ? warnPlayer1 : warnNeeds13
        return Outcome(intent: .none, warning: warning)
    }
}
