import XCTest
@testable import PepoMote

final class UpdateManifestTests: XCTestCase {
    private let current = AppVersion(major: 1, minor: 10, patch: 0)

    func testStableCanonicalVersionIsRequired() throws {
        for version in ["v1.11.0", "1.11", "01.11.0", "1.11.0-beta", "1.11.0 ", "1.11.0.1"] {
            XCTAssertThrowsError(try UpdateRelease.decode(fixture(["version": version])), version)
        }
    }

    func testValidManifestAndLocalizedNotes() throws {
        let release = try UpdateRelease.decode(fixture())
        XCTAssertEqual(release.version.description, "1.11.0")
        XCTAssertEqual(release.notes(language: "es"), ["Mejora la conexión."])
        XCTAssertEqual(release.notes(language: "en"), ["Improves connection."])
        XCTAssertEqual(release.notes(language: "fr"), ["Improves connection."])
        XCTAssertEqual(release.asset.size, 12345)
        XCTAssertEqual(release.asset.sha256, String(repeating: "a", count: 64))
    }

    func testRejectsUntrustedOrMismatchedAssetAndReleaseURLs() throws {
        for url in ["http://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.ipa",
                    "https://github.com/another/PepoMote/releases/download/v1.11.0/PepoMote.ipa",
                    "https://github.com/pepitolas13/PepoMote/releases/download/v1.10.0/PepoMote.ipa",
                    "https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.ipa?x=1",
                    "https://github.com@evil.test/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.ipa"] {
            XCTAssertThrowsError(try UpdateRelease.decode(fixture(asset: ["url": url])))
        }
        XCTAssertThrowsError(try UpdateRelease.decode(fixture(["release_url": "https://evil.test/releases/tag/v1.11.0"])))
        XCTAssertThrowsError(try UpdateRelease.decode(fixture(asset: ["name": "Other.ipa"])))
        XCTAssertThrowsError(try UpdateRelease.decode(fixture(["assets": [:]])))
    }

    func testBoundsSchemaHashesAndPublicationDate() throws {
        XCTAssertThrowsError(try UpdateRelease.decode(Data(repeating: 32, count: UpdateCheck.maxManifestBytes + 1)))
        for change: [String: Any] in [["schema": 2], ["published_at": "yesterday"], ["published_at": "2026-09-19T12:00:00+02:00"]] {
            XCTAssertThrowsError(try UpdateRelease.decode(fixture(change)))
        }
        for size in [0, -1, UpdateCheck.maxPackageBytes + 1] {
            XCTAssertThrowsError(try UpdateRelease.decode(fixture(asset: ["size": size])))
        }
        for hash in ["", "abcd", String(repeating: "g", count: 64)] {
            XCTAssertThrowsError(try UpdateRelease.decode(fixture(asset: ["sha256": hash])))
        }
    }

    func testNotesAreBoundedAndBothLanguagesAreRequired() throws {
        for notes in [["es": ["Nota"], "en": Array(repeating: "Note", count: 9)],
                      ["es": ["Nota"], "en": [String(repeating: "x", count: 281)]],
                      ["es": ["Nota"], "en": ["hidden\nline"]], ["es": ["Nota"], "en": [" "]],
                      ["es": [], "en": ["Note"]], ["en": ["Note"]]] {
            XCTAssertThrowsError(try UpdateRelease.decode(fixture(["notes": notes])))
        }
        let release = try UpdateRelease.decode(fixture(["notes": ["es": ["Latencia < 5 ms"], "en": ["Latency < 5 ms"]]]))
        XCTAssertEqual(release.notes(language: "fr"), ["Latency < 5 ms"])
    }

