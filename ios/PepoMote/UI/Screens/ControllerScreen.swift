import SwiftUI

/// Marco del mando vertical para un tamaño de pantalla: la escala de iPad
/// (`grow` > 1, UiScale: crece con la pantalla; en cualquier iPhone, 1), la
/// anchura de la columna del mando (las tiras la abrazan: en un iPad no se
/// van a los bordes) y las tiras. Lo que va DENTRO de la columna lo mide
/// `RemoteMetrics` con el alto que de verdad queda bajo la cabecera.
struct RemoteFrame {
    let grow: CGFloat
    let colW: CGFloat
    let flexible: Bool
    let stripH: CGFloat
    let precisionW: CGFloat
    let scrollW: CGFloat

    init(size: CGSize) {
        grow = UiScale.factor(size, base: UiScale.remoteBase, fixed: UiScale.remoteFixed)
        colW = Swift.min(size.width, 520 * grow)
        flexible = grow > 1
        stripH = size.height * 0.45
        precisionW = 34 * grow
        scrollW = 24 * grow
    }

    /// Texto: crece con el iPad; en un iPhone no cambia.
    func text(_ base: CGFloat) -> CGFloat { base * grow }
}

/// Medidas (pt) del cuerpo del mando vertical (cruceta, −/diana/+, A,
/// 1/Home/2, multimedia y B) para el alto que queda bajo la cabecera,
/// `bodyH` (margen, cabecera, chips y selector ya descontados: los mide
/// SwiftUI). En un iPhone bajo todo lo que escala se encoge a la vez
/// (`s` < 1) hasta que quepa, como en Android (WiiRemoteMetrics); en
/// cualquier otro iPhone las medidas de siempre (`s` = 1, exacto); en un iPad
/// todo crece a la vez (`grow` > 1) y nunca encoge, aunque la cabecera sea
/// alta, y los huecos se vuelven flexibles para repartir la holgura vertical
/// entre todos en vez de dejarla en uno solo.
struct RemoteMetrics {
    /// Margen superior de la columna, encima de la cabecera.
    static let top: CGFloat = 10
    /// Botón «Multimedia ▼» (TextLink: 14 pt de letra + 8 + 8): no escala.
    static let mediaButton: CGFloat = 36
    /// Hueco mínimo entre multimedia y la B.
    static let gapMin: CGFloat = 4
    /// Margen bajo la B.
    static let bottom: CGFloat = 12
    /// Huecos 18+16+16+14+10 + cruceta 168 + fila 64 + A 148 + 1/2 52 + B 88.
    static let scalable: CGFloat = 594
    /// Fila multimedia desplegada: botones de 46 (escalan) bajo un hueco de 4.
    static let mediaRow: CGFloat = 46
    static let mediaSpacing: CGFloat = 4
    /// Por debajo no se encoge más (1 y 2 quedan en 31 pt): con menos alto
    /// (iPad a media pantalla) el cuerpo se recorta por abajo y la B sigue entera.
    static let floor: CGFloat = 0.6

    let grow: CGFloat
    let bodyH: CGFloat
    let mediaOpen: Bool
    let flexible: Bool
    let s: CGFloat
    let cross: CGFloat
    let small: CGFloat
    let recenter: CGFloat
    let big: CGFloat
    let one: CGFloat
    let media: CGFloat
    let trigger: CGFloat
    let spacing: CGFloat

    init(grow: CGFloat, bodyH: CGFloat, mediaOpen: Bool = false) {
        self.grow = grow
        self.bodyH = bodyH
        self.mediaOpen = mediaOpen
        flexible = grow > 1
        let fixed: CGFloat = Self.mediaButton + Self.gapMin + Self.bottom + (mediaOpen ? Self.mediaSpacing : 0)
        let scalable: CGFloat = Self.scalable + (mediaOpen ? Self.mediaRow : 0)
        let fit: CGFloat = (bodyH - fixed) / scalable
        // iPad: crece y nunca encoge; iPhone: 1 si cabe y, si no, lo justo para que quepa
        s = flexible ? grow : Swift.min(1, Swift.max(Self.floor, fit))
        cross = 168 * s
        small = 54 * s
        recenter = 64 * s
        big = 148 * s
        one = 52 * s
        media = 46 * s
        trigger = 88 * s
        spacing = 20 * s
    }

