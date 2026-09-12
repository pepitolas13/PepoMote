import SwiftUI

struct SettingsScreen: View {
    @EnvironmentObject var model: AppModel
    @State private var sounds = AppPrefs.soundsEnabled
    @State private var dolphinChips = AppPrefs.showDolphinChips

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Spacer().frame(height: 24)
            Text(tr("channel_settings")).font(PepoFont.headline()).foregroundColor(Pepo.text)
            Spacer().frame(height: 20)
            ScrollView(showsIndicators: false) {
                VStack(spacing: 14) {
                    SettingRow(title: tr("sounds_title"), subtitle: tr("sounds_sub"), on: $sounds)
                        .onChange(of: sounds) { AppPrefs.soundsEnabled = $0 }
                    SettingRow(title: tr("chips_title"), subtitle: tr("chips_sub"), on: $dolphinChips)
                        .onChange(of: dolphinChips) { AppPrefs.showDolphinChips = $0 }
                    Button(action: {
                        model.linkRole = LinkState.roleWiimote // QR desde Ajustes: mando
                        LinkState.shared.pendingMode = nil
                        model.openPair()
                    }) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(tr("link_other_pc")).pepoTitle()
                            Text(tr("link_other_pc_sub")).pepoBody()
                        }
                        .padding(18)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .pepoCard()
                    }
                    .buttonStyle(.plain)
                    Text(tr("sens_note")).pepoBody().frame(maxWidth: .infinity, alignment: .leading)
                    Text(tr("ios_note")).pepoBody().frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            Spacer(minLength: 8)
            Text(tr("about")).pepoBody().frame(maxWidth: .infinity)
            TextLink(title: tr("back")) { model.screen = .home }
                .frame(maxWidth: .infinity)
            Spacer().frame(height: 16)
        }
        .padding(.horizontal, 20)
        .frame(maxWidth: 620)
        .frame(maxWidth: .infinity)
        .background(Pepo.background.ignoresSafeArea())
    }
}

private struct SettingRow: View {
    let title: String
    let subtitle: String
    @Binding var on: Bool

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).pepoTitle()
                Text(subtitle).pepoBody()
            }
            Spacer()
            Toggle("", isOn: $on)
                .labelsHidden()
                .tint(Pepo.blue)
        }
        .padding(18)
        .frame(maxWidth: .infinity)
        .pepoCard()
    }
}
