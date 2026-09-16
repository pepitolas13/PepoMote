import SwiftUI
import UIKit

/// Medidas (pt) del mando de Wii con Nunchuk en un solo móvil, apaisado. `s`
/// es la escala de iPad (`UiScale.landscapeBase`; 1 en cualquier iPhone). El
/// stick es lo más grande que dejan la altura bajo las pastillas Z/C, su
/// columna (40 % del ancho) y el hueco que necesita la columna central entre
/// él y la cruceta + A de la derecha. Mismos números que Android
/// (WiiNunchukMetrics.kt).
struct WiiNunchukMetrics {
    let s: CGFloat
    /// Cabecera compacta con su margen.
    let headerH: CGFloat = 44
    let margin: CGFloat
    let gap: CGFloat
    let pillH: CGFloat
    let zW: CGFloat
    let cW: CGFloat
    let bW: CGFloat
    let small: CGFloat
    let recenter: CGFloat
    let rowGap: CGFloat
    let bodyH: CGFloat
    let cross: CGFloat
    let a: CGFloat
    /// Lo que necesita la columna central (tres botones y sus huecos).
    let centerMin: CGFloat
    let stick: CGFloat

    init(size: CGSize) {
        s = UiScale.factor(size, base: UiScale.landscapeBase)
        margin = 8 * s
        gap = 8 * s
        pillH = 40 * s
        zW = 120 * s
        cW = 80 * s
        bW = 150 * s
        small = 44 * s
        recenter = 54 * s
        rowGap = 12 * s
        bodyH = size.height - 44
        cross = Swift.min(0.45 * bodyH, 150 * s)
        a = Swift.min(0.45 * bodyH, 148 * s)
        centerMin = small * 3 + rowGap * 2 + gap * 2
        stick = Swift.max(
            Swift.min(
                bodyH - pillH - gap - margin * 2,
                0.40 * size.width - margin * 2,
                size.width - margin * 2 - cross - gap - a - centerMin,
                280 * s
            ),
            120
        )
    }

    /// Tamaño de texto: crece con el iPad.
    func text(_ base: CGFloat) -> CGFloat { base * s }
}

/// Mando de Wii + Nunchuk en un solo móvil (modo Dolphin, apaisado, a dos
/// manos): a la izquierda Z y C arriba (donde el GamePad tiene L/ZL) y el
/// stick del Nunchuk; en el centro −, diana, +, 1, Home, 2; a la derecha B
/// arriba (el gatillo, bajo el índice), la cruceta y la A grande. El puntero
/// lo calcula Dolphin con los sensores, remapeados al marco apaisado como en
/// el GamePad: se apunta con el móvil de lado, y agitar es agitar. Solo se
/// enseña cuando el receptor ha confirmado el Nunchuk propio (`ownNunchuk`);
/// en vertical vuelve el mando de siempre.
struct WiimoteNunchukScreen: View {
    let showChips: Bool
    let onDisconnect: () -> Void
    @ObservedObject var link = LinkState.shared
    @EnvironmentObject var model: AppModel
    @State private var rotation: Int = OrientationLock.frameRotation(OrientationLock.current)

    var body: some View {
        GeometryReader { geo in
            let m = WiiNunchukMetrics(size: geo.size)
            ZStack {
                // Cabecera compacta: PC · «Dolphin · Nunchuk» · chips · Salir
                VStack(spacing: 2) {
                    HStack(spacing: 14) {
                        switch link.link {
                        case .reconnecting(let pc, _):
                            ReconnectingLabel(pcName: pc, font: PepoFont.bodyMedium()).layoutPriority(1)
                        case .connected(let c):
                            if geo.size.width >= 850 {
                                Text(c.pcName).pepoBody().lineLimit(1).frame(maxWidth: 160).layoutPriority(0)
                            }
                            if geo.size.width >= 1000 || !showModeChips(c, showChips) {
                                Text(tr("mode_dolphin_nunchuk")).pepoBody().lineLimit(1).layoutPriority(1)
                            }
                            HStack(spacing: 6) {
                                if showModeChips(c, showChips) {
                                    ModeChips(current: c.mode, supportsCemu: c.supportsCemu, supportsSwitch: c.supportsSwitch, compact: true)
                                }
                                NunchukChip(link: c, compact: true)
                            }
                            .layoutPriority(2)
                        case .connecting:
                            Text(tr("status_connecting")).pepoBody().lineLimit(1).layoutPriority(1)
                        default:
                            Text(tr("status_disconnected")).pepoBody().lineLimit(1).layoutPriority(1)
                        }
                        TextLink(title: tr("exit"), color: Pepo.error, action: onDisconnect).fixedSize().layoutPriority(4)
                    }
                }
                .padding(.top, 6)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)

                // Izquierda: Z y C arriba (bajo el índice), el stick del Nunchuk abajo (pulgar)
                HStack(spacing: m.gap) {
                    ShoulderButton(label: "Z", bit: Btn.z, width: m.zW, height: m.pillH, textSize: m.text(16))
                    ShoulderButton(label: "C", bit: Btn.c, width: m.cW, height: m.pillH, textSize: m.text(16))
                }
                .padding(.leading, m.margin)
                .padding(.top, m.headerH)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

                AnalogStick(size: m.stick) { x, y in ButtonState.shared.setStick(x, y) }
                    .padding(.leading, m.margin)
                    .padding(.bottom, m.margin)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)

                // Centro: − diana + y 1 Home 2
                VStack(spacing: m.rowGap) {
                    HStack(spacing: m.rowGap) {
                        RoundButton(label: "−", size: m.small, bit: Btn.minus, textSize: m.text(19))
                        RecenterButton(size: m.recenter)
                        RoundButton(label: "+", size: m.small, bit: Btn.plus, textSize: m.text(19))
                    }
                    HStack(spacing: m.rowGap) {
                        RoundButton(label: "1", size: m.small, bit: Btn.one, textSize: m.text(18))
                        RoundButton(label: tr("home_btn"), size: m.small, bit: Btn.home, textSize: m.text(12))
                        RoundButton(label: "2", size: m.small, bit: Btn.two, textSize: m.text(18))
                    }
                    // Acercar el mando a la pantalla (mantener): juegos que lo piden
                    NearPill()
                }

                // Derecha: B arriba (el gatillo, bajo el índice); cruceta y A grande abajo (pulgar)
                ShoulderButton(label: "B", bit: Btn.b, width: m.bW, height: m.pillH, textSize: m.text(16))
                    .padding(.trailing, m.margin)
                    .padding(.top, m.headerH)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)

