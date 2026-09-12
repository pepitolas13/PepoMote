import SwiftUI

/// Mando apaisado estilo "de lado" (NES): cruceta a la izquierda, 1 y 2
/// grandes a la derecha. Para juegos 2D en Dolphin con el Wiimote de lado.
/// Dentro de Wii U como Mando de Wii: cabecera «Wii U · Mando de Wii» con
/// «Teclado» y el selector «En Cemu soy» debajo.
struct ControllerLandscapeScreen: View {
    let showChips: Bool
    let onDisconnect: () -> Void
    @ObservedObject var link = LinkState.shared
    @State private var keyboardOpen = false

    var body: some View {
        GeometryReader { geo in
            ZStack {
                // Cabecera compacta (+ selector de mando dentro de Wii U). Si no
                // cabe todo, cae primero el nombre del PC; «Salir» nunca
                VStack(spacing: 2) {
                    HStack(spacing: 14) {
                        switch link.link {
                        case .reconnecting(let pc, _):
                            ReconnectingLabel(pcName: pc, font: PepoFont.bodyMedium()).layoutPriority(1)
                        case .connected(let c):
                            Text(c.pcName).pepoBody().lineLimit(1).frame(maxWidth: 160).layoutPriority(0)
                            if isWiiUAsWiimote(c) {
                                Text(tr("wiiu_as_wiimote")).pepoBody().lineLimit(1).layoutPriority(1)
                            }
                            if showModeChips(c, showChips) {
                                ModeChips(current: c.mode, supportsCemu: c.supportsCemu, compact: true).fixedSize().layoutPriority(2)
                            }
                            if c.mode == LinkState.modeCemu {
                                KeyboardButton(compact: true) { keyboardOpen = true }.fixedSize().layoutPriority(3)
                            }
                        case .connecting:
                            Text(tr("status_connecting")).pepoBody().lineLimit(1).layoutPriority(1)
                        default:
                            Text(tr("status_disconnected")).pepoBody().lineLimit(1).layoutPriority(1)
                        }
                        TextLink(title: tr("exit"), color: Pepo.error, action: onDisconnect).fixedSize().layoutPriority(4)
                    }
                    if let c = link.link.connected, isWiiUAsWiimote(c) {
                        PadSelector(link: c, width: geo.size.width, compact: true)
                    }
                }
                .padding(.top, 6)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)

                // Cruceta izquierda
                HStack {
                    PadCross(size: 190).padding(.leading, 34)
                    Spacer()
                }

                // − / + / A centro
                VStack(spacing: 12) {
                    Spacer().frame(height: 20)
                    HStack(spacing: 16) {
                        RoundButton(label: "−", size: 53, bit: Btn.minus, textSize: 19)
                        RoundButton(label: "+", size: 53, bit: Btn.plus, textSize: 19)
                    }
                    RoundButton(label: "A", size: 62, bit: Btn.a, textSize: 22)
                }

                // 1 y 2 grandes a la derecha (los botones de acción del modo NES)
                HStack(spacing: 18) {
                    Spacer()
                    RoundButton(label: "1", size: 92, bit: Btn.one, background: Pepo.blue, pressedColor: Pepo.blueHover, textColor: Pepo.onAccent, textSize: 28)
                    RoundButton(label: "2", size: 92, bit: Btn.two, background: Pepo.blue, pressedColor: Pepo.blueHover, textColor: Pepo.onAccent, textSize: 28, pop: true)
                }
                .padding(.trailing, 30)

                VStack {
                    Spacer()
                    Text(tr("rotate_hint")).pepoBody().lineLimit(1).padding(.bottom, 8)
                }

                VStack {
                    Spacer()
                    HStack {
                        PrecisionPill().padding(.leading, 16).padding(.bottom, 6)
                        Spacer()
                    }
                }

                VStack {
                    NoticeBanner().padding(.top, 84)
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
}
