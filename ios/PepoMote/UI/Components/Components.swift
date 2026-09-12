import SwiftUI

// MARK: - Gesto de pulsación

/// Pulsación momentánea que sigue al dedo hasta que lo levanta, aunque salga
/// de la vista (como `drag` en Android: nada se queda pulsado). `onUp`
/// también si la vista desaparece con el dedo puesto.
struct HoldGesture: ViewModifier {
    let onDown: () -> Void
    let onUp: () -> Void
    @State private var down = false

    func body(content: Content) -> some View {
        content
            .gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .local)
                    .onChanged { _ in
                        if !down {
                            down = true
                            onDown()
                        }
                    }
                    .onEnded { _ in
                        if down {
                            down = false
                            onUp()
                        }
                    }
            )
            .onDisappear {
                if down {
                    down = false
                    onUp()
                }
            }
    }
}

extension View {
    func holdGesture(onDown: @escaping () -> Void, onUp: @escaping () -> Void) -> some View {
        modifier(HoldGesture(onDown: onDown, onUp: onUp))
    }
}

// MARK: - Texto que se encoge hasta caber

/// Texto de una sola línea que se encoge hasta caber (nunca se corta):
/// etiquetas de botones y del selector en móviles pequeños.
struct FitText: View {
    let text: String
    var size: CGFloat = 16
    var weight: PepoFont.Weight = .bold
    var color: Color = Pepo.text

    init(_ text: String, size: CGFloat = 16, weight: PepoFont.Weight = .bold, color: Color = Pepo.text) {
        self.text = text
        self.size = size
        self.weight = weight
        self.color = color
    }

    var body: some View {
        Text(text)
            .font(PepoFont.nunito(size, weight))
            .foregroundColor(color)
            .lineLimit(1)
            .minimumScaleFactor(0.45)
            .allowsTightening(true)
    }
}

// MARK: - Botones momentáneos

/// Botón circular momentáneo: mantiene el bit activo mientras está pulsado.
/// Háptica y sonido en cada pulsación.
struct RoundButton: View {
    let label: String
    let size: CGFloat
    let bit: UInt32
    var background: Color = Pepo.card
    var pressedColor: Color = Pepo.glow
    var textColor: Color = Pepo.text
    var textSize: CGFloat = 20
    var pop = false
    @State private var down = false

    var body: some View {
        ZStack {
            Circle()
                .fill(down ? pressedColor : background)
                .shadow(color: Color.black.opacity(down ? 0.06 : 0.16), radius: down ? 1 : 6, x: 0, y: down ? 1 : 3)
            FitText(label, size: textSize, weight: .black, color: textColor)
                .padding(.horizontal, 3)
        }
        .frame(width: size, height: size)
        .scaleEffect(down ? 0.90 : 1)
        .animation(.easeOut(duration: 0.08), value: down)
        .contentShape(Circle())
        .holdGesture(
            onDown: {
                down = true
                ButtonState.shared.set(bit, true)
                Haptics.tap()
                if pop { UiSounds.shared.pop() } else { UiSounds.shared.blip() }
            },
            onUp: {
                down = false
                ButtonState.shared.set(bit, false)
            }
        )
    }
}

/// Botón rectangular momentáneo: los gatillos L/ZL/R/ZR del GamePad y las
/// pastillas pequeñas TV/Pad y Soplar.
struct ShoulderButton: View {
    let label: String
    let bit: UInt32
    let width: CGFloat
    let height: CGFloat
    var textSize: CGFloat = 16
    @State private var down = false

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: height / 2.5, style: .continuous)
        return ZStack {
            shape
                .fill(down ? Pepo.glow : Pepo.card)
                .shadow(color: Color.black.opacity(down ? 0.06 : 0.14), radius: down ? 1 : 4, x: 0, y: down ? 1 : 2)
            FitText(label, size: textSize, weight: .black)
                .padding(.horizontal, 4)
        }
        .frame(width: width, height: height)
        .scaleEffect(down ? 0.94 : 1)
        .animation(.easeOut(duration: 0.08), value: down)
        .contentShape(shape)
        .holdGesture(
            onDown: {
                down = true
                ButtonState.shared.set(bit, true)
                Haptics.tap()
                UiSounds.shared.blip()
            },
            onUp: {
                down = false
                ButtonState.shared.set(bit, false)
            }
        )
    }
}

