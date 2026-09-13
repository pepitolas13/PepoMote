import Foundation

/// Versión `mayor.menor.parche`, comparada numéricamente (1.10 > 1.9).
struct AppVersion: Equatable, Comparable, CustomStringConvertible {
    let major: Int
    let minor: Int
    let patch: Int

    /// `v1.6.0`, `1.6.0` o `v1.6` (→ 1.6.0). Sufijos (`-beta`) o basura → nil.
    static func parse(_ s: String?) -> AppVersion? {
        guard var t = s?.trimmingCharacters(in: .whitespacesAndNewlines) else { return nil }
        if t.hasPrefix("v") || t.hasPrefix("V") { t.removeFirst() }
        if t.isEmpty { return nil }
        let parts = t.split(separator: ".", omittingEmptySubsequences: false)
        guard (2...3).contains(parts.count) else { return nil }
        var nums: [Int] = []
        for p in parts {
            guard !p.isEmpty, p.allSatisfy({ $0.isASCII && $0.isNumber }), let n = Int(p) else { return nil }
            nums.append(n)
        }
        return AppVersion(major: nums[0], minor: nums[1], patch: nums.count > 2 ? nums[2] : 0)
    }

    static func < (a: AppVersion, b: AppVersion) -> Bool {
        (a.major, a.minor, a.patch) < (b.major, b.minor, b.patch)
    }

    var description: String { "\(major).\(minor).\(patch)" }
}

/// Aviso de versión nueva: la lógica pura. Privacidad: la única petición que
/// sale de la red local es un HEAD a `releases/latest` de GitHub, que
/// responde con una redirección a la etiqueta de la última versión publicada.
/// Sin cuerpo, sin identificadores, una vez al día, y se apaga en Ajustes.
/// Mismos casos que el receptor (desktop/src/update.rs) y que Android.
enum UpdateCheck {
    static let repo = "pepitolas13/PepoMote"
    static let latestURL = URL(string: "https://github.com/\(repo)/releases/latest")!
    /// Fijo y sin versión: GitHub solo ve «un PepoMote», nada más.
    static let userAgent = "PepoMote-update-check"
    static let checkEvery: TimeInterval = 24 * 3600
    /// La primera consulta espera a que el inicio esté en pantalla.
    static let firstDelay: TimeInterval = 3
    /// Sin red o GitHub caído: se vuelve a mirar en una hora, no antes.
    static let retryAfter: TimeInterval = 3600
    static let timeout: TimeInterval = 5

    /// `…/releases/tag/v1.6.0` → 1.6.0 (también con `?x`, `#x` o `/` detrás).
    static func version(fromLocation location: String?) -> AppVersion? {
        guard let location, let range = location.range(of: "/tag/", options: .backwards) else { return nil }
        let tag = location[range.upperBound...].prefix { $0 != "?" && $0 != "#" && $0 != "/" }
        return AppVersion.parse(String(tag))
    }

    /// Página de la release en GitHub (la que se abre desde el aviso).
    static func releaseURL(_ v: AppVersion) -> URL {
        URL(string: "https://github.com/\(repo)/releases/tag/v\(v)")!
    }

    /// La versión que hay que anunciar: la última publicada si es mayor que la
    /// actual y el usuario no la ocultó (una posterior sí se anuncia).
    static func pending(current: AppVersion?, latest: AppVersion?, dismissed: AppVersion?) -> AppVersion? {
        guard let current, let latest, latest > current, latest != dismissed else { return nil }
        return latest
    }

    /// ¿Toca consultar? Activado y, o nunca se consultó (`last == 0`), o han
    /// pasado 24 h desde la última vez (un reloj hacia atrás no dispara nada).
    static func due(enabled: Bool, last: TimeInterval, now: TimeInterval) -> Bool {
        enabled && (last == 0 || now - last >= checkEvery)
    }

    /// La versión instalada (CFBundleShortVersionString).
    static var current: AppVersion? {
        AppVersion.parse(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String)
    }
}
