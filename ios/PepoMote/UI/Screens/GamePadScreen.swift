import SwiftUI
import UIKit

/// GamePad de Wii U (modo Cemu), siempre apaisado. Como el mando real: L/ZL
/// arriba a la izquierda y R/ZR a la derecha; stick izquierdo y cruceta a la
/// izquierda; stick derecho y rombo A/B/X/Y a la derecha; la pantalla táctil
/// en el centro con −, Home, +, TV/Pad y Soplar debajo. Todo escalado al
/// tamaño real de la pantalla. Como Pro Controller (jugadores 2-4) no hay
/// táctil, ni Soplar, ni TV/Pad.
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
    @Environment(\.displayScale) private var displayScale
    @State private var keyboardOpen = false
    @State private var rotation: Int = OrientationLock.frameRotation(OrientationLock.current)

    var body: some View {
        let connected = link.link.connected
        let operative = Route.isGamePad(link.link)
        let pro = connected?.pad == LinkState.padPro
        return GeometryReader { geo in
            let w = geo.size.width
            let h = geo.size.height
            let gap: CGFloat = 6
            let headerH: CGFloat = 38
            let selectorH: CGFloat = 36
            let bodyH = h - headerH - selectorH - gap * 3
            let sideW = w * 0.29
            let shoulderH = min(max(bodyH * 0.09, 26), 40)
            let shoulderW = min(max(sideW * 0.6, 90), 150)
            // Stick y cruceta (o rombo) se reparten lo que queda bajo los gatillos
            let padSize = min((bodyH - shoulderH * 2 - gap * 3) / 2, sideW * 0.62, 200)
            let clickSize = min(max(padSize * 0.30, 30), 44)
            let faceBtn = padSize / 2.6
            let centerW = w - sideW * 2 - gap * 2
            let bottomRowH = min(max(bodyH * 0.16, 44), 60)
            let touchW = min(centerW, (bodyH - bottomRowH - gap * 2) * (16 / 9))
            let touchH = touchW * (9 / 16)
            // Doble pantalla: solo el GamePad (no un Pro Controller) y con el modo confirmado
            let wantScreen = operative && connected?.pad == LinkState.padGamepad
            let touchPx = Int((touchW * displayScale).rounded())

            ZStack(alignment: .top) {
                VStack(spacing: 0) {
                    GamePadHeader(
                        link: link.link, operative: operative, height: headerH, width: w,
                        screen: screenLink.client,
                        onKeyboard: operative ? { keyboardOpen = true } : nil,
                        onDisconnect: onDisconnect
                    )
                    Spacer().frame(height: gap)
                    // Qué mando soy en Cemu: debajo de la cabecera, centrado
                    ZStack {
                        if let c = connected { PadSelector(link: c, width: w, compact: true) }
                    }
                    .frame(height: selectorH)
                    Spacer().frame(height: gap)

                    HStack(spacing: gap) {
                        // Izquierda: L y ZL en la esquina, stick (+ L3), cruceta
                        VStack(alignment: .leading, spacing: 0) {
                            VStack(alignment: .leading, spacing: gap) {
                                ShoulderButton(label: "L", bit: Btn.l, width: shoulderW, height: shoulderH)
                                ShoulderButton(label: "ZL", bit: Btn.zl, width: shoulderW, height: shoulderH)
                            }
                            Spacer(minLength: 0)
                            HStack(alignment: .bottom, spacing: gap) {
                                AnalogStick(size: padSize) { x, y in ButtonState.shared.setStick(x, y) }
                                RoundButton(label: "L3", size: clickSize, bit: Btn.stickL, textSize: 12)
                            }
                            Spacer(minLength: 0)
                            PadCross(size: padSize)
                        }
                        .frame(width: sideW, height: bodyH)

                        // Centro: pantalla táctil y la fila − · Home · + (con TV/Pad y Soplar)
                        VStack(spacing: 0) {
                            Spacer(minLength: 0)
                            if pro {
                                Text(tr("pro_controller")).pepoBody().multilineTextAlignment(.center)
                            } else {
                                TouchScreenView(width: touchW, height: touchH, client: screenLink.client)
                            }
                            Spacer(minLength: 0)
                            // La fila entera tiene que caber en el centro: pastillas y círculos se encogen juntos
                            let rowGap: CGFloat = centerW < 300 ? 6 : 10
                            let pillW: CGFloat = pro ? 0 : min(max(centerW * 0.22, 44), 66)
                            let pills: CGFloat = pro ? 0 : 2
                            let roundBtn = min(max((centerW - rowGap * (2 + pills) - pillW * pills) / 3, 28), bottomRowH * 0.85)
                            HStack(alignment: .center, spacing: rowGap) {
                                if !pro { ShoulderButton(label: tr("tv_pad"), bit: Btn.screen, width: pillW, height: 30, textSize: 12) }
                                RoundButton(label: "−", size: roundBtn, bit: Btn.minus, textSize: 19)
                                RoundButton(label: tr("home_btn"), size: roundBtn, bit: Btn.home, textSize: 12)
                                RoundButton(label: "+", size: roundBtn, bit: Btn.plus, textSize: 19)
                                if !pro { ShoulderButton(label: tr("blow"), bit: Btn.mic, width: pillW, height: 30, textSize: 12) }
                            }
                            Spacer(minLength: 0)
                        }
                        .frame(width: centerW, height: bodyH)

                        // Derecha: R y ZR en la esquina, (R3 +) stick, rombo A/B/X/Y
                        VStack(alignment: .trailing, spacing: 0) {
                            VStack(alignment: .trailing, spacing: gap) {
                                ShoulderButton(label: "R", bit: Btn.r, width: shoulderW, height: shoulderH)
                                ShoulderButton(label: "ZR", bit: Btn.zr, width: shoulderW, height: shoulderH)
                            }
                            Spacer(minLength: 0)
                            HStack(alignment: .bottom, spacing: gap) {
                                RoundButton(label: "R3", size: clickSize, bit: Btn.stickR, textSize: 12)
                                AnalogStick(size: padSize) { x, y in ButtonState.shared.setStick2(x, y) }
                            }
                            Spacer(minLength: 0)
                            FaceButtons(size: padSize, btn: faceBtn)
                        }
                        .frame(width: sideW, height: bodyH)
                    }
                    .frame(height: bodyH)
                    // Inerte hasta que el receptor confirme el modo: nada llega a los controles
                    .opacity(operative ? 1 : 0.4)
                    .allowsHitTesting(operative)
                }

                NoticeBanner().padding(.top, headerH + selectorH + gap * 2)
            }
            .frame(width: w, height: h)
            .onChange(of: wantScreen) { want in
                if want { screenLink.request(width: min(ScreenClient.nativeWidth, touchPx), height: min(ScreenClient.nativeWidth, touchPx) * 9 / 16) } else { screenLink.release() }
            }
            .onChange(of: touchPx) { px in
                if wantScreen { screenLink.request(width: min(ScreenClient.nativeWidth, px), height: min(ScreenClient.nativeWidth, px) * 9 / 16) }
            }
            .onAppear {
                if wantScreen { screenLink.request(width: min(ScreenClient.nativeWidth, touchPx), height: min(ScreenClient.nativeWidth, touchPx) * 9 / 16) }
            }
        }
        .background(Pepo.background.ignoresSafeArea())
        .sheet(isPresented: $keyboardOpen) {
            KeyboardSheet { keyboardOpen = false }
        }
        // Solo con el modo confirmado el motor emite como GamePad; si el modo se
        // va (Mando de Wii) o se sale, vuelve lo de antes
        .onChange(of: operative) { _ in applyEngine(operative) }
        .onChange(of: rotation) { _ in applyEngine(operative) }
        .onReceive(NotificationCenter.default.publisher(for: UIDevice.orientationDidChangeNotification)) { _ in
            // El móvil puede girar 180° entre los dos apaisados: el remapeo lo sigue
            rotation = OrientationLock.frameRotation(OrientationLock.current)
        }
        .onAppear {
            UIApplication.shared.isIdleTimerDisabled = true
            rotation = OrientationLock.frameRotation(OrientationLock.current)
            applyEngine(operative)
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

    private func applyEngine(_ operative: Bool) {
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

    var body: some View {
        // El estado se acorta antes de recortarse: en móviles estrechos fuera el nombre del mando y el RTT
        let showPad = width >= 560
        let showRtt = width >= 700
        return HStack(alignment: .center, spacing: 8) {
            switch link {
            case .connected(let c):
                Text(c.pcName)
                    .pepoTitle()
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: 160, alignment: .leading)
                    .layoutPriority(0)
                let padName = c.pad == LinkState.padPro ? tr("pro_controller") : tr("gamepad")
                let status: String = {
                    if !operative { return tr("activating_wiiu") }
                    var s = "J\(c.player)"
                    if showPad { s += " · " + padName }
                    if showRtt, let rtt = c.rttMs { s += " · \(String(format: "%.0f", rtt)) ms" }
                    return s
                }()
                Text(status)
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

/// Rombo: X arriba, Y izquierda, A derecha (azul, destacado), B abajo.
private struct FaceButtons: View {
    let size: CGFloat
    let btn: CGFloat

    var body: some View {
        let off = (size - btn) / 2
        return ZStack {
            RoundButton(label: "X", size: btn, bit: Btn.x, textSize: 18).offset(y: -off)
            RoundButton(label: "Y", size: btn, bit: Btn.y, textSize: 18).offset(x: -off)
            RoundButton(label: "A", size: btn, bit: Btn.a, background: Pepo.blue, pressedColor: Pepo.blueHover, textColor: Pepo.onAccent, textSize: 20, pop: true).offset(x: off)
            RoundButton(label: "B", size: btn, bit: Btn.b, textSize: 18).offset(y: off)
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
        let fx = min(max(p.x / width, 0), 1)
        let fy = min(max(p.y / height, 0), 1)
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
