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
struct PadMetrics {
    let w: CGFloat
    let h: CGFloat
    let k: CGFloat
    /// Sin pantalla táctil: Pro Controller o el ajuste «GamePad sin pantalla».
    let noScreen: Bool
    let pro: Bool
    /// Stick y cruceta (o rombo) uno al lado del otro, L3/R3 junto a los
    /// gatillos y el centro en columna vertical.
    let row: Bool
    let gap: CGFloat = 6
    let headerH: CGFloat = 38
    let selectorH: CGFloat = 36
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

    init(size: CGSize, noScreen: Bool = false, pro: Bool = false) {
        w = size.width
        h = size.height
        k = UiScale.factor(size, base: UiScale.phoneLandscape)
        self.pro = pro
        self.noScreen = noScreen || pro
        let gap: CGFloat = 6
        bodyH = h - 38 - 36 - gap * 3
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
            let need = pro ? rb * 3 + 10 * 2 + 20 : rb * 3 + 10 * 4 + 66 * k * 2 + 20
            let sideS = (w - need - gap * 2) / 2
            let padStack = Swift.min(padH, sideS - clickMax - gap)
            let cw = pro ? rb : 66 * k
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
            roundBtn = bottomRowH * 0.85
            pillW = pro ? 0 : 66 * k
        } else {
            // La fila entera tiene que caber en el centro: pastillas y círculos se encogen juntos
            rowGap = centerW < 300 ? 6 : 10
            pillW = pro ? 0 : Swift.min(Swift.max(centerW * 0.22, 44), 66 * k)
            let pills: CGFloat = pro ? 0 : 2
            roundBtn = Swift.min(Swift.max((centerW - rowGap * (2 + pills) - pillW * pills) / 3, 28), bottomRowH * 0.85)
        }
        let tw = Swift.min(centerW, (bodyH - bottomRowH - gap * 2) * (16.0 / 9.0))
        touchW = Swift.max(tw, 64)
        touchH = touchW * (9.0 / 16.0)
    }

    /// Tamaño de texto de los botones: crece con el iPad.
    func text(_ base: CGFloat) -> CGFloat { base * k }

    /// Flechas de la cruceta: crecen con el pad (en un iPhone con pantalla, 14).
    var crossGlyph: CGFloat { Swift.max(14 * k, padSize * 0.12) }

    /// Tamaño máximo que se pide al receptor (píxeles, tope nativo 854×480).
    func screenRequest(scale: CGFloat) -> (Int, Int) {
        let px = Swift.min(ScreenClient.nativeWidth, Int((touchW * scale).rounded()))
        return (px, px * 9 / 16)
    }
}

