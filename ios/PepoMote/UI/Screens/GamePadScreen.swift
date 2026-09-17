import SwiftUI
import UIKit

/// Medidas del GamePad para un tamaño de pantalla (todo escalado al tamaño
/// real, como en Android). En un iPad los topes (gatillos, botones) crecen
/// con `k` (UiScale); en cualquier iPhone `k` = 1. Stick y cruceta (o rombo)
/// llenan la columna lateral y lo que queda de altura bajo los gatillos.
///
/// Sin pantalla táctil (Pro Controller, o el ajuste «GamePad sin pantalla»)
/// el centro solo necesita la fila − · Home · +: se prueban dos trazados y
/// se queda el que dé pads más grandes. Apilado (columnas laterales anchas:
/// iPad) o en fila, con stick y cruceta uno al lado del otro y el centro como
/// columna vertical (móviles, que van justos de altura y sobrados de ancho).
/// Con pantalla, en un iPhone todo mide lo de siempre.
/// Lo que la plantilla de consola necesita del trazado (RetroArch): con lo
/// del RetroPad no cambia ningún número.
struct PadNeeds: Equatable {
    var leftStick = true
    var rightStick: RightStick = .analog
    var centerButtons = 2
    static let retroPad = PadNeeds()
}

extension RetroLayout {
    var needs: PadNeeds { PadNeeds(leftStick: leftStick, rightStick: rightStick, centerButtons: center.count) }
}

struct PadMetrics {
    let w: CGFloat
    let h: CGFloat
    let k: CGFloat
    /// Sin pantalla táctil: Pro Controller o el ajuste «GamePad sin pantalla».
    let noScreen: Bool
    let pro: Bool
    let switchPad: Bool
    /// RetroArch: el mismo trazado que Switch, con etiquetas de RetroPad
    /// (L2/R2, Select/Start, Menú, Rápido).
    let retroPad: Bool
    /// Stick y cruceta (o rombo) uno al lado del otro, L3/R3 junto a los
    /// gatillos y el centro en columna vertical.
    let row: Bool
    let gap: CGFloat = 6
    let headerH: CGFloat = 38
    let selectorH: CGFloat
    let bodyH: CGFloat
    let sideW: CGFloat
    let shoulderH: CGFloat
    let shoulderW: CGFloat
    let padSize: CGFloat
    let clickSize: CGFloat
    let faceBtn: CGFloat
    let centerW: CGFloat
    let bottomRowH: CGFloat
    /// Fila (o columna) del centro: −, Home, + y las pastillas TV/Pad y Soplar.
    let rowGap: CGFloat
    let roundBtn: CGFloat
    let pillW: CGFloat
    let pillH: CGFloat
    let touchW: CGFloat
    let touchH: CGFloat
    /// Cruceta: con stick izquierdo `padSize`; sin él (plantillas de RetroArch) crece.
    let dpadSize: CGFloat
    /// Caja de los botones frontales: `padSize`, o mayor sin stick derecho.
    let faceBox: CGFloat

    init(size: CGSize, noScreen: Bool = false, pro: Bool = false, switchPad: Bool = false, retroPad: Bool = false, needs: PadNeeds = .retroPad) {
        w = size.width
        h = size.height
        k = UiScale.factor(size, base: UiScale.phoneLandscape)
        self.pro = pro
        self.switchPad = switchPad
        self.retroPad = retroPad
        self.noScreen = noScreen || pro || switchPad
        selectorH = switchPad ? 48 : 36
        let gap: CGFloat = 6
        bodyH = h - 38 - selectorH - gap * 3
        shoulderH = Swift.min(Swift.max(bodyH * 0.09, 26), 40 * k)
        bottomRowH = Swift.min(Swift.max(bodyH * 0.16, 44), 60 * k)
        pillH = 30 * k
        let clickMax = 44 * k
        // Dos pads apilados bajo los gatillos
        let padH = (bodyH - shoulderH * 2 - gap * 3) / 2
        let pad: CGFloat
        if !self.noScreen {
            row = false
            sideW = w * 0.29
            pad = Swift.min(padH, sideW - clickMax - gap)
        } else {
            let rb = bottomRowH * 0.85
            let pillsN: CGFloat = switchPad ? 1 : pro ? 0 : 2
            let n = CGFloat(1 + needs.centerButtons)
            let need = rb * n + 10 * (n - 1 + pillsN) + 66 * k * pillsN + 20
            let sideS = (w - need - gap * 2) / 2
            let padStack = Swift.min(padH, sideS - clickMax - gap)
            let cw = pro && !switchPad ? rb : 66 * k
            let padRow = Swift.min(bodyH - shoulderH * 2 - gap * 2, (w - cw - gap * 4) / 4)
            row = padRow > padStack
            pad = row ? padRow : padStack
            sideW = row ? Swift.max(pad, 40) * 2 + gap : sideS
        }
        padSize = Swift.max(pad, 40)
        clickSize = Swift.min(Swift.max(padSize * 0.30, 30), 44 * k)
        shoulderW = Swift.min(Swift.max(sideW * 0.6, 90), 150 * k)
        faceBtn = padSize / 2.6
        // En fila el centro es lo que dejan los pads: justo su columna, o más si los limitó la altura
        centerW = row ? w - sideW * 2 - gap * 2 : Swift.max(w - sideW * 2 - gap * 2, 60)
        if row {
            rowGap = gap
            roundBtn = switchPad ? min(bottomRowH * 0.85, (bodyH - pillH - gap * 3) / CGFloat(1 + needs.centerButtons)) : bottomRowH * 0.85
            pillW = pro && !switchPad ? 0 : 66 * k
        } else {
            // La fila entera tiene que caber en el centro: pastillas y círculos se encogen juntos
            rowGap = centerW < 300 ? 6 : 10
            pillW = pro && !switchPad ? 0 : Swift.min(Swift.max(centerW * 0.22, 44), 66 * k)
            let pills: CGFloat = switchPad ? 1 : pro ? 0 : 2
            let n = CGFloat(1 + needs.centerButtons)
            roundBtn = Swift.min(Swift.max((centerW - rowGap * (n - 1 + pills) - pillW * pills) / n, 28), bottomRowH * 0.85)
        }
        let tw = Swift.min(centerW, (bodyH - bottomRowH - gap * 2) * (16.0 / 9.0))
        touchW = Swift.max(tw, 64)
        touchH = touchW * (9.0 / 16.0)
        // Sin stick izquierdo la cruceta ocupa su hueco y crece; sin stick
        // derecho, los botones frontales igual (en fila, los dos huecos;
        // apilado, la altura bajo los gatillos)
        let slotH = bodyH - shoulderH * 2 - gap * 2
        let grown = Swift.min(padSize * 1.35, slotH, row ? padSize * 2 + gap : slotH)
        dpadSize = needs.leftStick ? padSize : grown
        faceBox = needs.rightStick == RightStick.none ? grown : padSize
    }

