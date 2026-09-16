import SwiftUI
import UIKit

/// Cuándo se pliega y cuándo se abre sola la cabecera de los mandos apaisados,
/// y cuánto mide su pastilla. Lógica pura (misma que Android
/// `HeaderCollapse.kt`), para poder probarla sin pintar nada.
enum HeaderCollapse {
    /// Lo que tarda en plegarse sola: 4 s, los mismos 4000 ms de Android.
    /// `TimeInterval` son SEGUNDOS (por eso no se llama `autoMs`: pasárselo a
    /// una API de milisegundos plegaría la cabecera a los 4000 s). Cualquier
    /// toque dentro de la tarjeta vuelve a empezar la cuenta.
    static let autoSeconds: TimeInterval = 4.0

    /// Ancho máximo de la tarjeta desplegada.
    static let cardWidth: CGFloat = 420

    /// Caja táctil de la pastilla (44 de alto, como cualquier botón).
    static let handleHeight: CGFloat = 44

    /// Píldora que se ve dentro de esa caja.
    static let pillHeight: CGFloat = 26

    /// ¿Se pliega sola? Solo con el mando en juego (hay PC al otro lado) y sin
    /// lector de pantalla: con VoiceOver la tarjeta se queda hasta que la
    /// cierren, que plegarse sola se lleva el foco.
    static func autoCollapses(connected: Bool, screenReader: Bool) -> Bool {
        connected && !screenReader
    }

    /// ¿Se abre sola al cambiar el enlace? Solo al quedarse sin PC, que es
    /// cuando hacen falta «Salir» y los chips; reconectando no, que el mando
    /// sigue en la mano y la tarjeta taparía los botones.
    static func expandsOn(_ link: UiLink) -> Bool {
        if case .disconnected = link { return true }
        return false
    }

    /// Ancho de la caja táctil de la pastilla: el texto con su aire (22 a cada
    /// lado), nunca menos de 96 ni más de 240.
    static func handleWidth(textWidth: CGFloat) -> CGFloat {
        Swift.min(Swift.max(96, textWidth + 44), 240)
    }
}

/// Los dos textos de la cabecera plegable del GamePad (Wii U, Pro y Switch),
/// ya traducidos por quien llama. Sin SwiftUI: se prueban solos, con los
/// mismos vectores que Android (`GamePadHeaderText` / `GamePadHeaderTextTest`).
enum GamePadHeaderText {
    /// Lo que dice la pastilla: con el enlace vivo, el modo que se quiere
    /// («Wii U», «Switch»), que es lo que se está jugando aunque el eco del
    /// receptor aún no haya llegado; si no, el estado del enlace.
    static func handle(connected: Bool, connecting: Bool, mode: String, connectingText: String, disconnectedText: String) -> String {
        if connected { return mode }
        return connecting ? connectingText : disconnectedText
    }

    /// La línea de estado de la tarjeta, la de siempre: «J1 · GamePad» con el
    /// ida y vuelta cuando se sabe («J1 · GamePad · 23 ms»). Sin el modo
    /// confirmado todavía, el aviso de que se está activando.
    ///
    /// En la tarjeta hay sitio de sobra, así que aquí no se recorta nada por
    /// ancho (la cabecera de una línea sí lo hacía).
    static func status(operative: Bool, player: Int, padName: String, rttMs: Float?, activating: String) -> String {
        if !operative { return activating }
        var s = "J\(player) · \(padName)"
        if let rtt = rttMs { s += " · \(String(format: "%.0f", rtt)) ms" }
        return s
    }
}

/// Plegar la tarjeta desde dentro: abrir el teclado la pliega (tapa la
/// pantalla, y al volver el mando queda despejado).
struct HeaderCollapseAction {
    let collapse: () -> Void
}

private struct HeaderCollapseKey: EnvironmentKey {
    static let defaultValue = HeaderCollapseAction(collapse: {})
}

extension EnvironmentValues {
    /// Solo vale dentro de la tarjeta de `CollapsibleHeader`; fuera no hace nada.
    var headerCollapse: HeaderCollapseAction {
        get { self[HeaderCollapseKey.self] }
        set { self[HeaderCollapseKey.self] = newValue }
    }
}

