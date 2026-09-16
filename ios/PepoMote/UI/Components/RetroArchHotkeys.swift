import SwiftUI

/// Una tecla rápida de RetroArch tal como la pide el móvil (mensaje `hotkey`,
/// PROTOCOL.md §3): nombre del protocolo, clave de su etiqueta y si es de
/// mantener (rebobinar: pulsada mientras el dedo esté encima) o de un toque.
struct RetroHotkey: Equatable {
    let name: String
    let label: String
    var hold = false
}

/// Las que caben en una tarjeta; el resto de la tabla del receptor no hace falta a un toque.
let retroHotkeys: [RetroHotkey] = [
    RetroHotkey(name: "save_state", label: "hotkey_save"),
    RetroHotkey(name: "load_state", label: "hotkey_load"),
    RetroHotkey(name: "slot_minus", label: "hotkey_slot_minus"),
    RetroHotkey(name: "slot_plus", label: "hotkey_slot_plus"),
    RetroHotkey(name: "rewind", label: "hotkey_rewind", hold: true),
    RetroHotkey(name: "pause", label: "hotkey_pause"),
    RetroHotkey(name: "screenshot", label: "hotkey_screenshot"),
    RetroHotkey(name: "reset", label: "hotkey_reset")
]

/// Fila de teclas rápidas de RetroArch para las tarjetas de cabecera: guardar
/// y cargar estado, ranura − / +, rebobinar (mantener), pausa, captura y
/// reiniciar. Cada una va por el canal de control al receptor, que la
/// traduce al comando de red de RetroArch; el avance rápido y el menú están
/// en el propio mando (Rápido y Menú).
struct RetroArchHotkeys: View {
    var compact = false

    var body: some View {
        VStack(spacing: 4) {
            FlowRowCompat(spacing: 6) {
                ForEach(retroHotkeys, id: \.name) { h in
                    HotkeyChip(label: tr(h.label), hold: h.hold, compact: compact) { down in
                        LinkState.shared.sendHotkey?(h.name, down)
                    }
                }
            }
            if !compact {
                Text(tr("hotkeys_hint"))
                    .font(PepoFont.nunito(11, .regular))
                    .foregroundColor(Pepo.textDim)
                    .lineLimit(1)
            }
        }
        // Si la tarjeta se pliega con Rebobinar bajo el dedo, el receptor no se
        // queda rebobinando: suelta al desaparecer
        .onDisappear { LinkState.shared.sendHotkey?("rewind", false) }
    }
}

/// Dos filas de cuatro (iOS 15 no tiene un FlowRow de serie): las ocho
/// teclas caben en la tarjeta de 420 pt en cualquier iPhone.
private struct FlowRowCompat<Content: View>: View {
    let spacing: CGFloat
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(spacing: spacing) {
            HStack(spacing: spacing) { content() }
        }
    }
}

/// Chip de tecla rápida. Las de un toque disparan al pulsar (`down` true una
/// vez); las de mantener mandan true al bajar el dedo y false al levantarlo, y
/// se pintan pulsadas mientras tanto.
private struct HotkeyChip: View {
    let label: String
    let hold: Bool
    let compact: Bool
    let onPress: (Bool) -> Void
    @State private var pressed = false

    var body: some View {
        Text(label)
            .font(PepoFont.nunito(compact ? 12 : 13, .regular))
            .foregroundColor(pressed ? Pepo.onAccent : Pepo.text)
            .lineLimit(1)
            .minimumScaleFactor(0.7)
            .padding(.horizontal, compact ? 8 : 10)
            .padding(.vertical, compact ? 5 : 7)
            .background(pressed ? Pepo.blue : Pepo.card)
            .clipShape(Capsule())
            .overlay(Capsule().stroke(pressed ? Pepo.blue : Pepo.cardBorder, lineWidth: 1.5))
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        if !pressed {
                            pressed = true
                            onPress(true)
                        }
                    }
                    .onEnded { _ in
                        pressed = false
                        if hold { onPress(false) }
                    }
            )
            .accessibilityAddTraits(.isButton)
    }
}