    /// Tamaño de texto de los botones: crece con el iPad.
    func text(_ base: CGFloat) -> CGFloat { base * k }

    /// Tamaño máximo que se pide al receptor (píxeles, tope nativo 854×480).
    func screenRequest(scale: CGFloat) -> (Int, Int) {
        let px = Swift.min(ScreenClient.nativeWidth, Int((touchW * scale).rounded()))
        return (px, px * 9 / 16)
    }
}

/// GamePad de Wii U y mando entero de Switch, siempre apaisados. L/ZL
/// arriba a la izquierda y R/ZR a la derecha; stick izquierdo y cruceta a la
/// izquierda; stick derecho y rombo A/B/X/Y a la derecha; la pantalla táctil
/// en el centro con −, Home, +, TV/Pad y Soplar debajo. Como Pro Controller
/// (jugadores 2-4) no hay táctil, ni Soplar, ni TV/Pad. Con el ajuste «GamePad
/// sin pantalla» tampoco hay táctil (ni doble pantalla) y los botones crecen.
/// Switch usa este trazado sin pantalla y con una sola pastilla Capturar;
/// Switch utiliza siempre el trazado completo de Pro Controller.
///
/// Se enseña de forma optimista: conectando o esperando el eco de `mode cemu`
/// («Activando Wii U…») los controles van atenuados e inertes. Con el modo
/// confirmado, el motor emite como GamePad (80 bytes, sensores remapeados al
/// marco apaisado); al salir se suelta todo y el motor vuelve a lo de antes.
///
/// Doble pantalla: siendo el GamePad (Jugador 1, no Pro) y con el modo
/// confirmado se abre el canal de pantalla (`ScreenLink`) y la zona táctil
/// pinta la pantalla del GamePad de Cemu que manda el receptor.
struct GamePadScreen: View {
    let onDisconnect: () -> Void
    @ObservedObject var link = LinkState.shared
    @ObservedObject var screenLink = ScreenLink.shared
    @EnvironmentObject var model: AppModel
    @AppStorage(AppPrefs.gamePadNoScreenKey) private var noScreenPref = false
    @AppStorage(AppPrefs.gamePadFullScreenKey) private var fullScreenPref = false
    @AppStorage(AppPrefs.gamePadFullScreenKeyboardKey) private var fullScreenKb = true
    @State private var warnedOld = false
    @Environment(\.displayScale) private var displayScale
    @State private var keyboardOpen = false
    @State private var rotation: Int = OrientationLock.frameRotation(OrientationLock.current)
    /// Dónde está cada botón, para «Pulsar deslizando».
    @StateObject private var press = PressRegistry()
    /// La tarjeta de la cabecera está abierta: la pregunta del lado espera a
    /// que se pliegue.
    @State private var headerExpanded = true
    /// Lo que mide la cabecera ahora: la pregunta del lado se pone justo debajo
    /// cuando la cabecera no se va a plegar sola.
    @State private var headerBox: CGFloat = 0
    /// Con VoiceOver la cabecera se queda abierta hasta que la cierren.
    @Environment(\.accessibilityVoiceOverEnabled) private var voiceOver

