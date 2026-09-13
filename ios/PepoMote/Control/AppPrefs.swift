import Foundation

/// Preferencias simples de la app (aparte del emparejamiento), en UserDefaults.
enum AppPrefs {
    static let langSystem = "system"

    private static var d: UserDefaults { .standard }

    /// Mostrar el selector Puntero/Dolphin en el mando al entrar por Conectar.
    static var showDolphinChips: Bool {
        get { d.object(forKey: "showDolphinChips") as? Bool ?? true }
        set { d.set(newValue, forKey: "showDolphinChips") }
    }

    static var soundsEnabled: Bool {
        get { d.object(forKey: "sounds") as? Bool ?? true }
        set {
            d.set(newValue, forKey: "sounds")
            UiSounds.shared.enabled = newValue
        }
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

    /// Avisos del receptor en pantalla («Dolphin configurado…»): unos
    /// segundos sobre el mando al cambiar de modo. Apagados, no se enseñan.
    static var receiverNotices: Bool {
        get { d.object(forKey: "receiverNotices") as? Bool ?? true }
        set { d.set(newValue, forKey: "receiverNotices") }
    }

    /// GamePad de Wii U sin pantalla táctil (ni doble pantalla): los botones
    /// crecen (Ajustes). `GamePadScreen` lo observa con @AppStorage.
    static let gamePadNoScreenKey = "gamePadNoScreen"
    static var gamePadNoScreen: Bool {
        get { d.bool(forKey: gamePadNoScreenKey) }
        set { d.set(newValue, forKey: gamePadNoScreenKey) }
    }

    // MARK: aviso de versión nueva (UpdateCheck)

    /// Consultar GitHub una vez al día si hay versión nueva (Ajustes).
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