/// Cabecera plegable de los mandos apaisados: una pastilla centrada arriba con
/// el modo y, desplegada bajo ella, la tarjeta con el nombre del PC, los chips
/// y «Salir». Entra abierta, se pliega sola a los 4 s y se vuelve a abrir
/// tocando la pastilla, para que nada quede encima de B/Z/C mientras se juega.
///
/// Al lado de la pastilla puede ir algo que NUNCA se pliega (`beside`): el
/// GamePad pone ahí «Teclado», que jugando tiene que estar a un toque.
///
/// Va la última de la pantalla (`.overlay(alignment: .top)`): la tarjeta gana
/// a los botones que tape.
struct CollapsibleHeader<Panel: View, Beside: View>: View {
    /// Texto de la pastilla (el modo, el mismo de la cabecera de siempre).
    private let handleLabel: String
    /// Hay PC al otro lado: sin él la tarjeta no se pliega sola.
    private let connected: Bool
    /// Aire sobre la pastilla. El GamePad lo pone a 0: así la caja táctil (44)
    /// ocupa justo la banda de la cabecera y su hueco (38 + 6) y nada de lo de
    /// abajo se mueve.
    private let topPadding: CGFloat
    /// Para que la pantalla sepa si la tarjeta tapa algo (la pregunta del lado).
    private let onExpandedChange: ((Bool) -> Void)?
    /// Lo que va junto a la pastilla, siempre a la vista.
    private let beside: () -> Beside
    private let panel: () -> Panel

    @ObservedObject private var link = LinkState.shared
    /// Se entra con la tarjeta abierta (ahí están los chips y «Salir»).
    @State private var expanded = true
    /// Cuenta atrás del pliegue automático.
    @State private var collapse: DispatchWorkItem?

    init(
        handleLabel: String,
        connected: Bool,
        topPadding: CGFloat = 6,
        onExpandedChange: ((Bool) -> Void)? = nil,
        @ViewBuilder beside: @escaping () -> Beside,
        @ViewBuilder panel: @escaping () -> Panel
    ) {
        self.handleLabel = handleLabel
        self.connected = connected
        self.topPadding = topPadding
        self.onExpandedChange = onExpandedChange
        self.beside = beside
        self.panel = panel
    }

    var body: some View {
        VStack(spacing: 4) {
            // Pastilla y lo de al lado, centrados como un bloque
            HStack(spacing: 8) {
                handle
                beside()
            }
            // Sin aire arriba, la fila entera (también lo de al lado, que
            // crece con la letra grande) se queda en la banda de 44
            .frame(height: topPadding == 0 ? HeaderCollapse.handleHeight : nil)
            if expanded { card }
        }
        .padding(.top, topPadding)
        .frame(maxWidth: .infinity, alignment: .top)
        .onAppear {
            // La pantalla se entera del estado inicial: si la cabecera se
            // quita y vuelve (pantalla completa del GamePad), nace abierta
            onExpandedChange?(expanded)
            poke()
        }
        // Al salir de la pantalla no se queda ninguna cuenta atrás viva
        .onDisappear { cancelCollapse() }
        .onChange(of: connected) { _ in poke() }
        .onChange(of: link.link) { l in
            if HeaderCollapse.expandsOn(l) { setExpanded(true) }
        }
    }

    // MARK: pastilla

