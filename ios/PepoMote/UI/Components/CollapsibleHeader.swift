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
/// Va la última de la pantalla (`.overlay(alignment: .top)`): la tarjeta gana
/// a los botones que tape.
struct CollapsibleHeader<Panel: View>: View {
    /// Texto de la pastilla (el modo, el mismo de la cabecera de siempre).
    private let handleLabel: String
    /// Hay PC al otro lado: sin él la tarjeta no se pliega sola.
    private let connected: Bool
    /// Para que la pantalla sepa si la tarjeta tapa algo (la pregunta del lado).
    private let onExpandedChange: ((Bool) -> Void)?
    private let panel: () -> Panel

    @ObservedObject private var link = LinkState.shared
    /// Se entra con la tarjeta abierta (ahí están los chips y «Salir»).
    @State private var expanded = true
    /// Cuenta atrás del pliegue automático.
    @State private var collapse: DispatchWorkItem?

    init(
        handleLabel: String,
        connected: Bool,
        onExpandedChange: ((Bool) -> Void)? = nil,
        @ViewBuilder panel: @escaping () -> Panel
    ) {
        self.handleLabel = handleLabel
        self.connected = connected
        self.onExpandedChange = onExpandedChange
        self.panel = panel
    }

    var body: some View {
        VStack(spacing: 4) {
            handle
            if expanded { card }
        }
        .padding(.top, 6)
        .frame(maxWidth: .infinity, alignment: .top)
        .onAppear { poke() }
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
                .frame(height: Swift.max(HeaderCollapse.handleHeight, pill))
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
