import Foundation
import Combine

/// One foreground loop and one request, shared by all screens and scene events.
@MainActor
final class UpdateManager: ObservableObject {
    static let shared = UpdateManager()

    @Published private(set) var release: UpdateRelease?
    @Published var offer: UpdateRelease?
    @Published private(set) var checking = false
    @Published private(set) var statusKey: String?
    @Published private(set) var isFresh = false
    @Published private(set) var handoffStatus: UpdateHandoffStatus?

    private let defaults: UserDefaults
    private let now: () -> TimeInterval
    private let current: AppVersion?
    private let fetch: () async throws -> UpdateRelease
    private var schedule: UpdateSchedule
    private var history: UpdateOfferHistory
    private var loop: Task<Void, Never>?
    private var request: Task<UpdateRelease, Error>?
    private var requestID: UUID?
    private var active = false
    private var presentationAllowed = false

    init(defaults: UserDefaults = .standard, current: AppVersion? = UpdateCheck.current,
         now: @escaping () -> TimeInterval = { Date().timeIntervalSince1970 },
         fetch: @escaping () async throws -> UpdateRelease = { try await UpdateClient.shared.latest() }) {
        self.defaults = defaults
        self.current = current
        self.now = now
        self.fetch = fetch
        schedule = defaults.data(forKey: "updateSchedule").flatMap { try? JSONDecoder().decode(UpdateSchedule.self, from: $0) } ?? UpdateSchedule()
        history = UpdateOfferHistory(versions: defaults.stringArray(forKey: "updateOfferedVersions") ?? [])
        // Preserve a dismissal from the previous updater without trusting its
        // version-only cache, which had no release metadata or artifact checks.
        if let dismissed = AppVersion.parse(defaults.string(forKey: "updateDismissed")) { history.record(dismissed) }
        release = defaults.data(forKey: "updateManifest").flatMap { try? UpdateRelease.decode($0) }
        // An old version-only/invalid cache cannot suppress the first metadata
        // lookup, but failures must still retain their backoff across restarts.
        if release == nil { schedule.lastSuccess = 0 }
        isFresh = release != nil && schedule.isFresh(now: now())
    }

    var enabled: Bool { defaults.object(forKey: "updateCheck") as? Bool ?? true }
    var available: UpdateRelease? {
        guard let current, let release, release.version > current else { return nil }
        return release
    }

    func setForeground(_ foreground: Bool) {
        active = foreground
        refreshFreshness()
        if foreground {
            presentIfNeeded()
            startLoop()
        } else {
            stopTasks()
            offer = nil
        }
    }

    func setPresentationAllowed(_ allowed: Bool) {
        presentationAllowed = allowed
        if allowed { presentIfNeeded() } else { offer = nil }
    }

    func setEnabled(_ enabled: Bool) {
        defaults.set(enabled, forKey: "updateCheck")
        if enabled {
            presentIfNeeded()
            startLoop()
        } else {
            stopTasks()
            offer = nil
        }
    }

    func markOfferPresented(_ release: UpdateRelease) {
        history.record(release.version)
        defaults.set(history.versions, forKey: "updateOfferedVersions")
    }

    func dismissOffer() {
        if let offer { markOfferPresented(offer) }
        offer = nil
    }

    /// Manual checks remain available even with automatic checks disabled.
    @discardableResult
    func check(force: Bool = false) async -> Bool {
        guard active, !checking, force || schedule.due(enabled: enabled, now: now()) else { return false }
        checking = true
        statusKey = "update_checking"
        let id = UUID()
        requestID = id
        let task = Task { try await fetch() }
        request = task
        defer {
            if requestID == id {
                request = nil
                requestID = nil
                checking = false
            }
        }
        do {
            let fetched = try await task.value
            guard requestID == id, active, !Task.isCancelled else { return false }
            release = fetched
            if let offer, offer.version != fetched.version { self.offer = nil }
            defaults.set(fetched.metadata, forKey: "updateManifest")
            schedule.succeeded(at: now())
            saveSchedule()
            refreshFreshness()
            statusKey = available == nil ? "update_current" : "update_ready"
            // Opening a store or returning from it never changes the installed
            // version: only the next running app bundle can confirm an update.
            if available == nil { handoffStatus = nil; offer = nil }
            presentIfNeeded()
            return true
        } catch {
            guard requestID == id, active, !Task.isCancelled, !task.isCancelled else { return false }
            schedule.failed(at: now())
            saveSchedule()
            refreshFreshness()
            statusKey = "update_failed"
            offer = nil
            return false
        }
    }

    /// Revalidate stale metadata before sending the user to an external store.
    /// A different version needs a new explicit click after showing its notes.
    func prepareHandoff(for version: AppVersion) async -> Bool {
        refreshFreshness()
        if !isFresh {
            guard await check(force: true) else { return false }
        }
        guard let available, available.version == version else {
            statusKey = available == nil ? "update_current" : "update_changed"
            return false
        }
        return active
    }

    func recordHandoff(store: UpdateStore, opened: Bool) {
        handoffStatus = UpdateHandoffStatus(store: store, opened: opened)
        // Do not dismiss the persistent update or claim installation success.
    }

    private func startLoop() {
        guard active, enabled, loop == nil else { return }
        loop = Task { [weak self] in
            do { try await Task.sleep(nanoseconds: UInt64(UpdateCheck.firstDelay * 1_000_000_000)) }
            catch { return }
            while !Task.isCancelled {
                guard let self, self.active, self.enabled else { return }
                self.refreshFreshness()
                await self.check()
                // Poll the clock once a minute; actual network checks remain
                // hourly on success and use persisted exponential backoff.
                do { try await Task.sleep(nanoseconds: 60 * 1_000_000_000) }
                catch { return }
            }
        }
    }

    private func stopTasks() {
        loop?.cancel()
        loop = nil
        request?.cancel()
        request = nil
        requestID = nil
        checking = false
        if statusKey == "update_checking" { statusKey = nil }
    }

    private func refreshFreshness() {
        isFresh = release != nil && schedule.isFresh(now: now())
    }

    private func presentIfNeeded() {
        refreshFreshness()
        guard presentationAllowed, offer == nil, let available,
              history.shouldOffer(available.version, current: current, enabled: enabled, active: active, fresh: isFresh) else { return }
        offer = available
    }

    private func saveSchedule() {
        if let data = try? JSONEncoder().encode(schedule) { defaults.set(data, forKey: "updateSchedule") }
    }
}

struct UpdateHandoffStatus {
    let store: UpdateStore
    let opened: Bool
}