/// GamePad de Wii U (modo Cemu), siempre apaisado. Como el mando real: L/ZL
/// arriba a la izquierda y R/ZR a la derecha; stick izquierdo y cruceta a la
/// izquierda; stick derecho y rombo A/B/X/Y a la derecha; la pantalla táctil
/// en el centro con −, Home, +, TV/Pad y Soplar debajo. Como Pro Controller
/// (jugadores 2-4) no hay táctil, ni Soplar, ni TV/Pad. Con el ajuste «GamePad
/// sin pantalla» tampoco hay táctil (ni doble pantalla) y los botones crecen.
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
    @AppStorage(AppPrefs.gamePadNoScreenKey) private var noScreenPref = false
    @Environment(\.displayScale) private var displayScale
    @State private var keyboardOpen = false
    @State private var rotation: Int = OrientationLock.frameRotation(OrientationLock.current)

    private var operative: Bool { Route.isGamePad(link.link) }

    var body: some View {
        GeometryReader { geo in
            content(size: geo.size)
                .frame(width: geo.size.width, height: geo.size.height)
        }
        .background(Pepo.background.ignoresSafeArea())
        .sheet(isPresented: $keyboardOpen) {
            KeyboardSheet { keyboardOpen = false }
        }
        // Solo con el modo confirmado el motor emite como GamePad; si el modo se
        // va (Mando de Wii) o se sale, vuelve lo de antes
        .onChange(of: operative) { _ in applyEngine() }
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
            UIApplication.shared.isIdleTimerDisabled = false
            screenLink.release()
            if let engine = link.motion {
                engine.kind = link.role == LinkState.roleNunchuk ? .nunchuk : .wiimote
                engine.rotation = Frame.rotation0
            }
            ButtonState.shared.reset() // nada queda pulsado, inclinado ni tocado
        }
    }

    private func content(size: CGSize) -> some View {
        let connected = link.link.connected
        let operative = self.operative
        let pro = connected?.pad == LinkState.padPro
        let m = PadMetrics(size: size, noScreen: noScreenPref, pro: pro)
        // Doble pantalla: solo el GamePad (no un Pro Controller), con el modo
        // confirmado y sin el ajuste «GamePad sin pantalla»
        let wantScreen = operative && connected?.pad == LinkState.padGamepad && !noScreenPref
        let request = m.screenRequest(scale: displayScale)
        let onKeyboard: (() -> Void)? = operative ? { keyboardOpen = true } : nil
        return ZStack(alignment: .top) {
            VStack(spacing: 0) {
                GamePadHeader(
                    link: link.link, operative: operative, height: m.headerH, width: m.w,
                    screen: screenLink.client, onKeyboard: onKeyboard, onDisconnect: onDisconnect
                )
                Spacer().frame(height: m.gap)
                // Qué mando soy en Cemu: debajo de la cabecera, centrado
                ZStack {
                    if let c = connected { PadSelector(link: c, width: m.w, compact: true) }
                }
                .frame(height: m.selectorH)
                Spacer().frame(height: m.gap)
                HStack(spacing: m.gap) {
                    LeftColumn(m: m)
                    CenterColumn(m: m, client: screenLink.client)
                    RightColumn(m: m)
                }
                .frame(height: m.bodyH)
                // Inerte hasta que el receptor confirme el modo: nada llega a los controles
                .opacity(operative ? 1 : 0.4)
                .allowsHitTesting(operative)
            }
            NoticeBanner().padding(.top, m.headerH + m.selectorH + m.gap * 2)
        }
        .onChange(of: wantScreen) { want in
            if want { screenLink.request(width: request.0, height: request.1) } else { screenLink.release() }
        }
        .onChange(of: request.0) { px in
            if wantScreen { screenLink.request(width: px, height: px * 9 / 16) }
        }
        .onAppear {
            if wantScreen { screenLink.request(width: request.0, height: request.1) }
        }
    }

    private func applyEngine() {
        guard let engine = link.motion else { return }
        if operative {
            engine.rotation = rotation
            engine.kind = .gamepad
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

    private var shoulders: some View {
        VStack(alignment: .leading, spacing: m.gap) {
            ShoulderButton(label: "L", bit: Btn.l, width: m.shoulderW, height: m.shoulderH, textSize: m.text(16))
            ShoulderButton(label: "ZL", bit: Btn.zl, width: m.shoulderW, height: m.shoulderH, textSize: m.text(16))
        }
    }

    private var stick: some View {
        AnalogStick(size: m.padSize) { x, y in ButtonState.shared.setStick(x, y) }
    }

    private var click: some View {
        RoundButton(label: "L3", size: m.clickSize, bit: Btn.stickL, textSize: m.text(12))
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
                    PadCross(size: m.padSize, glyph: m.crossGlyph)
                }
            } else {
                shoulders
                Spacer(minLength: 0)
                HStack(alignment: .bottom, spacing: m.gap) {
                    stick
                    click
                }
                // iPad: stick y cruceta juntos, abajo (donde llega el pulgar), en
                // vez de repartidos por toda la altura
                if m.k > 1 { Spacer().frame(height: m.gap * 2) } else { Spacer(minLength: 0) }
                PadCross(size: m.padSize, glyph: m.crossGlyph)
            }
        }
        .frame(width: m.sideW, height: m.bodyH)
    }
}

/// Derecha: R y ZR en la esquina, (R3 +) stick, rombo A/B/X/Y. En fila: R3
/// junto a los gatillos y rombo y stick uno al lado del otro, abajo.
private struct RightColumn: View {
    let m: PadMetrics

    private var shoulders: some View {
        VStack(alignment: .trailing, spacing: m.gap) {
            ShoulderButton(label: "R", bit: Btn.r, width: m.shoulderW, height: m.shoulderH, textSize: m.text(16))
            ShoulderButton(label: "ZR", bit: Btn.zr, width: m.shoulderW, height: m.shoulderH, textSize: m.text(16))
        }
    }

    private var stick: some View {
        AnalogStick(size: m.padSize) { x, y in ButtonState.shared.setStick2(x, y) }
    }

    private var click: some View {
        RoundButton(label: "R3", size: m.clickSize, bit: Btn.stickR, textSize: m.text(12))
    }

    var body: some View {
        VStack(alignment: .trailing, spacing: 0) {
            if m.row {
                HStack(alignment: .center, spacing: m.gap) {
                    click
                    shoulders
                }
                Spacer(minLength: 0)
                HStack(alignment: .center, spacing: m.gap) {
                    FaceButtons(size: m.padSize, btn: m.faceBtn)
                    stick
                }
            } else {
                shoulders
                Spacer(minLength: 0)
                HStack(alignment: .bottom, spacing: m.gap) {
                    click
                    stick
                }
                if m.k > 1 { Spacer().frame(height: m.gap * 2) } else { Spacer(minLength: 0) }
                FaceButtons(size: m.padSize, btn: m.faceBtn)
            }
        }
        .frame(width: m.sideW, height: m.bodyH)
    }
}

