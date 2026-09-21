import Foundation

/// Preferencias simples de la app (aparte del emparejamiento), en UserDefaults.
enum AppPrefs {
    static let langSystem = "system"

    private static var d: UserDefaults { .standard }

    static let cemuPadKey = "cemuPad"
    static var cemuPad: String {
        get { d.string(forKey: cemuPadKey) == LinkState.padWiimote ? LinkState.padWiimote : LinkState.padGamepad }
        set { d.set(newValue == LinkState.padWiimote ? newValue : LinkState.padGamepad, forKey: cemuPadKey) }
    }

    static let switchPadKey = "switchPad"
    static var switchPad: String {
        get {
            let pad = d.string(forKey: switchPadKey) ?? LinkState.padPro
            return LinkState.validSwitchPad(pad) ? pad : LinkState.padPro
        }
        set { d.set(LinkState.validSwitchPad(newValue) ? newValue : LinkState.padPro, forKey: switchPadKey) }
    }

    /// Mando de RetroArch (retropad / nes / gun); cualquier otro valor es el RetroPad.
    static let retroPadKey = "retroPad"
    static var retroPad: String {
        get {
            let pad = d.string(forKey: retroPadKey) ?? LinkState.padRetroPad
            return LinkState.validRetroPad(pad) ? pad : LinkState.padRetroPad
        }
        set { d.set(LinkState.validRetroPad(newValue) ? newValue : LinkState.padRetroPad, forKey: retroPadKey) }
    }

    /// RetroArch: mando de consola elegido a mano (JSON de `RetroLayoutChoice`, como Android).
    static let retroLayoutChoiceKey = "retroLayouts"
    static var retroLayoutChoice: RetroLayoutChoice {
        get { RetroLayoutChoice.decode(d.string(forKey: retroLayoutChoiceKey)) }
        set { d.set(newValue.encode(), forKey: retroLayoutChoiceKey) }
    }

    /// Mostrar el selector Puntero/Dolphin en el mando al entrar por Conectar.
    static var showDolphinChips: Bool {
        get { d.object(forKey: "showDolphinChips") as? Bool ?? true }
        set { d.set(newValue, forKey: "showDolphinChips") }
    }

    /// «Multimedia en todos los modos»: la fila multimedia del mando vertical
    /// sale siempre en modo puntero (el único en el que el PC atiende esas
    /// teclas); con esto encendido sale también en Dolphin, Wii U y RetroArch.
    /// Apagado de serie.
    static let mediaEverywhereKey = "mediaEverywhere"
    static var mediaEverywhere: Bool {
        get { d.bool(forKey: mediaEverywhereKey) }
        set { d.set(newValue, forKey: mediaEverywhereKey) }
    }

    static var soundsEnabled: Bool {
        get { d.object(forKey: "sounds") as? Bool ?? true }
        set {
            d.set(newValue, forKey: "sounds")
            UiSounds.shared.enabled = newValue
        }
    }

    /// Pulsar deslizando: el botón que queda bajo el dedo se pulsa, y
    /// deslizar a otro suelta el primero y pulsa el nuevo (un dedo que nace en
    /// el vacío también pulsa al entrar). Apagado de serie. Clave pública: los
    /// botones y el lienzo lo miran con @AppStorage.
    static let slidePressKey = "slidePress"
    static var slidePress: Bool {
        get { d.bool(forKey: slidePressKey) }
        set { d.set(newValue, forKey: slidePressKey) }
    }

    /// Mantener al salir del botón: un botón pulsado sigue pulsado mientras el
    /// dedo toque la pantalla, aunque se salga de él. Encendido de serie (lo
    /// de siempre en iOS); solo se puede elegir con «Pulsar deslizando»
    /// apagado, que manda en ejecución.
    static let stickyPressKey = "stickyPress"
    static var stickyPress: Bool {
        get { d.object(forKey: stickyPressKey) as? Bool ?? true }
        set { d.set(newValue, forKey: stickyPressKey) }
    }

    /// Idioma: "system", "es" o "en".
    static var lang: String {
        get { d.string(forKey: "lang") ?? langSystem }
        set { d.set(newValue, forKey: "lang") }
    }

    static var onboarded: Bool {
        get { d.bool(forKey: "onboarded") }
        set { d.set(newValue, forKey: "onboarded") }
    }

    /// Nunchuk en el mismo móvil (modo Dolphin): el mando lleva su propio
    /// Nunchuk y en apaisado sale el trazado con stick, C y Z. Apagado de
    /// serie (entrar en Dolphin nunca lo enciende solo); se recuerda porque
    /// cambiarlo obliga a reabrir Dolphin.
    static var ownNunchuk: Bool {
        get { d.object(forKey: "ownNunchuk") as? Bool ?? false }
        set { d.set(newValue, forKey: "ownNunchuk") }
    }

    /// Lado del mando + Nunchuk apaisado (`LandscapeSide`): "" hasta que se
    /// elige la primera vez; "left", "right" o "sensor" (Ajustes).
    static let nunchukSideKey = "nunchukSide"
    static var nunchukSide: String {
        get { d.string(forKey: nunchukSideKey) ?? "" }
        set { d.set(newValue, forKey: nunchukSideKey) }
    }

