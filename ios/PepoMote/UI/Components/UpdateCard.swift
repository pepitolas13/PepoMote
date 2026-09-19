import SwiftUI

/// Shared by the foreground offer, home and the persistent settings history.
@MainActor
struct UpdateCard: View {
    let release: UpdateRelease
    @ObservedObject private var updates = UpdateManager.shared
    @ObservedObject private var l10n = L10n.shared
    @Environment(\.openURL) private var openURL
    @State private var handingOff = false
    @State private var copied = false
    @State private var visible = false

    private var newer: Bool { updates.available?.version == release.version }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(tr(newer ? "update_available" : "update_notes_version", release.version.description)).pepoTitle()
            ForEach(Array(release.notes(language: l10n.code).enumerated()), id: \.offset) { _, note in
                // Manifest content stays literal text, never interpreted Markdown.
                Text(verbatim: "• " + note).pepoBody()
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            if !updates.isFresh { Text(tr("update_cached")).pepoBody() }
            if newer {
                Text(tr("update_ios_handoff")).pepoBody()
                ForEach([UpdateStore.sideStore, .altStore], id: \.self) { store in
                    Button(tr("update_open_store", store.name)) { handoff(to: store) }
                        .font(PepoFont.labelLarge())
                        .foregroundColor(Pepo.blue)
                        .disabled(handingOff || updates.checking)
                }
                if handingOff || updates.checking { ProgressView(tr("update_checking")) }
                if let status = updates.statusKey, ["update_failed", "update_changed"].contains(status) {
                    Text(tr(status)).pepoBody()
                }
                if let result = updates.handoffStatus {
                    Text(tr(result.opened ? "update_store_opened" : "update_store_missing", result.store.name)).pepoBody()
                    if !result.opened, result.store == .altStore || result.store == .altStoreLegacy {
                        Button(tr("update_open_altstore_legacy")) { handoff(to: .altStoreLegacy) }
                            .font(PepoFont.labelLarge())
                            .foregroundColor(Pepo.blue)
                            .disabled(handingOff || updates.checking)
                        Text(tr("update_altstore_legacy_help")).pepoBody()
                    }
                }
                Button(tr(copied ? "update_source_copied" : "update_copy_source")) {
                    UIPasteboard.general.string = UpdateCheck.sourceURL.absoluteString
                    copied = true
                }
                .font(PepoFont.labelLarge())
                .foregroundColor(Pepo.blue)
                Text(tr("update_store_steps")).pepoBody()
                Link(tr("update_install_guide"), destination: UpdateCheck.guideURL)
                    .font(PepoFont.labelLarge())
                    .foregroundColor(Pepo.blue)
            }
            Link(tr("update_download"), destination: release.releaseURL)
                .font(PepoFont.labelLarge())
                .foregroundColor(Pepo.blue)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .pepoCard(border: Pepo.blue, radius: 14)
        .onAppear { visible = true }
        .onDisappear { visible = false }
    }

    private func handoff(to store: UpdateStore) {
        handingOff = true
        Task { @MainActor in
            guard await updates.prepareHandoff(for: release.version) else {
                handingOff = false
                return
            }
            // The user can choose "Not now" or leave this screen while a
            // stale release is being checked. That cancels the store handoff.
            guard visible else { handingOff = false; return }
            openURL(store.sourceURL) { accepted in
                updates.recordHandoff(store: store, opened: accepted)
                handingOff = false
            }
        }
    }
}

@MainActor
struct UpdateOfferSheet: View {
    let release: UpdateRelease
    @ObservedObject private var updates = UpdateManager.shared

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                UpdateCard(release: release)
                Button(tr("update_dismiss")) { updates.dismissOffer() }
                    .font(PepoFont.labelLarge())
                    .foregroundColor(Pepo.blue)
                    .frame(maxWidth: .infinity)
            }
            .padding(20)
            .frame(maxWidth: 620)
            .frame(maxWidth: .infinity)
        }
        .background(Pepo.background.ignoresSafeArea())
    }
}
