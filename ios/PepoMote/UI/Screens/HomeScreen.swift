import SwiftUI

/// Estado del enlace en el inicio: el punto (apagado, en marcha, encendido) y su texto.
enum HomeTone {
    case off, busy, on
}

struct HomeScreen: View {
    @EnvironmentObject var model: AppModel
    @EnvironmentObject var l10n: L10n
    @ObservedObject var link = LinkState.shared

    private var status: (HomeTone, String) {
        switch link.link {
        case .connected(let c): return (.on, tr("status_connected_to", c.pcName))
        case .connecting: return (.busy, tr("status_connecting"))
        case .reconnecting(let pc, _): return (.busy, tr("status_reconnecting", pc))
        default:
            if let pc = PairStore.current()?.pcName { return (.off, tr("status_disconnected_pc", pc)) }
            return (.off, tr("status_disconnected"))
        }
    }

    var body: some View {
        let (tone, text) = status
        return ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 0) {
                Spacer().frame(height: 28)
                HStack(alignment: .center) {
                    Text("PepoMote")
                        .font(PepoFont.display())
                        .foregroundColor(Pepo.text)
                    Spacer()
                    // Idioma: ES / EN (toca para cambiar)
                    Button(action: { l10n.toggle() }) {
                        Text(l10n.code.uppercased())
                            .font(PepoFont.bodyMedium())
                            .foregroundColor(Pepo.text)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 6)
                            .background(Pepo.card)
                            .clipShape(Capsule())
                            .overlay(Capsule().stroke(Pepo.cardBorder, lineWidth: 1.5))
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(tr("lang_switch_to", L10n.name(l10n.otherCode)))
                }
                Text(tr("home_subtitle")).pepoBody()
                Spacer().frame(height: 10)
                HStack(spacing: 8) {
                    switch tone {
                    case .on: Circle().fill(Pepo.ok).frame(width: 10, height: 10)
                    case .busy: PulsingDot(color: Pepo.warn)
                    case .off: Circle().fill(Pepo.textDim).frame(width: 10, height: 10)
                    }
                    Text(text).pepoBody().lineLimit(1)
                }
                Spacer().frame(height: 24)
                // Tres filas de pares: Conectar | Mando, Dolphin | Wii U, Nunchuk | Ajustes
                LazyVGrid(columns: [GridItem(.flexible(), spacing: 16), GridItem(.flexible(), spacing: 16)], spacing: 16) {
                    ChannelCard(title: tr("channel_connect"), subtitle: tr("channel_connect_sub"), glyph: .qr, accent: Pepo.blue) {
                        model.openController(mode: LinkState.modePointer, dolphinOnly: false)
                    }
                    ChannelCard(title: tr("channel_controller"), subtitle: tr("channel_controller_sub"), glyph: .pad, accent: Pepo.blue) {
                        model.openPad()
                    }
                    ChannelCard(title: tr("channel_dolphin"), subtitle: tr("channel_dolphin_sub"), glyph: .pointer, accent: Pepo.ok) {
                        model.openController(mode: LinkState.modeDolphin, dolphinOnly: true)
                    }
                    // Wii U: el móvil como GamePad (o Pro Controller) para Cemu
                    ChannelCard(title: tr("channel_wiiu"), subtitle: tr("channel_wiiu_sub"), glyph: .gamePad, accent: Pepo.ok) {
                        model.openController(mode: LinkState.modeCemu, dolphinOnly: false)
                    }
                    // Con Dolphin: el segundo móvil, en la otra mano
                    ChannelCard(title: tr("channel_nunchuk"), subtitle: tr("channel_nunchuk_sub"), glyph: .stick, accent: Pepo.ok) {
                        model.openNunchuk()
                    }
                    ChannelCard(title: tr("channel_settings"), subtitle: tr("channel_settings_sub"), glyph: .gear, accent: Pepo.textDim) {
                        model.screen = .settings
                    }
                }
                .frame(maxWidth: 620)
                Spacer().frame(height: 24)
            }
            .padding(.horizontal, 20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Pepo.background.ignoresSafeArea())
    }
}
