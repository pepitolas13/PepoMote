import Foundation

enum UpdateError: Error {
    case invalidManifest, tooLarge, unavailable
}

/// Constructed only after validating the release and this platform's package.
/// This does not verify IPA bytes: the external store performs its own install.
struct UpdateRelease: Equatable, Identifiable {
    struct Asset: Codable, Equatable {
        let name: String
        let url: String
        let size: Int
        let sha256: String
    }

    let version: AppVersion
    let publishedAt: Date
    let releaseURL: URL
    let asset: Asset
    private let localizedNotes: [String: [String]]
    /// Cache the validated contract, then validate it again when restoring.
    let metadata: Data
    var id: String { version.description }

    func notes(language: String) -> [String] {
        if let notes = localizedNotes[language], !notes.isEmpty { return notes }
        if let notes = localizedNotes["en"], !notes.isEmpty { return notes }
        return localizedNotes["es"] ?? []
    }

    static func decode(_ data: Data) throws -> UpdateRelease {
        guard !data.isEmpty, data.count <= UpdateCheck.maxManifestBytes else { throw UpdateError.tooLarge }
        let manifest = try JSONDecoder().decode(Manifest.self, from: data)
        guard manifest.schema == 1,
              let version = AppVersion.parse(manifest.version), version.description == manifest.version,
              manifest.release_url == UpdateCheck.releaseURL(version).absoluteString,
              let asset = manifest.assets["ios"], asset.name == "PepoMote.ipa",
              asset.url == "https://github.com/\(UpdateCheck.repo)/releases/download/v\(version)/PepoMote.ipa",
              asset.size > 0, asset.size <= UpdateCheck.maxPackageBytes,
              asset.sha256.count == 64,
              asset.sha256.allSatisfy({ $0.isASCII && $0.isHexDigit }),
              manifest.published_at.hasSuffix("Z"),
              let date = publicationDate(manifest.published_at),
              Set(manifest.notes.keys) == Set(["es", "en"]),
              manifest.notes.values.allSatisfy({ notes in
                  (1...8).contains(notes.count) && notes.allSatisfy { note in
                      !note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
                      note.unicodeScalars.count <= 280 &&
                      !note.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) || CharacterSet.newlines.contains($0) })
                  }
              }) else { throw UpdateError.invalidManifest }
        return UpdateRelease(version: version, publishedAt: date, releaseURL: UpdateCheck.releaseURL(version),
                             asset: asset, localizedNotes: manifest.notes, metadata: data)
    }

    private static func publicationDate(_ text: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        guard let date = formatter.date(from: text), formatter.string(from: date) == text else { return nil }
        return date
    }

    private struct Manifest: Decodable {
        let schema: Int
        let version: String
        let published_at: String
        let notes: [String: [String]]
        let release_url: String
        let assets: [String: Asset]
    }
}