/// Zona-gatillo: banda ancha a todo el ancho, como el gatillo trasero del
/// Wiimote. Por defecto es B; el Nunchuk la usa para Z (azul) y para C.
struct TriggerZone: View {
    var bit: UInt32 = Btn.b
    var label: String = "B"
    var height: CGFloat = 88
    var background: Color = Pepo.blue
    var pressedColor: Color = Pepo.blueHover
    var textColor: Color = Pepo.onAccent
    @State private var down = false

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .fill(down ? pressedColor : background)
            Text(label)
                .font(PepoFont.headline())
                .foregroundColor(textColor)
        }
        .frame(maxWidth: .infinity)
        .frame(height: height)
        .contentShape(Rectangle())
        .holdGesture(
            onDown: {
                down = true
                ButtonState.shared.set(bit, true)
                Haptics.tap()
                UiSounds.shared.blip()
            },
            onUp: {
                down = false
                ButtonState.shared.set(bit, false)
            }
        )
    }
}

/// Cruceta: cuatro brazos momentáneos. En modo puntero el receptor hace
/// ↑/↓ = flechas del PC y ←/→ = atrás/adelante del navegador.
struct PadCross: View {
    let size: CGFloat

    var body: some View {
        let arm = size / 3
        return ZStack {
            PadArm(label: "▲", arm: arm, bit: Btn.dpadUp).offset(y: -arm)
            PadArm(label: "▼", arm: arm, bit: Btn.dpadDown).offset(y: arm)
            PadArm(label: "◀", arm: arm, bit: Btn.dpadLeft).offset(x: -arm)
            PadArm(label: "▶", arm: arm, bit: Btn.dpadRight).offset(x: arm)
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(Pepo.card)
                .frame(width: arm, height: arm)
        }
        .frame(width: size, height: size)
    }
}

private struct PadArm: View {
    let label: String
    let arm: CGFloat
    let bit: UInt32
    @State private var down = false

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(down ? Pepo.glow : Pepo.card)
            Text(label)
                .font(.system(size: 14))
                .foregroundColor(Pepo.textDim)
        }
        .frame(width: arm, height: arm)
        .contentShape(Rectangle())
        .holdGesture(
            onDown: {
                down = true
                ButtonState.shared.set(bit, true)
                Haptics.tap()
                UiSounds.shared.blip()
            },
            onUp: {
                down = false
                ButtonState.shared.set(bit, false)
            }
        )
    }
}

// MARK: - Stick analógico

/// Stick analógico virtual. El pomo sigue al pulgar dentro del círculo
/// (tocar el borde = tope) y al soltar vuelve al centro con un muelle.
/// Salida −127..127, +y arriba, zona muerta pequeña (StickMap); tic háptico
/// al salir del centro.
struct AnalogStick: View {
    let size: CGFloat
    let onStick: (Int, Int) -> Void
    @State private var active = false
    @State private var knob: CGSize = .zero
    @State private var centered = true

    var body: some View {
        let knobSize = size * 0.46
        let travel = (size - knobSize) / 2
        return ZStack {
            Circle().fill(Pepo.card)
            Circle().stroke(active ? Pepo.glow : Pepo.cardBorder, lineWidth: 1.5)
            // Marcas de los cuatro ejes: orientan el pulgar sin mirar
            ForEach(0..<4, id: \.self) { i in
                let a = CGFloat(i) * .pi / 2
                Circle()
                    .fill(Pepo.cardBorder)
                    .frame(width: 6, height: 6)
                    .offset(x: cos(a) * size * 0.42, y: sin(a) * size * 0.42)
            }
            ZStack {
                Circle()
                    .fill(active ? Pepo.blueHover : Pepo.blue)
                    .shadow(color: Color.black.opacity(0.18), radius: active ? 2 : 6, x: 0, y: 2)
                Circle()
                    .fill(Pepo.glow)
                    .frame(width: knobSize * 0.28, height: knobSize * 0.28)
            }
            .frame(width: knobSize, height: knobSize)
            .offset(knob)
            .animation(active ? nil : Animation.spring(response: 0.25, dampingFraction: 0.65), value: knob)
        }
        .frame(width: size, height: size)
        .contentShape(Circle())
        .gesture(
            DragGesture(minimumDistance: 0, coordinateSpace: .local)
                .onChanged { v in
                    if !active { active = true }
                    let dx = v.location.x - size / 2
                    let dy = v.location.y - size / 2
                    let len = (dx * dx + dy * dy).squareRoot()
                    if len > travel {
                        knob = CGSize(width: dx * travel / len, height: dy * travel / len)
                    } else {
                        knob = CGSize(width: dx, height: dy)
                    }
                    let (x, y) = StickMap.map(Float(dx), Float(dy), radius: Float(travel))
                    let atCenter = x == 0 && y == 0
                    if centered, !atCenter { Haptics.tick() }
                    centered = atCenter
                    onStick(x, y)
                }
                .onEnded { _ in
                    active = false
                    knob = .zero
                    centered = true
                    onStick(0, 0)
                }
        )
        .onDisappear { onStick(0, 0) }
    }
}