    private var handle: some View {
        let text = handleLabel + (expanded ? " ▴" : " ▾")
        let pill = Self.pillHeight()
        return Button(action: { setExpanded(!expanded) }) {
            Text(text)
                .pepoBody()
                .lineLimit(1)
                .truncationMode(.tail)
                .padding(.horizontal, 10)
                .frame(width: HeaderCollapse.handleWidth(textWidth: Self.textWidth(text)), height: pill)
                .background(Pepo.card.opacity(0.85))
                .clipShape(RoundedRectangle(cornerRadius: 13, style: .continuous))
                // Sin aire arriba (el GamePad) la caja táctil no puede crecer
                // con la letra grande: 44 es justo la banda más su hueco, y de
                // ahí para abajo empieza el selector de mando
                .frame(height: topPadding == 0 ? HeaderCollapse.handleHeight : Swift.max(HeaderCollapse.handleHeight, pill))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        // Plegada, la pastilla es lo único que se ve del enlace: el lector tiene
        // que decir también el modo («Controles · Dolphin · Nunchuk»)
        .accessibilityLabel(tr("header_controls") + " · " + handleLabel)
        .accessibilityValue(expanded ? tr("header_hide") : tr("header_show"))
        .accessibilityAddTraits(.isButton)
    }

    /// La tipografía de la pastilla (`pepoBody`, Nunito de 14) ya escalada por
    /// Dynamic Type: `Font.custom` crece con la letra del sistema, así que
    /// medir con los 14 pt pelados truncaba el modo con letra grande.
    private static func pillFont() -> UIFont {
        let base = UIFont(name: "Nunito-Regular", size: 14) ?? UIFont.systemFont(ofSize: 14)
        return UIFontMetrics.default.scaledFont(for: base)
    }

    /// Lo que ocupa el texto con la tipografía de la pastilla (bodyMedium).
    private static func textWidth(_ text: String) -> CGFloat {
        (text as NSString).size(withAttributes: [.font: pillFont()]).width.rounded(.up)
    }

    /// Alto de la píldora: los 26 de siempre, o lo que pida la letra grande.
    private static func pillHeight() -> CGFloat {
        Swift.max(HeaderCollapse.pillHeight, (pillFont().lineHeight + 6).rounded(.up))
    }

    // MARK: tarjeta

    private var card: some View {
        panel()
            .environment(\.headerCollapse, HeaderCollapseAction(collapse: { setExpanded(false) }))
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            // Como el `widthIn(max = 420)` de Compose: la tarjeta abraza su
            // contenido y solo topa en 420. Sin el `fixedSize`, un marco
            // flexible de SwiftUI se come todo lo que le ofrecen y la tarjeta
            // medía SIEMPRE 420, tapando mucho más de B/Z/C y de 1/2
            .frame(maxWidth: HeaderCollapse.cardWidth)
            .fixedSize(horizontal: true, vertical: false)
            .pepoCard(radius: 14)
            // Cualquier toque dentro reinicia la cuenta atrás, sin robarles el
            // toque a los chips ni a «Salir»
            .simultaneousGesture(DragGesture(minimumDistance: 0).onChanged { _ in poke() })
            .transition(.opacity.combined(with: .move(edge: .top)))
    }

    // MARK: abrir, plegar y la cuenta atrás

    private func setExpanded(_ on: Bool) {
        if on != expanded {
            withAnimation(.easeOut(duration: 0.16)) { expanded = on }
            onExpandedChange?(on)
        }
        if on { scheduleCollapse() } else { cancelCollapse() }
    }

    private func scheduleCollapse() {
        cancelCollapse()
        guard HeaderCollapse.autoCollapses(connected: connected, screenReader: UIAccessibility.isVoiceOverRunning) else { return }
        let work = DispatchWorkItem { setExpanded(false) }
        collapse = work
        DispatchQueue.main.asyncAfter(deadline: .now() + HeaderCollapse.autoSeconds, execute: work)
    }

    private func cancelCollapse() {
        collapse?.cancel()
        collapse = nil
    }

    /// Un toque dentro de la tarjeta (o entrar en la pantalla): la cuenta atrás
    /// vuelve a empezar.
    private func poke() {
        guard expanded else { return }
        scheduleCollapse()
    }
}

/// Cabecera sin nada al lado de la pastilla: los dos mandos de Wii apaisados,
/// que la llaman igual que siempre.
extension CollapsibleHeader where Beside == EmptyView {
    init(
        handleLabel: String,
        connected: Bool,
        topPadding: CGFloat = 6,
        onExpandedChange: ((Bool) -> Void)? = nil,
        @ViewBuilder panel: @escaping () -> Panel
    ) {
        self.init(
            handleLabel: handleLabel,
            connected: connected,
            topPadding: topPadding,
            onExpandedChange: onExpandedChange,
            beside: { EmptyView() },
            panel: panel
        )
    }
}