/// Centro: pantalla táctil (si la hay) y la fila − · Home · + (con TV/Pad y
/// Soplar). En fila, una columna vertical con lo mismo.
private struct CenterColumn: View {
    let m: PadMetrics
    let client: ScreenClient?

    private var minus: some View { RoundButton(label: "−", size: m.roundBtn, bit: Btn.minus, textSize: m.text(19)) }
    private var home: some View { RoundButton(label: tr("home_btn"), size: m.roundBtn, bit: Btn.home, textSize: m.text(12)) }
    private var plus: some View { RoundButton(label: "+", size: m.roundBtn, bit: Btn.plus, textSize: m.text(19)) }
    private var tv: some View { ShoulderButton(label: tr("tv_pad"), bit: Btn.screen, width: m.pillW, height: m.pillH, textSize: m.text(12)) }
    private var blow: some View { ShoulderButton(label: tr("blow"), bit: Btn.mic, width: m.pillW, height: m.pillH, textSize: m.text(12)) }

    var body: some View {
        VStack(spacing: 0) {
            Spacer(minLength: 0)
            if m.row {
                VStack(spacing: m.gap) {
                    minus
                    home
                    plus
                    if !m.pro {
                        tv
                        blow
                    }
                }
            } else {
                if m.pro {
                    Text(tr("pro_controller")).pepoBody().multilineTextAlignment(.center)
                    // iPad: la fila pegada al texto (un bloque centrado)
                    if m.k > 1 { Spacer().frame(height: m.gap * 3) } else { Spacer(minLength: 0) }
                } else if !m.noScreen {
                    TouchScreenView(width: m.touchW, height: m.touchH, client: client)
                    // iPad: la fila pegada a la pantalla (un bloque centrado)
                    if m.k > 1 { Spacer().frame(height: m.gap * 3) } else { Spacer(minLength: 0) }
                }
                HStack(alignment: .center, spacing: m.rowGap) {
                    if !m.pro { tv }
                    minus
                    home
                    plus
                    if !m.pro { blow }
                }
            }
            Spacer(minLength: 0)
        }
        .frame(width: m.centerW, height: m.bodyH)
    }
}

/// Cabecera compacta: PC · «J1 · GamePad» / «J2 · Pro Controller» · ritmo de
/// la doble pantalla · chips de modo (Jugador 1) · «Teclado» · Salir. Si no
/// cabe todo, cae primero el ritmo de la pantalla, luego el nombre del PC,
/// luego el estado; los chips y «Teclado» solo en último extremo; «Salir» nunca.
private struct GamePadHeader: View {
    let link: UiLink
    let operative: Bool
    let height: CGFloat
    let width: CGFloat
    let screen: ScreenClient?
    let onKeyboard: (() -> Void)?
    let onDisconnect: () -> Void

    /// «J1 · GamePad · 23 ms», acortado en móviles estrechos antes de recortarse.
    private func statusText(_ c: ConnectedLink) -> String {
        if !operative { return tr("activating_wiiu") }
        var s = "J\(c.player)"
        if width >= 560 { s += " · " + (c.pad == LinkState.padPro ? tr("pro_controller") : tr("gamepad")) }
        if width >= 700, let rtt = c.rttMs { s += " · \(String(format: "%.0f", rtt)) ms" }
        return s
    }

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            switch link {
            case .connected(let c):
                Text(c.pcName)
                    .pepoTitle()
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: 160, alignment: .leading)
                    .layoutPriority(0)
                Text(statusText(c))
                    .pepoBody()
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .layoutPriority(1)
                if let screen {
                    FpsLabel(client: screen).layoutPriority(-1)
                }
                Spacer(minLength: 0)
                if c.slot == 0 {
                    // Mientras se espera el eco, Wii U ya va marcado (es lo pedido)
                    ModeChips(current: operative ? c.mode : LinkState.modeCemu, supportsCemu: c.supportsCemu, compact: true)
                        .fixedSize()
                        .layoutPriority(2)
                }
                if let onKeyboard {
                    KeyboardButton(compact: true, action: onKeyboard).fixedSize().layoutPriority(3)
                }
            case .connecting:
                Text(tr("status_connecting")).pepoTitle().lineLimit(1)
                Spacer(minLength: 0)
            case .reconnecting(let pc, _):
                ReconnectingLabel(pcName: pc)
                Spacer(minLength: 0)
            default:
                Text(tr("status_disconnected")).pepoTitle().lineLimit(1)
                Spacer(minLength: 0)
            }
            TextLink(title: tr("exit"), color: Pepo.error, action: onDisconnect)
                .fixedSize()
                .layoutPriority(4)
        }
        .padding(.horizontal, 4)
        .frame(height: height)
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
private struct FaceButtons: View {
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