    /// ¿Se va a plegar sola la cabecera? Si no, la pregunta del lado no puede
    /// esperar a que se pliegue o no saldría nunca.
    private var autoCollapse: Bool {
        // Sin el eco del modo la tarjeta no se pliega sola (ver `header`)
        HeaderCollapse.autoCollapses(connected: link.link.connected != nil && operative, screenReader: voiceOver)
    }

    private var wantedMode: String { Route.wantedMode(link.link, link.intent) }
    /// Switch y RetroArch comparten trazado (un solo pastilla, sin pantalla ni Soplar).
    private var switchPad: Bool { wantedMode == LinkState.modeSwitch || wantedMode == LinkState.modeRetroArch }
    private var retroPad: Bool { wantedMode == LinkState.modeRetroArch }
    private var operative: Bool { Route.extendedOperative(link.link, link.intent) }
    /// RetroArch: la plantilla de la consola del juego cargado (o la elegida a
    /// mano); fuera de RetroArch, el RetroPad de siempre (nada cambia).
    private var layout: RetroLayout {
        retroPad ? (RetroLayouts.byId(link.effectiveRetroLayout(link.link.connected)) ?? RetroLayouts.retroPad) : RetroLayouts.retroPad
    }
    private var layoutIdentity: String {
        [wantedMode, link.link.connected?.pad ?? "", String(link.link.connected?.player ?? 0), layout.id].joined(separator: ":")
    }

    /// Doble pantalla: solo el GamePad (no un Pro Controller), con el modo
    /// confirmado y sin el ajuste «GamePad sin pantalla».
    private var wantScreen: Bool {
        operative && !switchPad && link.link.connected?.pad == LinkState.padGamepad && !noScreenPref
    }

    /// Pantalla completa: solo la pantalla de Cemu y el táctil (mando real en
    /// el PC). Nunca sin el modo confirmado: el estado «Activando Wii U…»
    /// sigue con cabecera y «Salir».
    private var fullScreen: Bool { wantScreen && fullScreenPref }