                HStack(spacing: m.gap) {
                    PadCross(size: m.cross)
                    RoundButton(
                        label: "A", size: m.a, bit: Btn.a,
                        background: Pepo.blue, pressedColor: Pepo.blueHover, textColor: Pepo.onAccent,
                        textSize: m.text(44), pop: true
                    )
                }
                .padding(.trailing, m.margin)
                .padding(.bottom, m.margin)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)

                // Primera vez: el lado del apaisado lo ha elegido iOS; se pregunta
                // si es el bueno y se guarda para siempre (Ajustes lo cambia)
                if model.nunchukSide == .unset {
                    let shown = LandscapeSide.effective(saved: LandscapeSide.current(OrientationLock.current), provisional: model.nunchukSideProvisional)
                    VStack {
                        SideAskCard(
                            title: tr("side_ask"),
                            onFlip: { model.nunchukSideProvisional = shown.flipped },
                            onKeep: { model.saveNunchukSide(shown) }
                        )
                        .padding(.top, m.headerH + 4)
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
        // Trama con stick + C/Z y los sensores remapeados al marco apaisado
        .onChange(of: rotation) { _ in applyEngine() }
        .onReceive(NotificationCenter.default.publisher(for: UIDevice.orientationDidChangeNotification)) { _ in
            // El móvil puede girar 180° entre los dos apaisados: el remapeo lo sigue
            rotation = OrientationLock.frameRotation(OrientationLock.current)
        }
        .onAppear {
            UIApplication.shared.isIdleTimerDisabled = true
            UIDevice.current.beginGeneratingDeviceOrientationNotifications()
            rotation = OrientationLock.frameRotation(OrientationLock.current)
            applyEngine()
        }
        .onDisappear {
            // Vertical o Salir: el Wiimote de siempre, y nada queda pulsado ni
            // inclinado; la prueba del lado sin confirmar se olvida
            model.nunchukSideProvisional = nil
            UIApplication.shared.isIdleTimerDisabled = false
            if let engine = link.motion {
                engine.kind = .wiimote
                engine.rotation = Frame.rotation0
            }
            ButtonState.shared.reset()
        }
    }

    private func applyEngine() {
        guard let engine = link.motion else { return }
        engine.rotation = rotation
        engine.kind = .wiiNunchuk
    }
}

/// Pregunta de la primera vez de un mando apaisado fijo (mando + Nunchuk,
/// GamePad): ¿está bien así? «Darle la vuelta» lo gira 180° y «Así lo quiero»
/// guarda ese lado para siempre.
struct SideAskCard: View {
    let title: String
    let onFlip: () -> Void
    let onKeep: () -> Void

    var body: some View {
        VStack(spacing: 6) {
            Text(title).pepoTitle().multilineTextAlignment(.center)
            Text(tr("side_ask_sub")).pepoBody().multilineTextAlignment(.center)
            HStack(spacing: 10) {
                ModeChip(label: tr("side_flip"), selected: false, action: onFlip)
                ModeChip(label: tr("side_keep"), selected: true, action: onKeep)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .frame(maxWidth: 340)
        .background(Pepo.card)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Pepo.blue, lineWidth: 1.5))
    }
}

/// Chip «Nunchuk» (modo Dolphin): el mando lleva su propio Nunchuk (con el
/// móvil de lado: stick, C y Z). Cambia el ajuste y se lo pide al receptor;
/// marcado solo cuando el receptor lo ha confirmado. Cambiarlo exige reabrir
/// Dolphin (el receptor lo avisa).
struct NunchukChip: View {
    let link: ConnectedLink
    var compact = false
    var dense = false

    var body: some View {
        ModeChip(label: tr("nunchuk_chip"), selected: link.ownNunchuk, compact: compact, dense: dense, toggle: true) {
            let want = !link.ownNunchuk
            AppPrefs.ownNunchuk = want
            LinkState.shared.sendNunchuk?(want)
        }
    }
}

/// Chip «Nunchuk»: en Dolphin, cualquier jugador que sea mando (no un Nunchuk).
func showNunchukChip(_ c: ConnectedLink) -> Bool {
    c.mode == LinkState.modeDolphin && c.role == LinkState.roleWiimote
}