    /// Medidas para una pantalla entera con una cabecera de `headerH` pt.
    static func forScreen(_ size: CGSize, headerH: CGFloat, mediaOpen: Bool = false) -> RemoteMetrics {
        RemoteMetrics(
            grow: UiScale.factor(size, base: UiScale.remoteBase, fixed: UiScale.remoteFixed),
            bodyH: size.height - top - headerH,
            mediaOpen: mediaOpen
        )
    }

    /// Texto: crece con el iPad; en un iPhone bajo NO se encoge.
    func text(_ base: CGFloat) -> CGFloat { base * grow }
    /// Hueco entre grupos, escalado como los botones.
    func gap(_ base: CGFloat) -> CGFloat { base * s }

    /// Alto mínimo del cuerpo con estas medidas: para las pruebas.
    var bodyMin: CGFloat {
        let gaps: CGFloat = gap(18) + gap(16) + gap(16) + gap(14) + gap(10)
        let buttons: CGFloat = cross + recenter + big + one + trigger
        let mediaH: CGFloat = mediaOpen ? Self.mediaSpacing + media : 0
        let fixed: CGFloat = Self.mediaButton + Self.gapMin + Self.bottom
        return gaps + buttons + mediaH + fixed
    }

    /// Lo que se sale por abajo (0 si cabe): solo por debajo de `floor`.
    var overflow: CGFloat { Swift.max(0, bodyMin - bodyH) }
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
    /// La fila multimedia, abierta o plegada: la pantalla lo sabe porque, en un
    /// iPhone bajo, abrirla encoge un poco el resto en vez de echar la B fuera.
    @State private var mediaOpen = false
    @State private var rotation = OrientationLock.frameRotation(OrientationLock.current)
    private var retro: Bool { Route.isRetroArch(link.link) }
    /// Dónde está cada botón, para «Pulsar deslizando».
    @StateObject private var press = PressRegistry()

    /// Home del Mando de Wii: conectado y fuera del modo puntero.
    private var showHome: Bool { link.link.connected.map(showHomeButton) ?? false }