    var body: some View {
        GeometryReader { geo in
            ZStack {
                // El primero: recoge los dedos que nacen fuera de los botones.
                // Inerte mientras no hay mando (esperando el modo) o solo está
                // la pantalla de Cemu
                SlideCanvas(enabled: operative && !fullScreen)
                content(size: geo.size)
            }
            .frame(width: geo.size.width, height: geo.size.height)
        }
        .coordinateSpace(name: PressRegistry.padSpace)
        .environmentObject(press)
        .background((fullScreen ? Color.black : Pepo.background).ignoresSafeArea())
        // Receptor anterior a 1.6: no confirma «solo pantalla» ni en el ok
        // ni con el eco; se avisa una vez (a los 2 s, por si el eco llega tarde)
        .task(id: fullScreen && link.link.connected?.screenOnly == nil) {
            guard fullScreen, link.link.connected?.screenOnly == nil, !warnedOld else { return }
            try? await Task.sleep(nanoseconds: 2_000_000_000)
            if !Task.isCancelled, fullScreen, link.link.connected?.screenOnly == nil {
                warnedOld = true
                LinkState.shared.publishNotice(tr("fullscreen_old_receiver"))
            }
        }
        .sheet(isPresented: $keyboardOpen) {
            KeyboardSheet { keyboardOpen = false }
        }
        // Solo con el modo confirmado el motor emite como GamePad; si el modo se
        // va (Mando de Wii) o se sale, vuelve lo de antes
        .onChange(of: operative) { _ in applyEngine() }
        .onChange(of: layoutIdentity) { _ in ButtonState.shared.reset(); applyEngine() }
        .onChange(of: rotation) { _ in ButtonState.shared.reset(); applyEngine() }
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
            model.gamePadSideProvisional = nil // la prueba del lado sin confirmar se olvida
            screenLink.release()
            // Al cambiar de pantalla, la pantalla nueva ya puede haber
            // configurado el motor. La anterior no debe deshacerlo al irse.
            if model.screen != .controller || Route.route(link.link, link.intent) == .gamePad {
                UIApplication.shared.isIdleTimerDisabled = false
                if let engine = link.motion {
                    engine.kind = link.role == LinkState.roleNunchuk ? .nunchuk : .wiimote
                    engine.rotation = Frame.rotation0
                }
            }
            press.releaseAll()
            ButtonState.shared.reset() // nada queda pulsado, inclinado ni tocado
        }
    }

    private func content(size: CGSize) -> some View {
        let connected = link.link.connected
        let operative = self.operative
        let pro = switchPad || connected?.pad == LinkState.padPro
        let m = PadMetrics(size: size, noScreen: noScreenPref, pro: pro, switchPad: switchPad, retroPad: retroPad, needs: layout.needs)
        let wantScreen = self.wantScreen
        let fullScreen = self.fullScreen
        // Tamaño que se pide al PC: en pantalla completa, el área entera en
        // píxeles (como mucho 854×480, estable); si no, lo de siempre
        let request = fullScreen
            ? FullScreenMetrics.streamRequest(
                containerPxW: Int((size.width * displayScale).rounded()),
                containerPxH: Int((size.height * displayScale).rounded())
            )
            : m.screenRequest(scale: displayScale)
        let onKeyboard: (() -> Void)? = operative && connected?.hasKeyboard == true ? { keyboardOpen = true } : nil
        return Group {
            if fullScreen {
                FullScreenGamePadView(
                    size: size, client: screenLink.client, showKeyboard: fullScreenKb && connected?.hasKeyboard == true,
                    onKeyboard: { keyboardOpen = true }, onDisconnect: onDisconnect
                )
            } else {
                normalContent(m: m, connected: connected, operative: operative)
            }
        }
        .onChange(of: wantScreen) { want in
            if want { screenLink.request(width: request.0, height: request.1) } else { screenLink.release() }
        }
        .onChange(of: [request.0, request.1]) { px in
            if wantScreen { screenLink.request(width: px[0], height: px[1]) }
        }
        .onAppear {
            if wantScreen { screenLink.request(width: request.0, height: request.1) }
            else { screenLink.release() }
        }
        // Primera vez: el lado del apaisado lo ha elegido iOS; se pregunta si es
        // el bueno y se guarda para siempre (Ajustes; aparte del mando + Nunchuk).
        // Con la tarjeta de la cabecera abierta se espera a que se pliegue (si
        // no, se solaparían) y, cuando no se pliega sola, se pone debajo
        .overlay(alignment: .top) {
            if model.gamePadSide == .unset, fullScreen || !headerExpanded || !autoCollapse {
                let shown = LandscapeSide.effective(saved: LandscapeSide.current(OrientationLock.current), provisional: model.gamePadSideProvisional)
                SideAskCard(
                    title: tr(retroPad ? "side_ask_retroarch" : switchPad ? "side_ask" : "side_ask_gamepad"),
                    onFlip: { model.gamePadSideProvisional = shown.flipped },
                    onKeep: { model.saveGamePadSide(shown) }
                )
                .padding(.top, fullScreen ? 8 : (headerExpanded ? headerBox + 4 : m.headerH + m.gap))
            }
        }
        // La última: la tarjeta desplegada gana al selector, a los controles y
        // al aviso. A pantalla completa no hay cabecera
        .overlay(alignment: .top) {
            if !fullScreen {
                header(operative: operative, onKeyboard: onKeyboard).background(
                    GeometryReader { g in
                        Color.clear
                            .onAppear { headerBox = g.size.height }
                            .onChange(of: g.size.height) { h in headerBox = h }
                    }
                    .allowsHitTesting(false)
                )
            }
        }
    }

    /// Cabecera plegable: la pastilla con el modo y, a su lado y siempre a la
    /// vista, «Teclado» (jugando tiene que estar a un toque). Desplegada, la
    /// tarjeta con el PC, el estado, los chips y «Salir».
    private func header(operative: Bool, onKeyboard: (() -> Void)?) -> some View {
        CollapsibleHeader(
            handleLabel: headerLabel,
            // Esperando el eco del modo el mando está inerte: la tarjeta se
            // queda (es lo que explica el «Activando…» y donde están los
            // chips y Salir), así que para plegarse sola hace falta el modo
            connected: link.link.connected != nil && operative,
            topPadding: 0,
            onExpandedChange: { headerExpanded = $0 },
            beside: {
                if let onKeyboard {
                    KeyboardButton(compact: true, action: onKeyboard).fixedSize()
                }
            }
        ) {
            GamePadHeaderCard(
                link: link.link, operative: operative, wantedMode: wantedMode,
                screen: switchPad ? nil : screenLink.client, onDisconnect: onDisconnect
            )
        }
    }

    /// Texto de la pastilla: el modo pedido («Wii U» o «Switch»), lo mismo que
    /// decía la cabecera de siempre.
    private var headerLabel: String {
        // Rehaciéndose la sesión, el nombre del PC con su intento: eso no es
        // un texto compartido con Android, lo pinta la pastilla aquí
        if case .reconnecting(let pc, _) = link.link { return tr("status_reconnecting", pc) }
        var connecting = false
        if case .connecting = link.link { connecting = true }
        return GamePadHeaderText.handle(
            connected: link.link.connected != nil,
            connecting: connecting,
            mode: modeLabel(wantedMode),
            connectingText: tr("status_connecting"),
            disconnectedText: tr("status_disconnected")
        )
    }

    private func normalContent(m: PadMetrics, connected: ConnectedLink?, operative: Bool) -> some View {
        ZStack(alignment: .top) {
            VStack(spacing: 0) {
                // El hueco de la cabecera: la pastilla y su tarjeta van encima
                // de todo, en la capa de arriba, sin mover nada de esto
                Spacer().frame(height: m.headerH)
                Spacer().frame(height: m.gap)
                // Qué mando soy en Cemu: debajo de la cabecera, centrado
                ZStack {
                    if let c = connected { PadSelector(link: c, width: m.w, compact: true, switchPad: switchPad).disabled(!operative) }
                }
                .frame(height: m.selectorH)
                Spacer().frame(height: m.gap)
                HStack(spacing: m.gap) {
                    LeftColumn(m: m, layout: layout)
                    CenterColumn(m: m, layout: layout, client: screenLink.client)
                    RightColumn(m: m, layout: layout)
                }
                .id(layoutIdentity)
                .frame(height: m.bodyH)
                // Inerte hasta que el receptor confirme el modo: nada llega a los controles
                .opacity(operative ? 1 : 0.4)
                .allowsHitTesting(operative)
                .overlay {
                    if !operative {
                        Text(tr(retroPad ? "activating_retroarch" : switchPad ? "activating_switch" : "activating_wiiu"))
                            .pepoTitle().padding(14).pepoCard()
                    }
                }
            }
            NoticeBanner().padding(.top, m.headerH + m.selectorH + m.gap * 2)
        }
    }

    private func applyEngine() {
        guard let engine = link.motion else { return }
        if operative {
            engine.rotation = rotation
            engine.kind = switchPad ? .switchPad : .gamepad
        } else {
            engine.kind = link.role == LinkState.roleNunchuk ? .nunchuk : .wiimote
            engine.rotation = Frame.rotation0
            ButtonState.shared.reset()
        }
    }
}

