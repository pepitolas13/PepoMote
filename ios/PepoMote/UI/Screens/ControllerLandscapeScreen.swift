import SwiftUI

/// Medidas del mando apaisado (NES): en un iPad todo crece a la vez
/// (UiScale, por la anchura natural del trazado); en cualquier iPhone, las
/// de siempre.
struct LandscapeMetrics {
    let s: CGFloat
    let cross: CGFloat
    let small: CGFloat
    /// Diana de recentrado entre − y + (como en el mando + Nunchuk).
    let recenter: CGFloat
    let a: CGFloat
    /// Home, pequeño, a la derecha de A (fuera del modo puntero).
    let home: CGFloat
    let big: CGFloat
    let crossInset: CGFloat
    let bigInset: CGFloat

    init(size: CGSize) {
        s = UiScale.factor(size, base: UiScale.landscapeBase)
        cross = 190 * s
        small = 53 * s
        recenter = 54 * s
        a = 62 * s
        home = 40 * s
        big = 92 * s
        crossInset = 34 * s
        bigInset = 30 * s
    }

    func text(_ base: CGFloat) -> CGFloat { base * s }
    func gap(_ base: CGFloat) -> CGFloat { base * s }
}

/// Mando apaisado estilo "de lado" (NES): cruceta a la izquierda, 1 y 2
/// grandes a la derecha. Para juegos 2D en Dolphin con el Wiimote de lado.
/// Arriba, la cabecera plegable: una pastilla con el modo y, desplegada, la
/// tarjeta con el PC, «Teclado», los chips, el selector «En Cemu soy» (dentro
/// de Wii U como Mando de Wii) y «Salir».
struct ControllerLandscapeScreen: View {
    let showChips: Bool
    let onDisconnect: () -> Void
    @ObservedObject var link = LinkState.shared
    @State private var keyboardOpen = false
    /// Home del Mando de Wii: conectado y fuera del modo puntero.
    private var showHome: Bool { link.link.connected.map(showHomeButton) ?? false }
    /// RetroArch como mando de NES: 1 y 2 son B y A del RetroPad, la A grande
    /// es X y Home abre el menú de RetroArch (el receptor los mapea).
    private var retro: Bool { Route.isRetroArch(link.link) }
    @State private var rotation: Int = OrientationLock.frameRotation(OrientationLock.current)
    /// Dónde está cada botón, para «Pulsar deslizando».
    @StateObject private var press = PressRegistry()

    var body: some View {
        GeometryReader { geo in
            let m = LandscapeMetrics(size: geo.size)
            ZStack {
                // El primero: recoge los dedos que nacen fuera de los botones
                SlideCanvas()

                // Cruceta izquierda
                HStack {
                    // En un juego, la cruceta de un mando girado (IR a la izquierda)
                    PadCross(size: m.cross, sideways: Route.sidewaysDpad(link.link)).padding(.leading, m.crossInset)
                    Spacer()
                }

                // − / ◎ / + y A en el centro: la diana (mantener) recentra
                // el cursor o el puntero IR, que de lado también se apunta.
                // Home, pequeño, a la derecha de A y fuera del modo puntero
                // (el PC no le da uso): como en el Mando de Wii de lado, donde
                // −/Home/+ quedan entre A y 1/2; un hueco igual a la izquierda
                // deja A centrada bajo la diana
                VStack(spacing: m.gap(12)) {
                    Spacer().frame(height: m.gap(20))
                    HStack(spacing: m.gap(16)) {
                        RoundButton(label: "−", size: m.small, bit: Btn.minus, textSize: m.text(19))
                        RecenterButton(size: m.recenter)
                        RoundButton(label: "+", size: m.small, bit: Btn.plus, textSize: m.text(19))
                    }
                    if showHome {
                        HStack(spacing: m.gap(14)) {
                            Color.clear.frame(width: m.home, height: m.home)
                            RoundButton(label: retro ? "X" : "A", size: m.a, bit: Btn.a, textSize: m.text(22))
                            RoundButton(label: tr(retro ? "retro_menu" : "home_btn"), size: m.home, bit: Btn.home, textSize: m.text(retro ? 10 : 11))
                        }
                    } else {
                        RoundButton(label: retro ? "X" : "A", size: m.a, bit: Btn.a, textSize: m.text(22))
                    }
                }

                // 1 y 2 grandes a la derecha (los botones de acción del modo NES)
                HStack(spacing: m.gap(18)) {
                    Spacer()
                    RoundButton(label: retro ? "B" : "1", size: m.big, bit: Btn.one, background: Pepo.blue, pressedColor: Pepo.blueHover, textColor: Pepo.onAccent, textSize: m.text(28))
                    RoundButton(label: retro ? "A" : "2", size: m.big, bit: Btn.two, background: Pepo.blue, pressedColor: Pepo.blueHover, textColor: Pepo.onAccent, textSize: m.text(28), pop: true)
                }
                .padding(.trailing, m.bigInset)

                VStack {
                    Spacer()
                    Text(tr("rotate_hint")).pepoBody().lineLimit(1).padding(.bottom, 8)
                }

                VStack {
                    Spacer()
                    HStack {
                        // En Dolphin la precisión no hace nada: la píldora es «Acercar»
                        if link.link.connected?.mode == LinkState.modeDolphin {
                            NearPill().padding(.leading, 16).padding(.bottom, 6)
                        } else {
                            PrecisionPill().padding(.leading, 16).padding(.bottom, 6)
                        }
                        Spacer()
                    }
                }

                VStack {
                    NoticeBanner().padding(.top, 84)
                    Spacer()
                }
            }
            .frame(width: geo.size.width, height: geo.size.height)
            // La última: la tarjeta desplegada gana a la cruceta y a 1/2
            .overlay(alignment: .top) { header }
        }
        .coordinateSpace(name: PressRegistry.padSpace)
        .environmentObject(press)
        .background(Pepo.background.ignoresSafeArea())
        .sheet(isPresented: $keyboardOpen) {
            KeyboardSheet { keyboardOpen = false }
        }
        // Móvil de lado: los sensores giran con el mando (Route.sidewaysRotation:
        // mando girado con el IR a la izquierda en un juego, o apuntando con el
        // borde largo en modo puntero); al salir, el mando vertical de siempre
        .onChange(of: rotation) { _ in applyEngine() }
        .onChange(of: link.link) { _ in applyEngine() }
        .onReceive(NotificationCenter.default.publisher(for: UIDevice.orientationDidChangeNotification)) { _ in
            rotation = OrientationLock.frameRotation(OrientationLock.current)
        }
        .onAppear {
            UIApplication.shared.isIdleTimerDisabled = true
            UIDevice.current.beginGeneratingDeviceOrientationNotifications()
            rotation = OrientationLock.frameRotation(OrientationLock.current)
            applyEngine()
        }
        .onDisappear {
            UIApplication.shared.isIdleTimerDisabled = false
            link.motion?.rotation = Frame.rotation0
            press.releaseAll() // nada queda pulsado al salir
            ButtonState.shared.reset()
        }
    }

