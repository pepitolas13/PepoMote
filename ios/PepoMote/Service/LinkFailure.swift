import Foundation

/// Qué hacer cuando el enlace muere con un `err` del receptor (o "io" de la
/// red): función pura, testeada en LinkFailureTests.
enum LinkFailure {
    /// Códigos con los que el PC rechaza el emparejamiento guardado.
    private static let needsQr: Set<String> = ["bad_token", "bad_code"]

    /// El PC ya no acepta el token guardado: solo un QR nuevo lo arregla.
    static func needsNewQr(_ code: String) -> Bool { needsQr.contains(code) }

    /// Fallo de red con varios PCs guardados: a Conectar a elegir otro.
    static func offerAnotherPc(_ code: String, _ savedCount: Int) -> Bool { code == "io" && savedCount >= 2 }

    /// Nombre del PC para las explicaciones, o `fallback` («Tu PC») si no se sabe.
    static func pcLabel(_ pcName: String?, _ fallback: String) -> String {
        let t = pcName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return t.isEmpty ? fallback : t
    }
}
