import SwiftUI

struct SettingsScreen: View {
    @EnvironmentObject var model: AppModel
    @State private var sounds = AppPrefs.soundsEnabled
    @State private var dolphinChips = AppPrefs.showDolphinChips
    @State private var noScreen = AppPrefs.gamePadNoScreen
    @State private var fullScreen = AppPrefs.gamePadFullScreen
    @State private var fullScreenKb = AppPrefs.gamePadFullScreenKeyboard
    @State private var ownNunchuk = AppPrefs.ownNunchuk
    @State private var notices = AppPrefs.receiverNotices
    @State private var updateCheck = AppPrefs.updateCheckEnabled

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
                    // Nunchuk en el mismo móvil (Dolphin): stick, C y Z con el móvil de lado
                    SettingRow(title: tr("nunchuk_own_title"), subtitle: tr("nunchuk_own_sub"), on: $ownNunchuk)
                        .onChange(of: ownNunchuk) {
                            AppPrefs.ownNunchuk = $0
                            // Con el enlace vivo se aplica ya (el receptor lo confirma con el eco)
                            LinkState.shared.sendNunchuk?($0)
                        }
                    // GamePad de Wii U sin pantalla táctil: botones más grandes
                    SettingRow(title: tr("noscreen_title"), subtitle: tr("noscreen_sub"), on: $noScreen)
                        .onChange(of: noScreen) {
                            AppPrefs.gamePadNoScreen = $0
                            if $0 { fullScreen = false }
                        }
                    // Pantalla del GamePad a pantalla completa (mando real en el PC),
                    // con la subopción del botón de teclado
                    SettingRowWithSub(
                        title: tr("fullscreen_title"), subtitle: tr("fullscreen_sub"), on: $fullScreen,
                        subTitle: tr("fullscreen_kb_title"), subSubtitle: tr("fullscreen_kb_sub"), subOn: $fullScreenKb
                    )
                    .onChange(of: fullScreen) {
                        AppPrefs.gamePadFullScreen = $0
                        if $0 { noScreen = false }
                        // Con el enlace vivo se aplica ya (el receptor lo confirma con el eco)
                        LinkState.shared.sendScreenOnly?($0)
                    }
                    .onChange(of: fullScreenKb) { AppPrefs.gamePadFullScreenKeyboard = $0 }
                    // Avisos del receptor sobre el mando al cambiar de modo
                    SettingRow(title: tr("notices_title"), subtitle: tr("notices_sub"), on: $notices)
                        .onChange(of: notices) { AppPrefs.receiverNotices = $0 }
                    // Aviso de versión nueva: la única consulta fuera de la red local
                    SettingRow(title: tr("update_title"), subtitle: tr("update_sub"), on: $updateCheck)
                        .onChange(of: updateCheck) { AppPrefs.updateCheckEnabled = $0 }
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
            Text(tr("about", UpdateCheck.current?.description ?? "?")).pepoBody().frame(maxWidth: .infinity)
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

/// Fila con una subopción debajo, atenuada y bloqueada mientras la principal
/// está apagada.
private struct SettingRowWithSub: View {
    let title: String
    let subtitle: String
    @Binding var on: Bool
    let subTitle: String
    let subSubtitle: String
    @Binding var subOn: Bool

    var body: some View {
        VStack(spacing: 0) {
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
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(subTitle).pepoTitle()
                    Text(subSubtitle).pepoBody()
                }
                Spacer()
                Toggle("", isOn: $subOn)
                    .labelsHidden()
                    .tint(Pepo.blue)
                    .disabled(!on)
            }
            .padding(.leading, 30)
            .padding(.trailing, 18)
            .padding(.bottom, 16)
            .opacity(on ? 1 : 0.5)
        }
        .frame(maxWidth: .infinity)
        .pepoCard()
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
