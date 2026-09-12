import SwiftUI

/// Mando vertical estilo Wiimote: cruceta, −/diana/+, A, 1/2, multimedia, B.
/// `showChips`: mostrar el selector Puntero/Dolphin/Wii U. Dentro de Wii U
/// como Mando de Wii, la cabecera lo dice, los chips se ven siempre (Jugador
/// 1) y debajo va el selector «En Cemu soy». En modo Wii U la cabecera lleva
/// además «Teclado». Las tiras: scroll a la derecha, precisión (mirilla) a la
/// izquierda, algo más ancha.
struct ControllerScreen: View {
    let showChips: Bool
    let onDisconnect: () -> Void
    @EnvironmentObject var model: AppModel
    @ObservedObject var link = LinkState.shared
    @State private var keyboardOpen = false

    var body: some View {
        GeometryReader { geo in
            // En pantallas bajas (iPhone SE) todo se encoge a la vez
            let s = min(1, geo.size.height / 780)
            ZStack {
                VStack(spacing: 0) {
                    Spacer().frame(height: 10)
                    header
                    if let c = link.link.connected {
                        if showModeChips(c, showChips) {
                            Spacer().frame(height: 6)
                            ModeChips(current: c.mode, supportsCemu: c.supportsCemu)
                        }
                        if isWiiUAsWiimote(c) {
                            Spacer().frame(height: 8)
                            PadSelector(link: c, width: geo.size.width - 48, help: tr("wii_pad_help"))
                        }
                    }
                    Spacer().frame(height: 10 * s)
                    PadCross(size: 168 * s)
                    Spacer().frame(height: 16 * s)
                    HStack(spacing: 20 * s) {
                        RoundButton(label: "−", size: 54 * s, bit: Btn.minus)
                        RecenterButton(size: 64 * s)
                        RoundButton(label: "+", size: 54 * s, bit: Btn.plus)
                    }
                    Spacer().frame(height: 16 * s)
                    RoundButton(
                        label: "A", size: 148 * s, bit: Btn.a,
                        background: Pepo.blue, pressedColor: Pepo.blueHover, textColor: Pepo.onAccent,
                        textSize: 44, pop: true
                    )
                    Spacer().frame(height: 14 * s)
                    HStack(spacing: 20 * s) {
                        RoundButton(label: "1", size: 52 * s, bit: Btn.one, textSize: 18)
                        RoundButton(label: "2", size: 52 * s, bit: Btn.two, textSize: 18)
                    }
                    Spacer().frame(height: 10 * s)
                    MediaRow(buttonSize: 46 * s)
                    Spacer(minLength: 4)
                    TriggerZone(height: 88 * s)
                    Spacer().frame(height: 12)
                }
                .padding(.horizontal, 24)
                .frame(maxWidth: 520)

                // Tira de scroll (derecha) y de precisión (izquierda, espejo, algo más ancha)
                HStack {
                    PrecisionStrip(width: 34, height: geo.size.height * 0.45)
                        .padding(.leading, 6)
                    Spacer()
                    ScrollStrip(width: 24, height: geo.size.height * 0.45)
                        .padding(.trailing, 6)
                }

                VStack {
                    NoticeBanner().padding(.top, 64).padding(.horizontal, 24)
                    Spacer()
                }
            }
            .frame(width: geo.size.width, height: geo.size.height)
        }
        .background(Pepo.background.ignoresSafeArea())
        .sheet(isPresented: $keyboardOpen) {
            KeyboardSheet { keyboardOpen = false }
        }
        .onAppear { UIApplication.shared.isIdleTimerDisabled = true }
        .onDisappear { UIApplication.shared.isIdleTimerDisabled = false }
    }

    /// Cabecera: estado + Teclado (Wii U) + Salir.
    private var header: some View {
        HStack(alignment: .center, spacing: 8) {
            VStack(alignment: .leading, spacing: 0) {
                switch link.link {
                case .connected(let c):
                    Text(c.slot > 0 ? tr("pc_player", c.pcName, c.slot + 1) : c.pcName)
                        .pepoTitle()
                        .lineLimit(1)
                    let modeText: String = {
                        if isWiiUAsWiimote(c) { return tr("wiiu_as_wiimote") }
                        if c.mode == LinkState.modeCemu { return tr("mode_wiiu") }
                        if c.mode == LinkState.modeDolphin { return tr("mode_dolphin") }
                        if c.slot > 0 { return tr("pointer_player1_points") }
                        return tr("mode_pointer")
                    }()
                    Text(modeText + (c.rttMs.map { " · \(String(format: "%.0f", $0)) ms" } ?? ""))
                        .pepoBody()
                        .lineLimit(1)
                case .connecting:
                    Text(tr("status_connecting")).pepoTitle()
                case .reconnecting(let pc, _):
                    ReconnectingLabel(pcName: pc)
                default:
                    Text(tr("status_disconnected")).pepoTitle()
                    if PairStore.current() != nil {
                        TextLink(title: tr("reconnect"), color: Pepo.blue) { model.reconnect(role: LinkState.roleWiimote) }
                    }
                }
            }
            .layoutPriority(0)
            Spacer(minLength: 4)
            // Modo Wii U: texto para el teclado en pantalla de Cemu
            if let c = link.link.connected, c.mode == LinkState.modeCemu {
                KeyboardButton(compact: true) { keyboardOpen = true }
                    .fixedSize()
                    .layoutPriority(2)
            }
            TextLink(title: tr("exit"), color: Pepo.error, action: onDisconnect)
                .fixedSize()
                .layoutPriority(3)
        }
    }
}