    var body: some View {
        GeometryReader { geo in
            let f = RemoteFrame(size: geo.size)
            ZStack {
                // El primero: recoge los dedos que nacen fuera de los botones
                SlideCanvas()
                // La columna del mando y las tiras comparten anchura
                ZStack {
                    column(f)
                        .padding(.horizontal, 24)
                    HStack {
                        // En Dolphin la precisión no hace nada: la tira es «Acercar»
                        if link.link.connected?.mode == LinkState.modeDolphin {
                            NearStrip(width: f.precisionW, height: f.stripH, glyph: f.text(16))
                                .padding(.leading, 6)
                        } else {
                            PrecisionStrip(width: f.precisionW, height: f.stripH, glyph: f.text(16))
                                .padding(.leading, 6)
                        }
                        Spacer()
                        ScrollStrip(width: f.scrollW, height: f.stripH)
                            .padding(.trailing, 6)
                    }
                }
                .frame(width: f.colW)

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
            link.motion?.rotation = rotation
        }
        .onChange(of: rotation) { link.motion?.rotation = $0 }
        .onReceive(NotificationCenter.default.publisher(for: UIDevice.orientationDidChangeNotification)) { _ in
            rotation = OrientationLock.frameRotation(OrientationLock.current)
        }
        .onAppear {
            UIApplication.shared.isIdleTimerDisabled = true
            UIDevice.current.beginGeneratingDeviceOrientationNotifications()
            rotation = OrientationLock.frameRotation(OrientationLock.current)
            link.motion?.rotation = rotation
        }
        .onDisappear {
            UIApplication.shared.isIdleTimerDisabled = false
            press.releaseAll() // nada queda pulsado al salir
            LinkState.shared.motion?.pointerHold = false
        }
    }

    private func column(_ f: RemoteFrame) -> some View {
        VStack(spacing: 0) {
            Spacer().frame(height: RemoteMetrics.top)
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
                    let dense = f.colW < 420
                    VStack(spacing: 5) {
                        HStack(spacing: dense ? 8 : 10) {
                            if modeChips { ModeChips(current: c.mode, supportsCemu: c.supportsCemu, supportsSwitch: c.supportsSwitch, supportsRetroArch: c.supportsRetroArch, supportsGamepad: c.supportsGamepad, supportsPointer: c.receiver.supportsPointer, dense: dense) }
                            if nunchuk && !(dense && c.supportsSwitch) { NunchukChip(link: c, dense: dense).padding(.leading, 4) }
                        }
                        if nunchuk && dense && c.supportsSwitch { NunchukChip(link: c, dense: dense) }
                    }
                }
                if isWiiUAsWiimote(c) {
                    Spacer().frame(height: 8)
                    PadSelector(link: c, width: f.colW - 48, help: tr("wii_pad_help"))
                } else if Route.isRetroArch(link.link) {
                    // RetroArch: qué mando soy (RetroPad / NES / pistola) y las teclas rápidas
                    Spacer().frame(height: 8)
                    PadSelector(link: c, width: f.colW - 48, help: tr("retro_pad_help"))
                    Spacer().frame(height: 6)
                    RetroArchHotkeys()
                }
            }
            // Cuerpo bajo la cabecera. El VStack mide antes lo que tiene tamaño
            // propio (cabecera, chips, selector) y este GeometryReader se queda
            // con lo que sobra, así que las medidas saben cuánto alto hay de
            // verdad: en un iPhone bajo todo encoge a la vez para que quepa y,
            // si ni así cabe (iPad a media pantalla), se recorta el cuerpo por
            // abajo; la B queda fuera del recorte y siempre entera
            GeometryReader { avail in
                let m = RemoteMetrics(grow: f.grow, bodyH: avail.size.height, mediaOpen: mediaOpen)
                VStack(spacing: 0) {
                    VStack(spacing: 0) {
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
                            label: Route.retroNes(link.link) ? "X" : "A", size: m.big, bit: Btn.a,
                            background: Pepo.blue, pressedColor: Pepo.blueHover, textColor: Pepo.onAccent,
                            textSize: m.text(44), pop: true
                        )
                        Gap(m.gap(14), flexible: m.flexible)
                        // 1 · Home · 2, como en el mando + Nunchuk. Home solo fuera del modo
                        // puntero (el PC no le da uso): en Dolphin es el menú HOME de la Wii
                        // y en Cemu, además, Mario Party 10 lo pide para dar por emparejado
                        // cada Mando de Wii emulado
                        HStack(spacing: m.spacing) {
                            RoundButton(label: retro ? "B" : "1", size: m.one, bit: Btn.one, textSize: m.text(18))
                            if showHome {
                                RoundButton(label: tr(retro ? "retro_menu" : "home_btn"), size: m.one, bit: Btn.home, textSize: m.text(12))
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
                            RoundButton(label: retro ? "A" : "2", size: m.one, bit: Btn.two, textSize: m.text(18))
                        }
                        Gap(m.gap(10), flexible: m.flexible)
                        MediaRow(expanded: $mediaOpen, buttonSize: m.media, textSize: m.text(16))
                        Spacer(minLength: RemoteMetrics.gapMin)
                    }
                    // Con mínimo y máximo, el marco mide justo lo que le dan:
                    // lo que no quepa asoma por abajo y se recorta
                    .frame(maxWidth: .infinity, minHeight: 0, maxHeight: .infinity, alignment: .top)
                    .clipped()
                    TriggerZone(label: Route.retroNes(link.link) ? "Y" : "B", height: m.trigger)
                    Spacer().frame(height: RemoteMetrics.bottom)
                }
                .frame(width: avail.size.width, height: avail.size.height, alignment: .top)
            }
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
