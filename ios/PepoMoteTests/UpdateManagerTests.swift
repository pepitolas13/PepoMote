import XCTest
@testable import PepoMote

final class UpdateManagerTests: XCTestCase {
    @MainActor
    func testDisabledAutomaticChecksStillAllowManualCheck() async throws {
        let suite = "UpdateManagerTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        defaults.set(false, forKey: "updateCheck")
        let release = try fixture()
        var calls = 0
        let manager = UpdateManager(defaults: defaults, current: AppVersion.parse("1.10.0"), now: { 100 }) {
            calls += 1
            return release
        }
        defer { manager.setForeground(false) }
        manager.setPresentationAllowed(true)
        manager.setForeground(true)
        let automatic = await manager.check()
        XCTAssertFalse(automatic)
        XCTAssertEqual(calls, 0)
        let manual = await manager.check(force: true)
        XCTAssertTrue(manual)
        XCTAssertEqual(calls, 1)
        XCTAssertEqual(manager.available, release)
        XCTAssertNil(manager.offer, "manual lookup must not re-enable automatic prompts")
    }

    @MainActor
    func testDismissedOfferKeepsNotesAfterRestartAndStoreOpeningIsNotInstallation() async throws {
        let suite = "UpdateManagerTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let release = try fixture()
        let manager = UpdateManager(defaults: defaults, current: AppVersion.parse("1.10.0"), now: { 100 }) { release }
        manager.setForeground(true)
        manager.setPresentationAllowed(true)
        let checked = await manager.check(force: true)
        XCTAssertTrue(checked)
        XCTAssertEqual(manager.offer, release)
        manager.markOfferPresented(release)
        manager.dismissOffer()
        manager.recordHandoff(store: .sideStore, opened: true)
        XCTAssertEqual(manager.available, release)
        manager.setForeground(false)
        let restored = UpdateManager(defaults: defaults, current: AppVersion.parse("1.10.0"), now: { 101 }) { release }
        restored.setPresentationAllowed(true)
        restored.setForeground(true)
        defer { restored.setForeground(false) }
        XCTAssertNil(restored.offer)
        XCTAssertEqual(restored.release?.notes(language: "es"), ["Mejora la conexión."])
        XCTAssertEqual(restored.available, release)
    }

    @MainActor
    func testStaleCachedOfferRequiresRevalidationAndFailureIsNotUpToDate() async throws {
        let suite = "UpdateManagerTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let release = try fixture()
        defaults.set(release.metadata, forKey: "updateManifest")
        var schedule = UpdateSchedule()
        schedule.succeeded(at: 100)
        defaults.set(try JSONEncoder().encode(schedule), forKey: "updateSchedule")
        var calls = 0
        let manager = UpdateManager(defaults: defaults, current: AppVersion.parse("1.10.0"), now: { 4000 }) {
            calls += 1
            throw UpdateError.unavailable
        }
        manager.setPresentationAllowed(true)
        manager.setForeground(true)
        defer { manager.setForeground(false) }
        XCTAssertNil(manager.offer)
        XCTAssertFalse(manager.isFresh)
        let handoff = await manager.prepareHandoff(for: release.version)
        XCTAssertFalse(handoff)
        XCTAssertEqual(manager.statusKey, "update_failed")
        XCTAssertEqual(manager.available, release, "failure preserves recoverable cached notes")
        let retry = await manager.check()
        XCTAssertFalse(retry)
        XCTAssertEqual(calls, 1, "backoff prevents immediate automatic retries")
    }

    @MainActor
    func testConcurrentChecksShareOneRequestAndBackgroundResponseCannotPresent() async throws {
        let suite = "UpdateManagerTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let release = try fixture()
        let started = expectation(description: "fetch started")
        let gate = UpdateFetchGate(started: started)
        let manager = UpdateManager(defaults: defaults, current: AppVersion.parse("1.10.0"), now: { 100 }) {
            try await gate.fetch()
        }
        manager.setForeground(true)
        manager.setPresentationAllowed(true)
        let first = Task { await manager.check(force: true) }
        await fulfillment(of: [started], timeout: 2)
        manager.setForeground(true)
        let duplicate = await manager.check(force: true)
        XCTAssertFalse(duplicate)
        let calls = await gate.calls
        XCTAssertEqual(calls, 1)
        manager.setForeground(false)
        await gate.complete(release)
        let completed = await first.value
        XCTAssertFalse(completed)
        XCTAssertFalse(manager.checking)
        XCTAssertNil(manager.offer)
        XCTAssertNil(manager.release, "late cancelled requests cannot overwrite current state")
    }

    @MainActor
    func testInitialFailureBackoffSurvivesRestartWithoutAnyCachedRelease() async throws {
        let suite = "UpdateManagerTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        var calls = 0
        let fetch: () async throws -> UpdateRelease = {
            calls += 1
            throw UpdateError.unavailable
        }
        let first = UpdateManager(defaults: defaults, now: { 100 }, fetch: fetch)
        first.setForeground(true)
        let failed = await first.check(force: true)
        XCTAssertFalse(failed)
        first.setForeground(false)
        let restarted = UpdateManager(defaults: defaults, now: { 101 }, fetch: fetch)
        restarted.setForeground(true)
        defer { restarted.setForeground(false) }
        let retry = await restarted.check()
        XCTAssertFalse(retry)
        XCTAssertEqual(calls, 1)
        XCTAssertNil(restarted.release)
    }

    @MainActor
    func testAChangedVersionDuringRevalidationRequiresAnotherClick() async throws {
        let suite = "UpdateManagerTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let old = try fixture()
        let newer = try fixture(version: "1.12.0")
        defaults.set(old.metadata, forKey: "updateManifest")
        let manager = UpdateManager(defaults: defaults, current: AppVersion.parse("1.10.0"), now: { 4000 }) { newer }
        manager.setForeground(true)
        defer { manager.setForeground(false) }
        let handoff = await manager.prepareHandoff(for: old.version)
        XCTAssertFalse(handoff)
        XCTAssertEqual(manager.available, newer)
        XCTAssertEqual(manager.statusKey, "update_changed")
        let confirmed = await manager.prepareHandoff(for: newer.version)
        XCTAssertTrue(confirmed)
    }

    private func fixture(version: String = "1.11.0") throws -> UpdateRelease {
        let json: [String: Any] = [
            "schema": 1, "version": version, "published_at": "2026-09-19T12:00:00Z",
            "notes": ["es": ["Mejora la conexión."], "en": ["Improves connection."]],
            "release_url": "https://github.com/pepitolas13/PepoMote/releases/tag/v\(version)",
            "assets": ["ios": ["name": "PepoMote.ipa", "url": "https://github.com/pepitolas13/PepoMote/releases/download/v\(version)/PepoMote.ipa",
                               "size": 12345, "sha256": String(repeating: "a", count: 64)]]
        ]
        return try UpdateRelease.decode(JSONSerialization.data(withJSONObject: json))
    }
}

private actor UpdateFetchGate {
    let started: XCTestExpectation
    private(set) var calls = 0
    private var continuation: CheckedContinuation<UpdateRelease, Error>?

    init(started: XCTestExpectation) { self.started = started }

    func fetch() async throws -> UpdateRelease {
        calls += 1
        return try await withCheckedThrowingContinuation { continuation in
            self.continuation = continuation
            started.fulfill()
        }
    }

    func complete(_ release: UpdateRelease) {
        continuation?.resume(returning: release)
        continuation = nil
    }
}