// MARK: - Avisos y estado

/// Punto que late (algo está en marcha: conectando, reconectando…).
struct PulsingDot: View {
    let color: Color
    var size: CGFloat = 10
    @State private var on = false

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: size, height: size)
            .opacity(on ? 1 : 0.35)
            .onAppear {
                withAnimation(.easeInOut(duration: 0.7).repeatForever(autoreverses: true)) { on = true }
            }
    }
}

/// «Reconectando con X…» con su punto latiendo, para las cabeceras del mando.
struct ReconnectingLabel: View {
    let pcName: String
    var font: Font = PepoFont.titleMedium()

    var body: some View {
        HStack(spacing: 8) {
            PulsingDot(color: Pepo.warn)
            Text(tr("status_reconnecting", pcName))
                .font(font)
                .foregroundColor(Pepo.text)
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }
}

/// Aviso transitorio del receptor (`notice`): se enseña ~6 s desde que llegó,
/// en la pantalla que esté. Un aviso ya caducado no se muestra.
struct NoticeBanner: View {
    @ObservedObject var link = LinkState.shared
    @State private var shown: Notice?

    var body: some View {
        Group {
            if let n = shown {
                Text(n.text)
                    .font(PepoFont.bodyMedium())
                    .foregroundColor(Pepo.text)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                    .frame(maxWidth: 440)
                    .pepoCard(border: Pepo.warn, radius: 14)
            }
        }
        .onReceive(link.$notice) { n in
            guard let n else { return }
            let remaining = 6000 - (LinkState.nowMs() - n.atMs)
            if remaining <= 0 { return }
            shown = n
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(Int(remaining))) {
                if shown == n { shown = nil }
            }
        }
    }
}

/// Nombre del modo del receptor para las cabeceras.
func modeLabel(_ mode: String) -> String {
    switch mode {
    case LinkState.modeDolphin: return tr("mode_dolphin")
    case LinkState.modeCemu: return tr("mode_wiiu")
    default: return tr("mode_pointer")
    }
}

/// Modo Wii U actuando como Mando de Wii (layouts Wii de siempre, 72 bytes).
func isWiiUAsWiimote(_ c: ConnectedLink) -> Bool {
    c.mode == LinkState.modeCemu && c.pad == LinkState.padWiimote
}

/// Chips de modo: solo el Jugador 1; con el ajuste activo o, siempre, dentro de Wii U.
func showModeChips(_ c: ConnectedLink, _ showChips: Bool) -> Bool {
    c.slot == 0 && (showChips || c.mode == LinkState.modeCemu)
}

// MARK: - Chips de modo y botón Teclado

/// Selector de modo: Puntero (controla el PC) / Dolphin (Wiimote virtual) /
/// Wii U (GamePad para Cemu, solo si el receptor lo soporta).
struct ModeChips: View {
    let current: String
    let supportsCemu: Bool
    var compact = false

    var body: some View {
        HStack(spacing: compact ? 6 : 10) {
            ModeChip(label: tr("mode_pointer"), selected: current == LinkState.modePointer, compact: compact) {
                LinkState.shared.requestMode(LinkState.modePointer)
            }
            ModeChip(label: tr("mode_dolphin"), selected: current == LinkState.modeDolphin, compact: compact) {
                LinkState.shared.requestMode(LinkState.modeDolphin)
            }
            if supportsCemu {
                ModeChip(label: tr("mode_wiiu"), selected: current == LinkState.modeCemu, compact: compact) {
                    LinkState.shared.requestMode(LinkState.modeCemu)
                }
            }
        }
    }
}