/// Izquierda: L y ZL en la esquina, stick (+ L3), cruceta. En fila: L3 junto
/// a los gatillos y stick y cruceta uno al lado del otro, abajo.
private struct LeftColumn: View {
    let m: PadMetrics
    /// La plantilla de consola (RetroArch); el RetroPad fuera de RetroArch.
    let layout: RetroLayout

    // Hombros y stick según la plantilla: sin stick, la cruceta ocupa su hueco y crece
    private var shoulders: some View {
        VStack(alignment: .leading, spacing: m.gap) {
            if let l = m.retroPad ? layout.shoulders.l : Optional("L") {
                ShoulderButton(label: l, bit: Btn.l, width: m.shoulderW, height: m.shoulderH, textSize: m.text(16))
            }
            if let l2 = m.retroPad ? layout.shoulders.l2 : Optional("ZL") {
                ShoulderButton(label: l2, bit: Btn.zl, width: m.shoulderW, height: m.shoulderH, textSize: m.text(16))
            }
        }
    }

    @ViewBuilder private var stick: some View {
        if layout.leftStick {
            AnalogStick(size: m.padSize) { x, y in ButtonState.shared.setStick(x, y) }
        }
    }

    @ViewBuilder private var click: some View {
        if layout.stickClicks {
            RoundButton(label: "L3", size: m.clickSize, bit: Btn.stickL, textSize: m.text(12))
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if m.row {
                HStack(alignment: .center, spacing: m.gap) {
                    shoulders
                    click
                }
                Spacer(minLength: 0)
                HStack(alignment: .center, spacing: m.gap) {
                    stick
                    PadCross(size: m.dpadSize)
                }
                .frame(width: m.sideW)
            } else {
                shoulders
                Spacer(minLength: 0)
                if layout.leftStick {
                    HStack(alignment: .bottom, spacing: m.gap) {
                        stick
                        click
                    }
                    // iPad: stick y cruceta juntos, abajo (donde llega el pulgar), en
                    // vez de repartidos por toda la altura
                    if m.k > 1 { Spacer().frame(height: m.gap * 2) } else { Spacer(minLength: 0) }
                }
                PadCross(size: m.dpadSize)
                if !layout.leftStick { Spacer(minLength: 0) }
            }
        }
        .frame(width: m.sideW, height: m.bodyH)
    }
}

/// Derecha: R y ZR en la esquina, (R3 +) stick, rombo A/B/X/Y. En fila: R3
/// junto a los gatillos y rombo y stick uno al lado del otro, abajo.
private struct RightColumn: View {
    let m: PadMetrics
    let layout: RetroLayout

    private var shoulders: some View {
        VStack(alignment: .trailing, spacing: m.gap) {
            if let r = m.retroPad ? layout.shoulders.r : Optional("R") {
                ShoulderButton(label: r, bit: Btn.r, width: m.shoulderW, height: m.shoulderH, textSize: m.text(16))
            }
            if let r2 = m.retroPad ? layout.shoulders.r2 : Optional("ZR") {
                ShoulderButton(label: r2, bit: Btn.zr, width: m.shoulderW, height: m.shoulderH, textSize: m.text(16))
            }
        }
    }

    /// El hueco del stick derecho: stick, los botones C de N64 o nada.
    @ViewBuilder private var stick: some View {
        switch layout.rightStick {
        case .analog: AnalogStick(size: m.padSize) { x, y in ButtonState.shared.setStick2(x, y) }
        case .cbuttons: CButtons(size: m.padSize) { x, y in ButtonState.shared.setStick2(x, y) }
        case RightStick.none: EmptyView()
        }
    }

    @ViewBuilder private var click: some View {
        if layout.stickClicks {
            RoundButton(label: "R3", size: m.clickSize, bit: Btn.stickR, textSize: m.text(12))
        }
    }

    private var hasRight: Bool { layout.rightStick != RightStick.none }