    func testHourlyChecksBackoffDisabledSettingAndClockRollback() {
        var schedule = UpdateSchedule()
        XCTAssertTrue(schedule.due(enabled: true, now: 100))
        XCTAssertFalse(schedule.due(enabled: false, now: 100))
        schedule.succeeded(at: 100)
        XCTAssertFalse(schedule.due(enabled: true, now: 3699))
        XCTAssertTrue(schedule.due(enabled: true, now: 3700))
        XCTAssertTrue(schedule.due(enabled: true, now: 99), "rollback must not suppress checks indefinitely")
        schedule.failed(at: 3700)
        XCTAssertFalse(schedule.due(enabled: true, now: 3759))
        XCTAssertTrue(schedule.due(enabled: true, now: 3760))
        schedule.failed(at: 3760)
        XCTAssertFalse(schedule.due(enabled: true, now: 3879))
        XCTAssertTrue(schedule.due(enabled: true, now: 3880))
        for _ in 0..<20 { schedule.failed(at: 4000) }
        XCTAssertEqual(schedule.retryDelay, 3600)
        schedule.succeeded(at: 5000)
        XCTAssertEqual(schedule.failures, 0)
        XCTAssertTrue(schedule.isFresh(now: 5001))
        XCTAssertFalse(schedule.isFresh(now: 4999))
        XCTAssertFalse(schedule.isFresh(now: 8600))
    }

    func testOfferRequiresFreshForegroundReleaseAndRemembersEachVersion() throws {
        let release = try UpdateRelease.decode(fixture())
        var offers = UpdateOfferHistory()
        XCTAssertTrue(offers.shouldOffer(release.version, current: current, enabled: true, active: true, fresh: true))
        XCTAssertFalse(offers.shouldOffer(release.version, current: current, enabled: false, active: true, fresh: true))
        XCTAssertFalse(offers.shouldOffer(release.version, current: current, enabled: true, active: false, fresh: true))
        XCTAssertFalse(offers.shouldOffer(release.version, current: current, enabled: true, active: true, fresh: false))
        offers.record(release.version)
        XCTAssertFalse(offers.shouldOffer(release.version, current: current, enabled: true, active: true, fresh: true))
        let restored = UpdateOfferHistory(versions: offers.versions)
        XCTAssertFalse(restored.shouldOffer(release.version, current: current, enabled: true, active: true, fresh: true))
        XCTAssertTrue(restored.shouldOffer(AppVersion(major: 1, minor: 12, patch: 0), current: current, enabled: true, active: true, fresh: true))
        XCTAssertFalse(restored.shouldOffer(current, current: current, enabled: true, active: true, fresh: true))
    }

    func testStoreLinksContainOnlyOfficialSourceAndClassicScheme() {
        for store in UpdateStore.allCases {
            let link = URLComponents(url: store.sourceURL, resolvingAgainstBaseURL: false)!
            XCTAssertEqual(link.host, "source")
            XCTAssertEqual(link.queryItems, [URLQueryItem(name: "url", value: UpdateCheck.sourceURL.absoluteString)])
        }
        XCTAssertEqual(UpdateStore.sideStore.sourceURL.scheme, "sidestore")
        XCTAssertEqual(UpdateStore.altStore.sourceURL.scheme, "altstore-classic")
        XCTAssertEqual(UpdateStore.altStoreLegacy.sourceURL.scheme, "altstore")
    }

    func testRedirectPolicyRejectsOtherRepositoriesAndInsecureHosts() {
        XCTAssertTrue(UpdateCheck.allowsMetadataURL(UpdateCheck.latestURL))
        XCTAssertTrue(UpdateCheck.allowsMetadataURL(URL(string: "https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/update.json")!))
        XCTAssertTrue(UpdateCheck.allowsMetadataURL(URL(string: "https://release-assets.githubusercontent.com/github-production-release-asset/123?sig=abc")!))
        for url in ["https://github.com/other/PepoMote/releases/download/v1.11.0/update.json",
                    "http://release-assets.githubusercontent.com/asset", "https://evil.test/update.json",
                    "https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.ipa"] {
            XCTAssertFalse(UpdateCheck.allowsMetadataURL(URL(string: url)!))
        }
    }

    private func fixture(_ changes: [String: Any] = [:], asset changesToAsset: [String: Any] = [:]) throws -> Data {
        var asset: [String: Any] = ["name": "PepoMote.ipa", "url": "https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.ipa", "size": 12345, "sha256": String(repeating: "a", count: 64)]
        asset.merge(changesToAsset) { _, new in new }
        var manifest: [String: Any] = ["schema": 1, "version": "1.11.0", "published_at": "2026-09-19T12:00:00Z", "notes": ["es": ["Mejora la conexión."], "en": ["Improves connection."]], "release_url": "https://github.com/pepitolas13/PepoMote/releases/tag/v1.11.0", "assets": ["ios": asset]]
        manifest.merge(changes) { _, new in new }
        return try JSONSerialization.data(withJSONObject: manifest)
    }
}