struct ModeChip: View {
    let label: String
    let selected: Bool
    var compact = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(PepoFont.bodyMedium())
                .foregroundColor(selected ? Pepo.card : Pepo.textDim)
                .lineLimit(1)
                .padding(.horizontal, compact ? 12 : 18)
                .padding(.vertical, compact ? 6 : 8)
                .background(selected ? Pepo.blue : Pepo.card)
                .clipShape(Capsule())
        }
        .buttonStyle(.plain)
    }
}

/// Botón pequeño «Teclado» de las cabeceras en modo Wii U: abre `KeyboardDialog`.
struct KeyboardButton: View {
    var compact = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(tr("keyboard"))
                .font(PepoFont.bodyMedium())
                .foregroundColor(Pepo.text)
                .lineLimit(1)
                .padding(.horizontal, compact ? 12 : 16)
                .padding(.vertical, compact ? 6 : 8)
                .background(Pepo.card)
                .clipShape(Capsule())
                .overlay(Capsule().stroke(Pepo.cardBorder, lineWidth: 1.5))
        }
        .buttonStyle(.plain)
    }
}

/// Diálogo «Teclado» del modo Wii U: el texto se escribe aquí y se manda por
/// el canal de control. Qué se manda en cada caso lo decide `TextInput`.
struct KeyboardDialog: View {
    let onSend: (String) -> Void
    let onClose: () -> Void
    @State private var field = ""
    @FocusState private var focused: Bool

    private func apply(_ a: TextInput.Action) {
        if let s = a.send { onSend(s) }
        field = a.field
        if a.close { onClose() }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(tr("kb_title")).font(PepoFont.titleMedium()).foregroundColor(Pepo.text)
            Text(tr("kb_help")).pepoBody()
            TextField(tr("kb_placeholder"), text: $field)
                .textFieldStyle(.roundedBorder)
                .focused($focused)
                .submitLabel(.send)
                .disableAutocorrection(true)
                .onSubmit { apply(TextInput.accept(field)) }
            HStack(spacing: 4) {
                TextLink(title: tr("kb_delete"), color: Pepo.text) { apply(TextInput.delete(field)) }
                TextLink(title: tr("kb_write"), color: Pepo.text) { apply(TextInput.write(field)) }
                Spacer()
            }
            HStack(spacing: 8) {
                Spacer()
                TextLink(title: tr("kb_close")) { apply(TextInput.close(field)) }
                Button(action: { apply(TextInput.accept(field)) }) {
                    Text(tr("kb_accept"))
                        .font(PepoFont.labelLarge())
                        .foregroundColor(Pepo.onAccent)
                        .padding(.horizontal, 20)
                        .padding(.vertical, 10)
                        .background(Pepo.blue)
                        .clipShape(Capsule())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(18)
        .frame(maxWidth: 440)
        .pepoCard()
        .padding(16)
        .onAppear {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { focused = true }
        }
    }
}

/// Hoja que presenta el teclado de Cemu sobre la pantalla actual.
struct KeyboardSheet: View {
    let onClose: () -> Void

    var body: some View {
        ZStack {
            Pepo.background.ignoresSafeArea()
            VStack {
                KeyboardDialog(onSend: { LinkState.shared.sendText?($0) }, onClose: onClose)
                Spacer()
            }
            .padding(.top, 12)
        }
    }
}

// MARK: - Selector «En Cemu soy»

/// «En Cemu soy: [ GamePad ] [ Mando de Wii ]»: qué mando es este móvil
/// dentro del modo Wii U. Segmento activo relleno en azul; el pedido y aún sin
/// eco, a medio tono. Tocar envía `pad wiimote` / `pad gamepad`.
struct PadSelector: View {
    let link: ConnectedLink
    let width: CGFloat
    var compact = false
    var help: String? = nil
    @State private var pending: String?

    private static let echoTimeoutMs = 2000

    var body: some View {
        let wiimote = link.pad == LinkState.padWiimote
        let inlinePrefix = width >= 430
        let prefixW: CGFloat = inlinePrefix ? 110 : 0
        let segmentsW = min(max(width - prefixW - 24, 160), compact ? 330 : 360)
        return VStack(spacing: 2) {
            if !inlinePrefix {
                Text(tr("in_cemu"))
                    .font(PepoFont.nunito(11, .regular))
                    .foregroundColor(Pepo.textDim)
                    .lineLimit(1)
            }
            HStack(spacing: compact ? 8 : 10) {
                if inlinePrefix {
                    Text(tr("in_cemu"))
                        .font(PepoFont.bodyMedium())
                        .foregroundColor(Pepo.text)
                        .lineLimit(1)
                }
                HStack(spacing: 3) {
                    Segment(
                        label: link.player == 1 ? tr("gamepad") : tr("pro_controller"),
                        selected: !wiimote,
                        pending: pending == LinkState.padGamepad,
                        compact: compact
                    ) {
                        if wiimote {
                            pending = LinkState.padGamepad
                            LinkState.shared.sendPad?(LinkState.padGamepad)
                        }
                    }
                    Segment(
                        label: tr("wii_remote"),
                        selected: wiimote,
                        pending: pending == LinkState.padWiimote,
                        compact: compact
                    ) {
                        if !wiimote {
                            pending = LinkState.padWiimote
                            LinkState.shared.sendPad?(LinkState.padWiimote)
                        }
                    }
                }
                .padding(3)
                .frame(width: segmentsW)
                .background(Pepo.card)
                .clipShape(Capsule())
                .overlay(Capsule().stroke(Pepo.cardBorder, lineWidth: 1.5))
            }
            if let help {
                Text(help)
                    .pepoBody()
                    .multilineTextAlignment(.center)
                    .padding(.top, 4)
                    .padding(.horizontal, 12)
            }
        }
        // El eco de `pad` (cambie o no el mando) cierra la espera…
        .onChange(of: link.pad) { _ in pending = nil }
        // …y si no llega (PC antiguo), se deja de esperar
        .onChange(of: pending) { p in
            guard let p else { return }
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(PadSelector.echoTimeoutMs)) {
                if pending == p { pending = nil }
            }
        }
    }

    private struct Segment: View {
        let label: String
        let selected: Bool
        let pending: Bool
        let compact: Bool
        let action: () -> Void

        var body: some View {
            let bg: Color = selected ? Pepo.blue : (pending ? Pepo.glow : Pepo.card)
            let fg: Color = selected ? Pepo.onAccent : (pending ? Pepo.text : Pepo.textDim)
            return Button(action: action) {
                FitText(label, size: 15, weight: .bold, color: fg)
                    .padding(.horizontal, compact ? 8 : 12)
                    .padding(.vertical, compact ? 6 : 8)
                    .frame(maxWidth: .infinity)
                    .background(bg)
                    .clipShape(Capsule())
            }
            .buttonStyle(.plain)
        }
    }
}

// MARK: - Precisión y scroll

/// Mirilla de francotirador: aro, cuatro marcas que lo cruzan y punto central.
struct CrosshairGlyph: View {
    let color: Color
    let size: CGFloat