    /// Cabecera plegable: la pastilla con el modo y, desplegada, el PC, el
    /// Teclado, los chips, el selector «En Cemu soy» y «Salir». Ya no hay
    /// puertas de anchura: en la tarjeta cabe todo en cualquier móvil.
    private var header: some View {
        CollapsibleHeader(handleLabel: headerLabel, connected: link.link.connected != nil) {
            VStack(spacing: 8) {
                HStack(spacing: 20) {
                    if let c = link.link.connected {
                        Text(c.pcName).pepoBody().lineLimit(1).frame(maxWidth: 160)
                        if c.hasKeyboard && (c.mode == LinkState.modeCemu || c.mode == LinkState.modeRetroArch) {
                            HeaderKeyboardButton { keyboardOpen = true }
                        }
                    }
                    // Reconectando, el punto latiendo: es la única señal de que
                    // el servicio está rehaciendo la sesión (como en Android)
                    if case .reconnecting(let pc, _) = link.link {
                        ReconnectingLabel(pcName: pc, font: PepoFont.bodyMedium())
                    }
                    TextLink(title: tr("exit"), color: Pepo.error, action: onDisconnect).fixedSize()
                }
                if let c = link.link.connected {
                    if showModeChips(c, showChips) {
                        ModeChips(current: c.mode, supportsCemu: c.supportsCemu, supportsSwitch: c.supportsSwitch, supportsRetroArch: c.supportsRetroArch, supportsPointer: c.receiver.supportsPointer, compact: true)
                    }
                    // En Dolphin, el chip «Nunchuk» (aquí apagado: encenderlo
                    // cambia este NES por el mando + Nunchuk)
                    if showNunchukChip(c) { NunchukChip(link: c, compact: true) }
                    if isWiiUAsWiimote(c) || Route.isRetroArch(link.link) {
                        PadSelector(link: c, width: HeaderCollapse.cardWidth - 40, compact: true)
                    }
                    if Route.isRetroArch(link.link) {
                        RetroArchHotkeys(compact: true)
                    }
                }
            }
        }
    }

    /// Texto de la pastilla: lo mismo que decía la cabecera de siempre.
    private var headerLabel: String {
        switch link.link {
        case .reconnecting(let pc, _): return tr("status_reconnecting", pc)
        case .connected(let c): return isWiiUAsWiimote(c) ? tr("wiiu_as_wiimote") : modeLabel(c.mode)
        case .connecting: return tr("status_connecting")
        default: return tr("status_disconnected")
        }
    }

    private func applyEngine() {
        link.motion?.rotation = Route.sidewaysRotation(link.link, displayRotation: rotation)
    }
}

/// «Teclado» dentro de la cabecera plegable: al abrirlo, la tarjeta se pliega
/// (el teclado tapa la pantalla y al volver el mando queda despejado).
private struct HeaderKeyboardButton: View {
    @Environment(\.headerCollapse) private var header
    let onOpen: () -> Void

    var body: some View {
        KeyboardButton(compact: true) {
            header.collapse()
            onOpen()
        }
        .fixedSize()
    }
}
