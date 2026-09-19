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
    static let warnNeedsSwitch = "warn_needs_switch"
    static let warnNeedsRetroArch = "warn_needs_retroarch"

    /// Modo Wii U activo como GamePad/Pro: el receptor confirmó `cemu` y este
    /// móvil (mando) no ha elegido ser Mando de Wii. Solo entonces se emiten
    /// paquetes de 80 bytes.
    static func isGamePad(_ link: UiLink) -> Bool {
        guard let c = link.connected else { return false }
        return c.mode == LinkState.modeCemu && c.role == LinkState.roleWiimote && c.pad != LinkState.padWiimote
    }

    /// ¿Se enseña el botón «Teclado»? El receptor teclea en todos estos modos
    /// (PROTOCOL.md §3: en Wii U a la ventana de Cemu, en los demás a la que
    /// tenga el foco), pero solo si dijo que sabe (`ok.text_input`: el
    /// servidor de Android no).
    ///
    /// En modo puntero, además, solo el Jugador 1 con papel de mando: los
    /// demás móviles no mueven el cursor, así que no tienen dónde escribir.
    static func showsKeyboard(_ link: UiLink) -> Bool {
        guard let c = link.connected, c.receiver.textInput else { return false }
        switch c.mode {
        case LinkState.modePointer: return pointsAtPc(c)
        case LinkState.modeCemu, LinkState.modeSwitch, LinkState.modeRetroArch: return true
        default: return false
        }
    }

    /// ¿El botón «Teclado» hace antes un clic izquierdo donde apunta el
    /// usuario, para dejar el cursor dentro del campo?
    ///
    /// Solo en modo puntero: en Dolphin, Wii U y Switch los botones van al
    /// mando emulado (DSU) y el clic no llegaría al escritorio —pulsaría A
    /// dentro del juego—, y en RetroArch con pistola A es el clic DERECHO
    /// (recargar), que abriría un menú contextual sobre la partida.
    static func clicksBeforeKeyboard(_ link: UiLink, _ setting: Bool) -> Bool {
        guard setting, let c = link.connected else { return false }
        return c.mode == LinkState.modePointer && pointsAtPc(c)
    }

    /// ¿Se congela la pose mientras el teclado está abierto? Solo cuando este
    /// móvil mueve el cursor: escribiendo, el móvil se mueve en la mano y el
    /// puntero se iría solo (y donde el foco sigue al ratón, el texto acabaría
    /// en otra ventana).
    static func holdsPointerForKeyboard(_ link: UiLink) -> Bool {
        guard let c = link.connected else { return false }
        return pointsAtPc(c) && (c.mode == LinkState.modePointer ||
            (isRetroArch(link) && c.receiver.supportsPointer &&
                [LinkState.padNes, LinkState.padGun].contains(c.pad)))
    }

    /// Este móvil es el que mueve el cursor del PC: Jugador 1 y mando.
    private static func pointsAtPc(_ c: ConnectedLink) -> Bool {
        c.slot == 0 && c.role == LinkState.roleWiimote
    }

    static func isSwitch(_ link: UiLink) -> Bool {
        guard let c = link.connected else { return false }
        return c.mode == LinkState.modeSwitch && c.role == LinkState.roleWiimote
    }

    /// Modo RetroArch confirmado por un receptor que lo entiende (este móvil es mando).
    static func isRetroArch(_ link: UiLink) -> Bool {
        guard let c = link.connected else { return false }
        return c.mode == LinkState.modeRetroArch && c.role == LinkState.roleWiimote && c.supportsRetroArch
    }

    /// RetroArch como RetroPad: el mando apaisado de dos sticks (80 bytes, como Switch).
    static func isRetroPad(_ link: UiLink) -> Bool {
        isRetroArch(link) && link.connected?.pad == LinkState.padRetroPad
    }

    /// Mando Wii de RetroArch: vertical o apaisado según el teléfono.
    static func retroNes(_ link: UiLink) -> Bool {
        isRetroArch(link) && link.connected?.pad == LinkState.padNes
    }

    /// RetroArch como pistola de luz: el Mando Wii apuntando (B dispara, A recarga).
    static func retroGun(_ link: UiLink) -> Bool {
        isRetroArch(link) && link.connected?.pad == LinkState.padGun
    }

    /// La intención manda mientras se espera el eco, aunque todas usen GamePadScreen.
    static func wantedMode(_ link: UiLink, _ intent: PadIntent) -> String {
        intent.mode ?? (isSwitch(link) ? LinkState.modeSwitch : isRetroPad(link) ? LinkState.modeRetroArch : LinkState.modeCemu)
    }

    /// Solo un modo confirmado puede emitir el bloque de 80 bytes.
    static func extendedOperative(_ link: UiLink, _ intent: PadIntent) -> Bool {
        guard isGamePad(link) || isSwitch(link) || isRetroPad(link) else { return false }
        return intent.mode == nil || intent.mode == link.connected?.mode
    }

    /// - Nunchuk si el enlace es un Nunchuk (nunca es GamePad, pida lo que pida);
    /// - GamePad si el receptor confirmó `cemu` y este mando no es Mando de Wii;
    /// - GamePad si hay intención Wii U pendiente y hay enlace;
    /// - si no, el layout Wii (vertical / NES apaisado según orientación).
    static func route(_ link: UiLink, _ intent: PadIntent) -> PadScreen {
        if let c = link.connected, c.role == LinkState.roleNunchuk { return .nunchuk }
        if let wanted = intent.mode, link.alive, link.connected?.mode != wanted { return .gamePad }
        if isSwitch(link) { return .gamePad }
        if isRetroPad(link) { return .gamePad }
        if isGamePad(link) { return .gamePad }
        if intent != .none, link.alive { return .gamePad }
        return .wii
    }

    /// Layout Wii apaisado = mando + Nunchuk en un solo móvil: modo Dolphin,
    /// este móvil es mando y el receptor ha confirmado el Nunchuk propio (un
    /// receptor antiguo no lo confirma y se queda el NES de siempre).
    static func wiiLandscapeNunchuk(_ link: UiLink) -> Bool {
        guard let c = link.connected else { return false }
        return c.mode == LinkState.modeDolphin && c.role == LinkState.roleWiimote && c.ownNunchuk
    }

    /// Pantallas del mando que van fijas en apaisado: el GamePad de Wii U y el
    /// mando + Nunchuk de Dolphin. El mando se gira solo: no hace falta girar
    /// el móvil.
    static func forcesLandscape(_ link: UiLink, _ intent: PadIntent) -> Bool {
        let screen = route(link, intent)
        return screen == .gamePad || wiiLandscapeNunchuk(link)
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
        guard let wanted = intent.mode else { return Outcome(intent: intent, warning: nil) }
        if mode == wanted || byPc { return Outcome(intent: .none, warning: nil) }
        let c = link.connected
        let supported: Bool? = wanted == LinkState.modeSwitch ? c?.supportsSwitch
            : wanted == LinkState.modeRetroArch ? c?.supportsRetroArch
            : c?.supportsCemu
        let oldReceiver = wanted == LinkState.modeSwitch ? warnNeedsSwitch
            : wanted == LinkState.modeRetroArch ? warnNeedsRetroArch
            : warnNeeds13
        let warning = supported == true && c?.slot != 0 ? warnPlayer1 : oldReceiver
        return Outcome(intent: .none, warning: warning)
    }
}
