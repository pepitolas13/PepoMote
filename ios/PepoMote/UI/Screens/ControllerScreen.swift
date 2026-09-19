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

    init(size: CGSize, extraHeader: CGFloat = 0) {
        grow = UiScale.factor(size, base: UiScale.remoteBase, fixed: UiScale.remoteFixed + extraHeader)
        s = Swift.min(1, (size.height - extraHeader) / 780) * grow
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

/// Mando vertical estilo Wiimote: cruceta, −/diana/+, A, 1/Home/2 (Home fuera
/// del modo puntero), multimedia, B.
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
    /// Dónde está cada botón, para «Pulsar deslizando».
    @StateObject private var press = PressRegistry()

    /// Home del Mando de Wii: conectado y fuera del modo puntero.
    private var showHome: Bool { link.link.connected.map(showHomeButton) ?? false }

    var body: some View {
        GeometryReader { geo in
            let wrapsNunchuk = link.link.connected.map { $0.supportsSwitch && showModeChips($0, showChips) && showNunchukChip($0) } ?? false
            let m = RemoteMetrics(size: geo.size, extraHeader: wrapsNunchuk && geo.size.width < 420 ? 37 : 0)
            ZStack {
                // El primero: recoge los dedos que nacen fuera de los botones
                SlideCanvas()
                // La columna del mando y las tiras comparten anchura
                ZStack {
                    column(m)
                        .padding(.horizontal, 24)
                    HStack {
                        // En Dolphin la precisión no hace nada: la tira es «Acercar»
                        if link.link.connected?.mode == LinkState.modeDolphin {
                            NearStrip(width: m.precisionW, height: m.stripH, glyph: m.text(16))
                                .padding(.leading, 6)
                        } else {
                            PrecisionStrip(width: m.precisionW, height: m.stripH, glyph: m.text(16))
                                .padding(.leading, 6)
                        }
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
        .coordinateSpace(name: PressRegistry.padSpace)
        .environmentObject(press)
        .background(Pepo.background.ignoresSafeArea())
        .sheet(isPresented: $keyboardOpen) {
            KeyboardSheet { keyboardOpen = false }
        }
        // El teclado y la pose congelada van juntos, y se sueltan SOLOS: al
        // cerrarlo, al cambiar de modo (el PC lo cambia por su cuenta al abrir
        // un emulador), al reconectar con un motor nuevo y al salir de la
        // pantalla. Una pose congelada que se olvide dejaría el puntero muerto
        .onChange(of: keyboardOpen) { open in
            LinkState.shared.motion?.pointerHold = open && Route.holdsPointerForKeyboard(link.link)
        }
        .onChange(of: link.link) { current in
            if keyboardOpen && !Route.showsKeyboard(current) { keyboardOpen = false }
            LinkState.shared.motion?.pointerHold = keyboardOpen && Route.holdsPointerForKeyboard(current)
        }
        .onAppear { UIApplication.shared.isIdleTimerDisabled = true }
        .onDisappear {
            UIApplication.shared.isIdleTimerDisabled = false
            press.releaseAll() // nada queda pulsado al salir
            LinkState.shared.motion?.pointerHold = false
        }
    }

    private func column(_ m: RemoteMetrics) -> some View {
        VStack(spacing: 0) {
            Spacer().frame(height: 10)
            header
            if let c = link.link.connected {
                // Chips de modo (Jugador 1 con el ajuste activo, y siempre dentro de
                // Wii U) y, en Dolphin, el interruptor «Nunchuk» a la derecha de Wii U
                // con otro tono: es una opción, no un modo. Todo en UNA fila, lejos
                // de la cruceta (antes el Nunchuk iba en una fila propia justo
                // encima y se pulsaba sin querer): en un iPhone los chips van densos
                // para que quepan los cuatro, y si aun así no caben el texto se encoge
                let modeChips = showModeChips(c, showChips)
                let nunchuk = showNunchukChip(c)
                if modeChips || nunchuk {
                    Spacer().frame(height: 6)
                    let dense = m.colW < 420
                    VStack(spacing: 5) {
                        HStack(spacing: dense ? 8 : 10) {
                            if modeChips { ModeChips(current: c.mode, supportsCemu: c.supportsCemu, supportsSwitch: c.supportsSwitch, supportsRetroArch: c.supportsRetroArch, supportsPointer: c.receiver.supportsPointer, dense: dense) }
                            if nunchuk && !(dense && c.supportsSwitch) { NunchukChip(link: c, dense: dense).padding(.leading, 4) }
                        }
                        if nunchuk && dense && c.supportsSwitch { NunchukChip(link: c, dense: dense) }
                    }
                }
                if isWiiUAsWiimote(c) {
                    Spacer().frame(height: 8)
                    PadSelector(link: c, width: m.colW - 48, help: tr("wii_pad_help"))
                } else if Route.isRetroArch(link.link) {
                    // RetroArch: qué mando soy (RetroPad / NES / pistola) y las teclas rápidas
                    Spacer().frame(height: 8)
                    PadSelector(link: c, width: m.colW - 48, help: tr("retro_pad_help"))
                    Spacer().frame(height: 6)
                    RetroArchHotkeys()
                }
            }
            // Hueco de sobra entre los chips y la cruceta: que ir a por ↑ no toque un chip
            Gap(m.gap(18), flexible: m.flexible)
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
            // 1 · Home · 2, como en el mando + Nunchuk. Home solo fuera del modo
            // puntero (el PC no le da uso): en Dolphin es el menú HOME de la Wii
            // y en Cemu, además, Mario Party 10 lo pide para dar por emparejado
            // cada Mando de Wii emulado
            HStack(spacing: m.spacing) {
                RoundButton(label: "1", size: m.one, bit: Btn.one, textSize: m.text(18))
                if showHome {
                    RoundButton(label: tr("home_btn"), size: m.one, bit: Btn.home, textSize: m.text(12))
                } else if Route.showsKeyboard(link.link) {
                    // Modo puntero: Home no hace nada y su hueco queda libre,
                    // centrado bajo la A y lejos de la cruceta y de B. No es un
                    // botón del mando (no entra en el registro de pulsación):
                    // con «pulsar deslizando» un dedo de paso lo abriría
                    KeyboardButton(compact: true) {
                        openKeyboard(link.link, press)
                        keyboardOpen = true
                    }
                    .fixedSize()
                }
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
                        if c.mode == LinkState.modeRetroArch { return tr("mode_retroarch") }
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
            // Modo Wii U: texto para el teclado en pantalla de Cemu; en
            // RetroArch, para su ventana. En modo puntero el botón NO va aquí
            // arriba sino en el hueco de Home, al alcance del pulgar
            if Route.showsKeyboard(link.link), showHome {
                KeyboardButton(compact: true) {
                    openKeyboard(link.link, press)
                    keyboardOpen = true
                }
                .fixedSize()
                .layoutPriority(2)
            }
            TextLink(title: tr("exit"), color: Pepo.error, action: onDisconnect)
                .fixedSize()
                .layoutPriority(3)
        }
    }
}