    var body: some View {
        Canvas { ctx, sz in
            let px = min(sz.width, sz.height)
            let c = CGPoint(x: sz.width / 2, y: sz.height / 2)
            let stroke = px * 0.11
            var ring = Path()
            ring.addArc(center: c, radius: px * 0.30, startAngle: .zero, endAngle: .degrees(360), clockwise: false)
            ctx.stroke(ring, with: .color(color), lineWidth: stroke)
            let dirs: [(CGFloat, CGFloat)] = [(0, -1), (0, 1), (-1, 0), (1, 0)]
            for (dx, dy) in dirs {
                var p = Path()
                p.move(to: CGPoint(x: c.x + dx * px * 0.19, y: c.y + dy * px * 0.19))
                p.addLine(to: CGPoint(x: c.x + dx * px * 0.47, y: c.y + dy * px * 0.47))
                ctx.stroke(p, with: .color(color), style: StrokeStyle(lineWidth: stroke, lineCap: .round))
            }
            let dot = px * 0.06
            ctx.fill(Path(ellipseIn: CGRect(x: c.x - dot, y: c.y - dot, width: dot * 2, height: dot * 2)), with: .color(color))
        }
        .frame(width: size, height: size)
    }
}

/// Mantener = bit de precisión (el puntero del PC va al 40 %); háptico y tic
/// al activar. Sigue activo aunque el dedo se salga. Solo en modo puntero.
struct PrecisionHold: ViewModifier {
    @Binding var active: Bool

