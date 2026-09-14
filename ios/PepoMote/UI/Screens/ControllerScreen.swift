import SwiftUI

/// Medidas del mando vertical para un tamaño de pantalla. En un iPhone bajo
/// (SE) todo se encoge a la vez (`s` < 1); en un iPad todo crece a la vez
/// (`grow` > 1, UiScale: lo que limita es la anchura o la altura que queda
/// para lo que escala, no la altura de un iPhone) y los huecos se vuelven
/// flexibles para repartir la holgura vertical entre todos en vez de dejarla
/// en uno solo. En cualquier otro iPhone, las medidas de siempre.
struct RemoteMetrics {
    let grow: CGFloat
    let s: CGFloat
    /// Anchura de la columna del mando; las tiras la abrazan (en un iPad no
    /// se van a los bordes de la pantalla).
    let colW: CGFloat
    let flexible: Bool
    let cross: CGFloat
    let small: CGFloat
    let recenter: CGFloat
    let big: CGFloat
    let one: CGFloat
    let media: CGFloat
    let trigger: CGFloat
    let spacing: CGFloat
    let stripH: CGFloat
    let precisionW: CGFloat
    let scrollW: CGFloat

    init(size: CGSize) {
        grow = UiScale.factor(size, base: UiScale.remoteBase, fixed: UiScale.remoteFixed)
        s = Swift.min(1, size.height / 780) * grow
        colW = Swift.min(size.width, 520 * grow)
        flexible = grow > 1
        cross = 168 * s
        small = 54 * s
        recenter = 64 * s
        big = 148 * s
        one = 52 * s
        media = 46 * s
        trigger = 88 * s
        spacing = 20 * s
        stripH = size.height * 0.45
        precisionW = 34 * grow
        scrollW = 24 * grow
    }

    /// Texto: crece con el iPad; en el SE no se encoge (como hasta ahora).
    func text(_ base: CGFloat) -> CGFloat { base * grow }
    /// Hueco entre grupos, escalado como los botones.
    func gap(_ base: CGFloat) -> CGFloat { base * s }
}

/// Hueco entre grupos: fijo en el móvil; flexible en el iPad (reparte la
/// holgura vertical con los demás huecos flexibles).
struct Gap: View {
    let h: CGFloat
    let flexible: Bool

    init(_ h: CGFloat, flexible: Bool) {
        self.h = h
        self.flexible = flexible
    }

    var body: some View {
        if flexible {
            Spacer(minLength: h)
        } else {
            Spacer().frame(height: h)
        }
    }
}

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
            let m = RemoteMetrics(size: geo.size)
            ZStack {
                // La columna del mando y las tiras comparten anchura
                ZStack {
                    column(m)
                        .padding(.horizontal, 24)
                    HStack {
                        PrecisionStrip(width: m.precisionW, height: m.stripH, glyph: m.text(16))
                            .padding(.leading, 6)
                        Spacer()
                        ScrollStrip(width: m.scrollW, height: m.stripH)
                            .padding(.trailing, 6)
                    }
                }
                .frame(width: m.colW)

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

    private func column(_ m: RemoteMetrics) -> some View {
        VStack(spacing: 0) {
            Spacer().frame(height: 10)
            header
            if let c = link.link.connected {
                if showModeChips(c, showChips) {
                    Spacer().frame(height: 6)
                    ModeChips(current: c.mode, supportsCemu: c.supportsCemu)
                }
                // Dolphin: el Nunchuk en el mismo móvil (gira el móvil para usarlo)
                if showNunchukChip(c) {
                    Spacer().frame(height: 6)
                    NunchukChip(link: c)
                }
                if isWiiUAsWiimote(c) {
                    Spacer().frame(height: 8)
                    PadSelector(link: c, width: m.colW - 48, help: tr("wii_pad_help"))
                }
            }
            Gap(m.gap(10), flexible: m.flexible)
            PadCross(size: m.cross)
            Gap(m.gap(16), flexible: m.flexible)
            HStack(spacing: m.spacing) {
                RoundButton(label: "−", size: m.small, bit: Btn.minus, textSize: m.text(20))
                RecenterButton(size: m.recenter)
                RoundButton(label: "+", size: m.small, bit: Btn.plus, textSize: m.text(20))
            }
            Gap(m.gap(16), flexible: m.flexible)
            RoundButton(
                label: "A", size: m.big, bit: Btn.a,
                background: Pepo.blue, pressedColor: Pepo.blueHover, textColor: Pepo.onAccent,
                textSize: m.text(44), pop: true
            )
            Gap(m.gap(14), flexible: m.flexible)
            HStack(spacing: m.spacing) {
                RoundButton(label: "1", size: m.one, bit: Btn.one, textSize: m.text(18))
                RoundButton(label: "2", size: m.one, bit: Btn.two, textSize: m.text(18))
            }
            Gap(m.gap(10), flexible: m.flexible)
            MediaRow(buttonSize: m.media, textSize: m.text(16))
            Spacer(minLength: 4)
            TriggerZone(height: m.trigger)
            Spacer().frame(height: 12)
        }
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
                        if c.mode == LinkState.modeDolphin { return tr(c.ownNunchuk ? "mode_dolphin_nunchuk" : "mode_dolphin") }
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