    var body: some View {
        VStack(alignment: .trailing, spacing: 0) {
            if m.row {
                HStack(alignment: .center, spacing: m.gap) {
                    click
                    shoulders
                }
                Spacer(minLength: 0)
                HStack(alignment: .center, spacing: m.gap) {
                    FaceCluster(layout: layout, boxW: m.faceBox, boxH: m.faceBox)
                    stick
                }
                .frame(width: m.sideW)
            } else {
                shoulders
                Spacer(minLength: 0)
                if hasRight {
                    HStack(alignment: .bottom, spacing: m.gap) {
                        click
                        stick
                    }
                    if m.k > 1 { Spacer().frame(height: m.gap * 2) } else { Spacer(minLength: 0) }
                }
                FaceCluster(layout: layout, boxW: m.faceBox, boxH: m.faceBox)
                if !hasRight { Spacer(minLength: 0) }
            }
        }
        .frame(width: m.sideW, height: m.bodyH)
    }
}

/// Centro: pantalla táctil (si la hay) y la fila − · Home · + (con TV/Pad y
/// Soplar). En fila, una columna vertical con lo mismo.
private struct CenterColumn: View {
    let m: PadMetrics
    let layout: RetroLayout
    let client: ScreenClient?

    // RetroArch: los botones del centro de la consola (Select/Start, Mode/Start,
    // Pause, Run…) con Menú (el menú de RetroArch) en medio, y Capturar es el
    // avance rápido (mantener)
    private var home: some View { RoundButton(label: tr(m.retroPad ? "retro_menu" : "home_btn"), size: m.roundBtn, bit: Btn.home, textSize: m.text(m.retroPad ? 10 : 12)) }
    private func centerButton(_ c: RetroCenter) -> some View {
        RoundButton(label: retroLabel(c.label), size: m.roundBtn, bit: c.bit, textSize: m.text(9))
    }
    @ViewBuilder private var centers: some View {
        if m.retroPad {
            if layout.center.count >= 2 {
                centerButton(layout.center[0])
                home
                centerButton(layout.center[1])
            } else {
                home
                ForEach(layout.center, id: \.label) { centerButton($0) }
            }
        } else {
            RoundButton(label: "−", size: m.roundBtn, bit: Btn.minus, textSize: m.text(19))
            home
            RoundButton(label: "+", size: m.roundBtn, bit: Btn.plus, textSize: m.text(19))
        }
    }
    private var tv: some View { ShoulderButton(label: tr("tv_pad"), bit: Btn.screen, width: m.pillW, height: m.pillH, textSize: m.text(12)) }
    private var blow: some View { ShoulderButton(label: tr("blow"), bit: Btn.mic, width: m.pillW, height: m.pillH, textSize: m.text(12)) }
    private var capture: some View { ShoulderButton(label: tr(m.retroPad ? "retro_ff" : "capture"), bit: Btn.screen, width: m.pillW, height: m.pillH, textSize: m.text(12)) }

    var body: some View {
        VStack(spacing: 0) {
            Spacer(minLength: 0)
            if m.row {
                VStack(spacing: m.gap) {
                    centers
                    if m.switchPad {
                        capture
                    } else if !m.pro {
                        tv
                        blow
                    }
                }
            } else {
                if m.pro {
                    Text(m.retroPad ? layout.name : tr(m.switchPad ? "mode_switch" : "pro_controller")).pepoBody().multilineTextAlignment(.center)
                    // iPad: la fila pegada al texto (un bloque centrado)
                    if m.k > 1 { Spacer().frame(height: m.gap * 3) } else { Spacer(minLength: 0) }
                } else if !m.noScreen {
                    TouchScreenView(width: m.touchW, height: m.touchH, client: client)
                    // iPad: la fila pegada a la pantalla (un bloque centrado)
                    if m.k > 1 { Spacer().frame(height: m.gap * 3) } else { Spacer(minLength: 0) }
                }
                HStack(alignment: .center, spacing: m.rowGap) {
                    if m.switchPad { capture } else if !m.pro { tv }
                    centers
                    if !m.pro && !m.switchPad { blow }
                }
            }
            Spacer(minLength: 0)
        }
        .frame(width: m.centerW, height: m.bodyH)
    }
}

/// Tarjeta de la cabecera plegable: el PC y «Salir», debajo el estado
/// («J1 · GamePad · 23 ms» o «Activando Wii U…») con el ritmo de la doble
/// pantalla, y debajo los chips de modo (Jugador 1). Aquí cabe todo, así que
/// ya no hay puertas de anchura. «Teclado» NO va dentro: va junto a la
/// pastilla, siempre a la vista.
private struct GamePadHeaderCard: View {
    let link: UiLink
    let operative: Bool
    let wantedMode: String
    let screen: ScreenClient?
    let onDisconnect: () -> Void