    func body(content: Content) -> some View {
        content.holdGesture(
            onDown: {
                active = true
                ButtonState.shared.set(Btn.precision, true)
                Haptics.tap()
                UiSounds.shared.tick()
            },
            onUp: {
                active = false
                ButtonState.shared.set(Btn.precision, false)
            }
        )
    }
}

/// Tira de precisión del borde izquierdo (espejo de la de scroll).
struct PrecisionStrip: View {
    let width: CGFloat
    let height: CGFloat
    @State private var active = false

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 15, style: .continuous)
                .fill(active ? Pepo.glow : Pepo.cardBorder)
            CrosshairGlyph(color: Pepo.textDim, size: 16)
        }
        .frame(width: width, height: height)
        .contentShape(Rectangle())
        .modifier(PrecisionHold(active: $active))
    }
}

/// Píldora «Precisión» del mando apaisado (mismo gesto: mantener).
struct PrecisionPill: View {
    @State private var active = false

    var body: some View {
        HStack(spacing: 8) {
            CrosshairGlyph(color: Pepo.textDim, size: 14)
            Text(tr("precision")).pepoBody()
        }
        .padding(.horizontal, 14)
        .frame(height: 34)
        .background(active ? Pepo.glow : Pepo.cardBorder)
        .clipShape(Capsule())
        .contentShape(Capsule())
        .modifier(PrecisionHold(active: $active))
    }
}

/// Tira de scroll del borde derecho: arrastra para hacer scroll en el PC.
/// Los puntos se convierten a píxeles (como en Android) para que la misma
/// distancia de dedo dé el mismo scroll.
struct ScrollStrip: View {
    let width: CGFloat
    let height: CGFloat
    @State private var active = false
    @State private var lastY: CGFloat?

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 15, style: .continuous)
                .fill(active ? Pepo.glow : Pepo.cardBorder)
            VStack(spacing: 6) {
                ForEach(0..<3, id: \.self) { _ in
                    Circle().fill(Pepo.textDim).frame(width: 6, height: 6)
                }
            }
        }
        .frame(width: width, height: height)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0, coordinateSpace: .local)
                .onChanged { v in
                    active = true
                    if let last = lastY {
                        // dedo hacia arriba (delta negativo) = scroll up = positivo
                        let dyPx = (last - v.location.y) * UIScreen.main.scale
                        ButtonState.shared.addScroll(Int(dyPx.rounded()))
                    }
                    lastY = v.location.y
                }
                .onEnded { _ in
                    active = false
                    lastY = nil
                }
        )
    }
}

/// Diana de recentrado: mantener 150 ms → vibra y recentra.
struct RecenterButton: View {
    var size: CGFloat = 64
    @State private var down = false
    @State private var work: DispatchWorkItem?

    var body: some View {
        ZStack {
            Circle().fill(down ? Pepo.glow : Pepo.card)
            Circle().fill(Pepo.background).frame(width: size * 0.41, height: size * 0.41)
            Circle().fill(Pepo.blue).frame(width: size * 0.16, height: size * 0.16)
        }
        .frame(width: size, height: size)
        .contentShape(Circle())
        .holdGesture(
            onDown: {
                down = true
                let w = DispatchWorkItem {
                    ButtonState.shared.bumpRecenter()
                    Haptics.longPress()
                    UiSounds.shared.tick()
                }
                work = w
                DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(150), execute: w)
            },
            onUp: {
                down = false
                work?.cancel()
                work = nil
            }
        )
    }
}

/// Fila multimedia plegable.
struct MediaRow: View {
    var buttonSize: CGFloat = 46
    @State private var expanded = false

    var body: some View {
        VStack(spacing: 4) {
            TextLink(title: expanded ? tr("media_open") : tr("media_closed")) {
                expanded.toggle()
            }
            if expanded {
                HStack(spacing: 12) {
                    RoundButton(label: "⏮", size: buttonSize, bit: Btn.mediaPrev, textSize: 16)
                    RoundButton(label: "🔉", size: buttonSize, bit: Btn.mediaVolDown, textSize: 16)
                    RoundButton(label: "⏯", size: buttonSize, bit: Btn.mediaPlayPause, textSize: 16)
                    RoundButton(label: "🔇", size: buttonSize, bit: Btn.mediaMute, textSize: 16)
                    RoundButton(label: "🔊", size: buttonSize, bit: Btn.mediaVolUp, textSize: 16)
                    RoundButton(label: "⏭", size: buttonSize, bit: Btn.mediaNext, textSize: 16)
                }
            }
        }
    }
}
