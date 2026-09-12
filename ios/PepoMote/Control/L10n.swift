import Foundation

/// Idioma de la app: el del sistema (inglés si lo es; español para todo lo
/// demás) o el elegido con el botón ES/EN del inicio. Los textos viven en
/// es.lproj/en.lproj/Localizable.strings; se leen del bundle del idioma
/// activo, y al cambiarlo la raíz de la UI se recompone entera.
final class L10n: ObservableObject {
    static let shared = L10n()

    /// Código activo de verdad: "es" o "en".
    @Published private(set) var code: String
    private var bundle: Bundle

    init() {
        let c = L10n.resolve(AppPrefs.lang)
        code = c
        bundle = L10n.bundle(for: c)
    }

    /// "system" → el del sistema (solo hay español e inglés).
    static func resolve(_ pref: String) -> String {
        if pref == "es" || pref == "en" { return pref }
        let first = Locale.preferredLanguages.first ?? "es"
        return first.lowercased().hasPrefix("en") ? "en" : "es"
    }

    static func bundle(for code: String) -> Bundle {
        if let path = Bundle.main.path(forResource: code, ofType: "lproj"), let b = Bundle(path: path) {
            return b
        }
        return .main
    }

    func t(_ key: String) -> String {
        bundle.localizedString(forKey: key, value: key, table: nil)
    }

    func t(_ key: String, _ args: CVarArg...) -> String {
        String(format: t(key), locale: Locale.current, arguments: args)
    }

    /// El otro idioma, para el botón.
    var otherCode: String { code == "en" ? "es" : "en" }

    static func name(_ code: String) -> String { code == "en" ? "English" : "Español" }

    /// Botón ES/EN del inicio: guarda el otro idioma y lo aplica.
    func toggle() {
        let other = otherCode
        AppPrefs.lang = other
        bundle = L10n.bundle(for: other)
        code = other
    }
}

/// Atajo para las vistas: `tr("exit")`, `tr("status_connected_to", pc)`.
func tr(_ key: String) -> String { L10n.shared.t(key) }
func tr(_ key: String, _ args: CVarArg...) -> String {
    String(format: L10n.shared.t(key), locale: Locale.current, arguments: args)
}