    /// «J1 · GamePad · 23 ms»; sin el modo confirmado, «Activando Wii U…».
    private func statusText(_ c: ConnectedLink) -> String {
        let padName = wantedMode == LinkState.modeSwitch
            ? switchPadLabel(c.pad)
            : wantedMode == LinkState.modeRetroArch
            ? (RetroLayouts.byId(LinkState.shared.wouldBeRetroLayout(c))?.name ?? tr("retropad"))
            : (c.pad == LinkState.padPro ? tr("pro_controller") : tr("gamepad"))
        return GamePadHeaderText.status(
            operative: operative,
            player: c.player,
            padName: padName,
            rttMs: c.rttMs,
            activating: tr(wantedMode == LinkState.modeSwitch ? "activating_switch"
                : wantedMode == LinkState.modeRetroArch ? "activating_retroarch"
                : "activating_wiiu")
        )
    }

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 20) {
                switch link {
                case .connected(let c):
                    Text(c.pcName)
                        .pepoBody()
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(maxWidth: 160)
                case .connecting:
                    Text(tr("status_connecting")).pepoBody().lineLimit(1)
                case .reconnecting(let pc, _):
                    // Reconectando, el punto latiendo: es la única señal de que
                    // el servicio está rehaciendo la sesión (como en Android)
                    ReconnectingLabel(pcName: pc, font: PepoFont.bodyMedium())
                default:
                    Text(tr("status_disconnected")).pepoBody().lineLimit(1)
                }
                TextLink(title: tr("exit"), color: Pepo.error, action: onDisconnect).fixedSize()
            }
            if case .connected(let c) = link {
                HStack(spacing: 8) {
                    Text(statusText(c)).pepoBody().lineLimit(1).truncationMode(.tail)
                    if let screen {
                        FpsLabel(client: screen)
                    }
                }
                // RetroArch: el juego cargado (lo que RetroArch acaba de abrir, según el PC)
                if wantedMode == LinkState.modeRetroArch, let g = c.game, !g.title.isEmpty {
                    Text([g.title, g.core].filter { !$0.isEmpty }.joined(separator: " · "))
                        .font(PepoFont.nunito(12, .regular))
                        .foregroundColor(Pepo.textDim)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                if c.slot == 0 {
                    // Mientras se espera el eco va marcado el modo pedido (es lo pedido)
                    ModeChips(current: operative ? c.mode : wantedMode, supportsCemu: c.supportsCemu, supportsSwitch: c.supportsSwitch, supportsRetroArch: c.supportsRetroArch, supportsPointer: c.receiver.supportsPointer, compact: true)
                }
                // RetroArch: guardar/cargar estado, ranura, rebobinar, pausa…
                if operative, Route.isRetroArch(link) {
                    RetroArchHotkeys(compact: true)
                }
            }
        }
    }
}

/// «pantalla · 30 fps» (media de 1 s), solo con imagen.
private struct FpsLabel: View {
    @ObservedObject var client: ScreenClient

    var body: some View {
        Group {
            if client.fps > 0 {
                Text(tr("screen_fps", client.fps))
                    .font(PepoFont.nunito(12, .regular))
                    .foregroundColor(Pepo.textDim)
                    .lineLimit(1)
            }
        }
    }
}

/// Rombo: X arriba, Y izquierda, A derecha (azul, destacado), B abajo. El
/// texto crece con el botón (18 y 20 pt en los 44 pt de un iPhone).
struct FaceButtons: View {
    let size: CGFloat
    let btn: CGFloat

    var body: some View {
        let off = (size - btn) / 2
        let text = btn * (18.0 / 44.0)
        return ZStack {
            RoundButton(label: "X", size: btn, bit: Btn.x, textSize: text).offset(y: -off)
            RoundButton(label: "Y", size: btn, bit: Btn.y, textSize: text).offset(x: -off)
            RoundButton(label: "A", size: btn, bit: Btn.a, background: Pepo.blue, pressedColor: Pepo.blueHover, textColor: Pepo.onAccent, textSize: btn * (20.0 / 44.0), pop: true).offset(x: off)
            RoundButton(label: "B", size: btn, bit: Btn.b, textSize: text).offset(y: off)
        }
        .frame(width: size, height: size)
    }
}

/// «Pantalla del GamePad a pantalla completa»: solo la pantalla de Cemu,
/// ajustada a su proporción real sobre fondo negro, con el táctil sobre la
/// imagen (un gesto que empieza en las bandas negras se ignora), una ✕
/// pequeña arriba a la izquierda para salir y, si el ajuste lo pide, el botón
/// de teclado arriba a la derecha. Sin sticks ni botones: el mando real va
/// en el PC.
private struct FullScreenGamePadView: View {
    let size: CGSize
    let client: ScreenClient?
    let showKeyboard: Bool
    let onKeyboard: () -> Void
    let onDisconnect: () -> Void

