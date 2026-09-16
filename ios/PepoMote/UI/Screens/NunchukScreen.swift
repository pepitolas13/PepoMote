import SwiftUI

/// Nunchuk: el móvil de la otra mano, en vertical o de lado (mismo orden,
/// solo escalado). Stick centrado; C es una banda ancha justo encima del
/// stick y Z la banda inferior. Sin selector de modo ni puntero: eso lo
/// decide el móvil-Wiimote; el modo real y el jugador van en la cabecera.
struct NunchukScreen: View {
    let onDisconnect: () -> Void
    @EnvironmentObject var model: AppModel
    @ObservedObject var link = LinkState.shared
    /// Dónde está cada botón, para «Pulsar deslizando».
    @StateObject private var press = PressRegistry()

    var body: some View {
        GeometryReader { geo in
            // De lado hay poca altura: todo más pequeño, mismo orden. En un
            // iPad todo crece a la vez (UiScale; en cualquier iPhone, 1)
            let compact = geo.size.height < 520
            let grow: CGFloat = compact ? 1 : UiScale.factor(geo.size, base: UiScale.remoteBase, fixed: UiScale.remoteFixed)
            let stickSize = compact ? geo.size.height * 0.40 : min(geo.size.width * 0.66, 264 * grow)
            let cHeight: CGFloat = compact ? 48 : 64 * grow
            let zHeight: CGFloat = compact ? 64 : 96 * grow
            ZStack {
                // El primero: recoge los dedos que nacen fuera de los botones
                SlideCanvas()

                VStack(spacing: 0) {
                    Spacer().frame(height: 10)
                    header
                    // Aviso permanente: en Wii U el Nunchuk solo acompaña a un Mando de Wii
                    if let c = link.link.connected, c.mode == LinkState.modeCemu, c.pad != LinkState.padWiimote {
                        Spacer().frame(height: 6)
                        Text(tr("nunchuk_help"))
                            .font(PepoFont.bodyMedium())
                            .foregroundColor(Pepo.warn)
                            .multilineTextAlignment(.center)
                    }
                    Spacer()
                    // C: banda ancha encima del stick, se acierta con el pulgar sin mirar
                    TriggerZone(bit: Btn.c, label: "C", height: cHeight, background: Pepo.cardBorder, pressedColor: Pepo.glow, textColor: Pepo.text)
                    Spacer().frame(height: compact ? 10 : 18 * grow)
                    AnalogStick(size: stickSize) { x, y in ButtonState.shared.setStick(x, y) }
                    Spacer()
                    // Z: banda inferior, como el gatillo B del mando
                    TriggerZone(bit: Btn.z, label: "Z", height: zHeight)
                    Spacer().frame(height: 12)
                }
                .padding(.horizontal, 24)
                .frame(maxWidth: 520 * grow)

                VStack {
                    NoticeBanner().padding(.top, 64).padding(.horizontal, 24)
                    Spacer()
                }
            }
            .frame(width: geo.size.width, height: geo.size.height)
        }
        .coordinateSpace(name: PressRegistry.padSpace)
        .environmentObject(press)
        .background(Pepo.background.ignoresSafeArea())
        .onAppear { UIApplication.shared.isIdleTimerDisabled = true }
        .onDisappear {
            UIApplication.shared.isIdleTimerDisabled = false
            press.releaseAll() // nada queda pulsado…
            ButtonState.shared.setStick(0, 0) // …ni inclinado al salir
        }
    }

    /// Cabecera: PC · "Nunchuk · Jugador N · modo" (+ RTT) / Conectando… / Sin conexión, y Salir.
    private var header: some View {
        HStack(alignment: .center, spacing: 8) {
            VStack(alignment: .leading, spacing: 0) {
                switch link.link {
                case .connected(let c):
                    Text(c.pcName).pepoTitle().lineLimit(1)
                    let line = tr("nunchuk_line", c.player, modeLabel(c.mode))
                    Text(line + (c.rttMs.map { " · \(String(format: "%.0f", $0)) ms" } ?? "")).pepoBody().lineLimit(1)
                case .connecting:
                    Text(tr("status_connecting")).pepoTitle()
                case .reconnecting(let pc, _):
                    ReconnectingLabel(pcName: pc)
                default:
                    Text(tr("status_disconnected")).pepoTitle()
                    if PairStore.current() != nil {
                        TextLink(title: tr("reconnect"), color: Pepo.blue) { model.reconnect(role: LinkState.roleNunchuk) }
                    }
                }
            }
            .layoutPriority(0)
            Spacer(minLength: 4)
            TextLink(title: tr("exit"), color: Pepo.error, action: onDisconnect)
                .fixedSize()
                .layoutPriority(3)
        }
    }
}
