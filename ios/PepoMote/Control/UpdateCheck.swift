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

/// Release metadata only; packages are installed by SideStore/AltStore Classic.
enum UpdateCheck {
    static let repo = "pepitolas13/PepoMote"
    static let latestURL = URL(string: "https://github.com/\(repo)/releases/latest/download/update.json")!
    static let sourceURL = URL(string: "https://github.com/\(repo)/releases/latest/download/altstore.json")!
    static let guideURL = URL(string: "https://github.com/\(repo)/blob/main/docs/IOS.md")!
    static let maxManifestBytes = 256 * 1024
    static let maxPackageBytes = 1024 * 1024 * 1024
    /// Fijo y sin versión: GitHub solo ve «un PepoMote», nada más.
    static let userAgent = "PepoMote-update-check"
    static let checkEvery: TimeInterval = 3600
    /// La primera consulta espera a que el inicio esté en pantalla.
    static let firstDelay: TimeInterval = 3
    static let timeout: TimeInterval = 15

    /// Página de la release en GitHub (la que se abre desde el aviso).
    static func releaseURL(_ v: AppVersion) -> URL {
        URL(string: "https://github.com/\(repo)/releases/tag/v\(v)")!
    }

    /// A clock rollback must not indefinitely suppress future checks.
    static func due(enabled: Bool, last: TimeInterval, now: TimeInterval) -> Bool {
        enabled && (last <= 0 || now < last || now - last >= checkEvery)
    }

    static func allowsMetadataURL(_ url: URL) -> Bool {
        guard url.scheme == "https", url.user == nil, url.password == nil,
              url.port == nil, url.fragment == nil else { return false }
        if url.host == "release-assets.githubusercontent.com" || url.host == "objects.githubusercontent.com" {
            return true
        }
        guard url.host == "github.com", url.query == nil else { return false }
        if url == latestURL { return true }
        let prefix = "/\(repo)/releases/download/v"
        let suffix = "/update.json"
        guard url.path.hasPrefix(prefix), url.path.hasSuffix(suffix) else { return false }
        let version = String(url.path.dropFirst(prefix.count).dropLast(suffix.count))
        return AppVersion.parse(version)?.description == version
    }

    /// La versión instalada (CFBundleShortVersionString).
    static var current: AppVersion? {
        AppVersion.parse(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String)
    }
}

/// Persisted wall-clock state. Backoff survives repeated foreground transitions.
struct UpdateSchedule: Codable, Equatable {
    var lastSuccess: TimeInterval = 0
    var lastAttempt: TimeInterval = 0
    var failures: Int = 0

    var retryDelay: TimeInterval { min(3600, 60 * pow(2, Double(min(max(failures, 1), 7) - 1))) }

    func due(enabled: Bool, now: TimeInterval) -> Bool {
        guard enabled else { return false }
        if now < max(lastAttempt, lastSuccess) { return true }
        if failures > 0, lastAttempt > 0, now >= lastAttempt {
            return now - lastAttempt >= retryDelay
        }
        return UpdateCheck.due(enabled: enabled, last: lastSuccess, now: now)
    }

    func isFresh(now: TimeInterval) -> Bool {
        failures == 0 && lastSuccess > 0 && now >= lastSuccess && now - lastSuccess < UpdateCheck.checkEvery
    }

    mutating func succeeded(at now: TimeInterval) {
        lastAttempt = now
        lastSuccess = now
        failures = 0
    }

    mutating func failed(at now: TimeInterval) {
        lastAttempt = now
        failures = min(max(0, failures), 6) + 1
    }
}

struct UpdateOfferHistory {
    private(set) var versions: [String] = []

    init(versions: [String] = []) {
        self.versions = versions.filter { AppVersion.parse($0)?.description == $0 }
    }

    func shouldOffer(_ version: AppVersion, current: AppVersion?, enabled: Bool, active: Bool, fresh: Bool) -> Bool {
        guard enabled, active, fresh, let current else { return false }
        return version > current && !versions.contains(version.description)
    }

    mutating func record(_ version: AppVersion) {
        if !versions.contains(version.description) { versions.append(version.description) }
    }
}

enum UpdateStore: CaseIterable, Hashable {
    case sideStore, altStore, altStoreLegacy

    var name: String {
        switch self {
        case .sideStore: return "SideStore"
        case .altStore: return "AltStore Classic"
        case .altStoreLegacy: return "AltStore"
        }
    }

    var sourceURL: URL {
        var components = URLComponents()
        switch self {
        case .sideStore: components.scheme = "sidestore"
        case .altStore: components.scheme = "altstore-classic"
        case .altStoreLegacy: components.scheme = "altstore"
        }
        components.host = "source"
        components.queryItems = [URLQueryItem(name: "url", value: UpdateCheck.sourceURL.absoluteString)]
        return components.url!
    }
}