    var body: some View {
        ZStack(alignment: .top) {
            FullScreenTouchView(size: size, client: client)
            NoticeBanner().padding(.top, 8)
            HStack(alignment: .top) {
                Button(action: onDisconnect) {
                    Text("✕")
                        .font(PepoFont.nunito(16, .bold))
                        .foregroundColor(Pepo.text)
                        .frame(width: 34, height: 34)
                        .background(Pepo.card.opacity(0.7))
                        .clipShape(Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(tr("exit"))
                Spacer()
                if showKeyboard {
                    KeyboardButton(compact: true, action: onKeyboard).opacity(0.7)
                }
            }
            .padding(8)
        }
    }
}

/// La pantalla del GamePad a pantalla completa: la misma imagen (y el mismo
/// ACK de fotograma) que `ScreenImageView`, pero el rectángulo de la imagen
/// (16:9 mientras no hay fotograma) lo da `FullScreenMetrics` y el táctil se
/// mapea sobre él.
private struct FullScreenTouchView: View {
    let size: CGSize
    let client: ScreenClient?
    @State private var finger: CGPoint?
    @State private var ignoring = false

    private var rect: FitRect {
        FullScreenMetrics.fitRect(container: size, image: client?.image?.image.size)
    }

    private func report(_ p: CGPoint, _ down: Bool) {
        let (fx, fy) = FullScreenMetrics.fraction(rect, p)
        ButtonState.shared.setTouch(fx, fy, down)
    }

    var body: some View {
        ZStack {
            Color.black
            if let client {
                ScreenImageView(client: client, width: size.width, height: size.height)
            } else {
                Text(tr("touch_screen")).pepoBody()
            }
            if let p = finger {
                Circle()
                    .fill(Pepo.blue)
                    .frame(width: 14, height: 14)
                    .position(p)
            }
        }
        .frame(width: size.width, height: size.height)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0, coordinateSpace: .local)
                .onChanged { v in
                    if finger == nil && !ignoring {
                        // el gesto empieza en las bandas negras: no es un toque en la pantalla
                        if !FullScreenMetrics.contains(rect, v.startLocation) {
                            ignoring = true
                            return
                        }
                        Haptics.tick()
                    }
                    if ignoring { return }
                    finger = v.location
                    report(v.location, true)
                }
                .onEnded { v in
                    if ignoring {
                        ignoring = false
                        return
                    }
                    finger = nil
                    report(v.location, false)
                }
        )
    }
}

/// Pantalla táctil del GamePad: rectángulo 16:9 con marco fino. Un solo
/// dedo; mientras esté, su posición dentro del rectángulo (recortada a 0..1)
/// va como fracción 0..65535 con FLAG_TOUCH; al soltar, sin dedo. Con el
/// canal de pantalla abierto pinta la imagen del GamePad de Cemu.
private struct TouchScreenView: View {
    let width: CGFloat
    let height: CGFloat
    let client: ScreenClient?
    @State private var finger: CGPoint?

    private func report(_ p: CGPoint, _ down: Bool) {
        let fx = Swift.min(Swift.max(p.x / width, 0), 1)
        let fy = Swift.min(Swift.max(p.y / height, 0), 1)
        ButtonState.shared.setTouch(Int((fx * 65535).rounded()), Int((fy * 65535).rounded()), down)
    }

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Pepo.card)
            if let client {
                ScreenImageView(client: client, width: width, height: height)
            } else {
                Text(tr("touch_screen")).pepoBody()
            }
            if let p = finger {
                Circle()
                    .fill(Pepo.blue)
                    .frame(width: 14, height: 14)
                    .position(p)
            }
        }
        .frame(width: width, height: height)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(finger != nil ? Pepo.glow : Pepo.cardBorder, lineWidth: 1.5))
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0, coordinateSpace: .local)
                .onChanged { v in
                    if finger == nil { Haptics.tick() }
                    finger = v.location
                    report(v.location, true)
                }
                .onEnded { v in
                    finger = nil
                    report(v.location, false)
                }
        )
    }
}

/// La imagen del canal de pantalla (o el estado mientras no la hay), y la
/// confirmación al receptor en cuanto se pinta cada una.
private struct ScreenImageView: View {
    @ObservedObject var client: ScreenClient
    let width: CGFloat
    let height: CGFloat

    private var statusText: String {
        guard let s = client.status else { return tr("screen_connecting") }
        if s == ScreenClient.statusUnavailable { return tr("screen_unavailable") }
        if s == ScreenClient.statusReconnecting { return tr("screen_reconnecting") }
        if s.hasPrefix(ScreenClient.statusDetail) { return tr("screen_unavailable_detail", String(s.dropFirst(ScreenClient.statusDetail.count))) }
        return s
    }

    var body: some View {
        ZStack {
            if let img = client.image {
                Image(uiImage: img.image)
                    .resizable()
                    .interpolation(.medium)
                    .aspectRatio(contentMode: .fit)
                    .frame(width: width, height: height)
            } else {
                VStack(spacing: 2) {
                    Text(tr("touch_screen")).pepoBody()
                    Text(statusText)
                        .font(PepoFont.nunito(12, .regular))
                        .foregroundColor(Pepo.textDim)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 12)
                }
            }
        }
        .onReceive(client.$image) { img in
            // Pintada en el siguiente ciclo de dibujo: entonces sale la confirmación al receptor
            guard let img else { return }
            DispatchQueue.main.async { client.shown(img.seq) }
        }
    }
}
