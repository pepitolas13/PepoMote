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
}