    /// Lado del GamePad de Wii U apaisado (`LandscapeSide`), aparte del del mando + Nunchuk.
    static let gamePadSideKey = "gamePadSide"
    static var gamePadSide: String {
        get { d.string(forKey: gamePadSideKey) ?? "" }
        set { d.set(newValue, forKey: gamePadSideKey) }
    }

    /// El botón «Teclado» del modo puntero hace antes un clic izquierdo donde
    /// apunta el usuario, para dejar el cursor dentro del campo. Apagado, el
    /// clic lo da el usuario con A antes de escribir. De serie, encendido.
    static var keyboardClickFirst: Bool {
        get { d.object(forKey: "kbClickFirst") as? Bool ?? true }
        set { d.set(newValue, forKey: "kbClickFirst") }
    }

    static let padAimAsk = ""
    static let padAimOn = "on"
    static let padAimOff = "off"

    /// Mando universal: mover el móvil mueve el stick derecho del mando de
    /// Xbox. "" hasta que se elige (el mando lo pregunta la primera vez que
    /// conecta), "on" u "off"; Ajustes lo cambia cuando se quiera. Sin elegir
    /// va apagado: así ningún juego hace cosas raras mientras se lee el
    /// aviso. Solo vale para ese modo. Clave pública: el mando la mira con
    /// @AppStorage. Igual que en Android.
    static let padAimKey = "padAim"
    static var padAim: String {
        get { d.string(forKey: padAimKey) ?? padAimAsk }
        set { d.set(newValue, forKey: padAimKey) }
    }

    static var padAimEnabled: Bool { padAim == padAimOn }

    /// Avisos del receptor en pantalla («Dolphin configurado…»): unos
    /// segundos sobre el mando al cambiar de modo. Apagados, no se enseñan.
    static var receiverNotices: Bool {
        get { d.object(forKey: "receiverNotices") as? Bool ?? true }
        set { d.set(newValue, forKey: "receiverNotices") }
    }

    /// Vibración en los juegos (RUMBLE del receptor): "high" | "normal" |
    /// "low" | "off", normal de serie; un valor desconocido se lee como
    /// normal. La escala se lee en cada orden al motor: cambiarla vale al
    /// instante, sin reconectar.
    static let rumbleKey = "rumble"
    static var rumble: String {
        get { RumblePref.normalize(d.string(forKey: rumbleKey)) }
        set { d.set(RumblePref.normalize(newValue), forKey: rumbleKey) }
    }

    /// GamePad de Wii U sin pantalla táctil (ni doble pantalla): los botones
    /// crecen (Ajustes). `GamePadScreen` lo observa con @AppStorage.
    static let gamePadNoScreenKey = "gamePadNoScreen"
    static var gamePadNoScreen: Bool {
        get { d.bool(forKey: gamePadNoScreenKey) }
        set {
            d.set(newValue, forKey: gamePadNoScreenKey)
            // excluyente con la pantalla completa
            if newValue { d.set(false, forKey: gamePadFullScreenKey) }
        }
    }

    /// Pantalla del GamePad a pantalla completa: solo la pantalla de Cemu y el
    /// táctil, sin sticks ni botones (el mando real va en el PC). Ajustes.
    /// `GamePadScreen` lo observa con @AppStorage.
    static let gamePadFullScreenKey = "gamePadFullScreen"
    static var gamePadFullScreen: Bool {
        get { d.bool(forKey: gamePadFullScreenKey) }
        set {
            d.set(newValue, forKey: gamePadFullScreenKey)
            // excluyente con «GamePad sin pantalla táctil»
            if newValue { d.set(false, forKey: gamePadNoScreenKey) }
        }
    }

    /// En pantalla completa, botón de teclado arriba a la derecha (Ajustes).
    static let gamePadFullScreenKeyboardKey = "gamePadFullScreenKeyboard"
    static var gamePadFullScreenKeyboard: Bool {
        get { d.object(forKey: gamePadFullScreenKeyboardKey) as? Bool ?? true }
        set { d.set(newValue, forKey: gamePadFullScreenKeyboardKey) }
    }

    // MARK: aviso de versión nueva (UpdateCheck)

    /// Consultar el manifiesto de GitHub cada hora en primer plano (Ajustes).
    static var updateCheckEnabled: Bool {
        get { d.object(forKey: "updateCheck") as? Bool ?? true }
        set { d.set(newValue, forKey: "updateCheck") }
    }

    /// Versión anunciada que se ocultó ("" = ninguna).
    static var updateDismissed: String {
        get { d.string(forKey: "updateDismissed") ?? "" }
        set { d.set(newValue, forKey: "updateDismissed") }
    }

    /// Última consulta (segundos UNIX, 0 = nunca) y última versión publicada vista.
    static var updateLast: TimeInterval {
        get { d.double(forKey: "updateLast") }
        set { d.set(newValue, forKey: "updateLast") }
    }

    static var updateLatest: String {
        get { d.string(forKey: "updateLatest") ?? "" }
        set { d.set(newValue, forKey: "updateLatest") }
    }
}
