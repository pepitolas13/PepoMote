import SwiftUI

/// Etiqueta de un botón de plantilla: las palabras (`$coin`, `$fire`) se traducen.
func retroLabel(_ label: String) -> String {
    switch label {
    case "$coin": return tr("retro_coin")
    case "$fire": return tr("retro_fire")
    default: return label
    }
}

/// Amarillo de los botones C de N64.
private let cYellow = Color(red: 0xE0 / 255.0, green: 0xB4 / 255.0, blue: 0x2A / 255.0)

/// Relleno de un botón de consola (`card` = el de siempre).
private func tint(_ color: RetroColor) -> Color? {
    switch color {
    case .card: return nil
    case .red: return Color(red: 0xD9 / 255.0, green: 0x4A / 255.0, blue: 0x4A / 255.0)
    case .yellow: return cYellow
    case .green: return Color(red: 0x3F / 255.0, green: 0xA5 / 255.0, blue: 0x5B / 255.0)
    case .blue: return Pepo.blue
    case .pink: return Color(red: 0xD9 / 255.0, green: 0x6B / 255.0, blue: 0xB0 / 255.0)
    case .purple: return Color(red: 0x8A / 255.0, green: 0x63 / 255.0, blue: 0xC9 / 255.0)
    }
}

private func darker(_ c: Color) -> Color { c.opacity(0.75) }

/// Los botones frontales de la plantilla en una caja de `boxW`×`boxH`: el
/// rombo de siempre (X arriba, Y izquierda, A a la derecha en azul, B abajo:
/// con el RetroPad es exactamente el `FaceButtons` de antes), dos en fila (a
/// distinta altura si la consola los tenía así), uno, seis en dos filas o tres
/// en fila. El texto crece con el botón y los colores son los de cada consola.
struct FaceCluster: View {
    let layout: RetroLayout
    let boxW: CGFloat
    let boxH: CGFloat

    private var base: CGFloat {
        switch layout.shape {
        case .diamond: return min(boxW, boxH) / 2.6
        case .two: return min(boxH * 0.55, boxW * 0.42)
        case .single: return min(boxW, boxH) * 0.7
        case .grid2x3: return min(boxW / 3.3, boxH / 2.2)
        case .row3: return min(boxW / 3.3, boxH * 0.8)
        }
    }

    private func bias(_ f: RetroFace) -> (CGFloat, CGFloat) {
        let s = CGFloat(layout.stagger)
        switch (layout.shape, f.slot) {
        case (.diamond, "top"): return (0, -1)
        case (.diamond, "left"): return (-1, 0)
        case (.diamond, "right"): return (1, 0)
        case (.diamond, "bottom"): return (0, 1)
        case (.two, "left"): return (-1, 0.6 * s)
        case (.two, "right"): return (1, -0.6 * s)
        case (.grid2x3, "tl"): return (-1, -1)
        case (.grid2x3, "tm"): return (0, -1)
        case (.grid2x3, "tr"): return (1, -1)
        case (.grid2x3, "bl"): return (-1, 1)
        case (.grid2x3, "bm"): return (0, 1)
        case (.grid2x3, "br"): return (1, 1)
        case (.row3, "l"): return (-1, 0)
        case (.row3, "r"): return (1, 0)
        default: return (0, 0)
        }
    }

    var body: some View {
        ZStack {
            ForEach(layout.face, id: \.slot) { f in
                let btn = base * CGFloat(f.size)
                let b = bias(f)
                let fill = tint(f.color)
                Group {
                    if f.primary {
                        RoundButton(label: retroLabel(f.label), size: btn, bit: f.bit,
                                    background: fill ?? Pepo.blue, pressedColor: fill.map(darker) ?? Pepo.blueHover,
                                    textColor: Pepo.onAccent, textSize: btn * (20.0 / 44.0), pop: true)
                    } else if let fill {
                        RoundButton(label: retroLabel(f.label), size: btn, bit: f.bit,
                                    background: fill, pressedColor: darker(fill), textColor: Pepo.onAccent, textSize: btn * (18.0 / 44.0))
                    } else {
                        RoundButton(label: retroLabel(f.label), size: btn, bit: f.bit, textSize: btn * (18.0 / 44.0))
                    }
                }
                .offset(x: b.0 * (boxW - btn) / 2, y: b.1 * (boxH - btn) / 2)
            }
        }
        .frame(width: boxW, height: boxH)
    }
}

/// N64: los cuatro botones C en el hueco del stick derecho. Emiten los ejes
/// de ese stick a ±127 (así los lee Mupen64Plus), con un dedo por botón:
/// `onStick` recibe la suma de los pulsados (+y arriba, como el stick).
struct CButtons: View {
    let size: CGFloat
    let onStick: (Int, Int) -> Void
    @State private var held = 0

    private static let dirs: [(Int, String, CGFloat, CGFloat)] = [(0, "▲", 0, -1), (1, "▼", 0, 1), (2, "◀", -1, 0), (3, "▶", 1, 0)]

    private func emit(_ mask: Int) {
        held = mask
        let x = (mask & 8 != 0 ? 127 : 0) - (mask & 4 != 0 ? 127 : 0)
        let y = (mask & 1 != 0 ? 127 : 0) - (mask & 2 != 0 ? 127 : 0)
        onStick(x, y)
    }

    var body: some View {
        let btn = size * 0.34
        let d = size * 0.36
        return ZStack {
            ForEach(Self.dirs, id: \.0) { item in
                let (dir, label, bx, by) = item
                let bit = 1 << dir
                let down = (held & bit) != 0
                Text(label)
                    .font(PepoFont.nunito(13, .bold))
                    .foregroundColor(Pepo.onAccent)
                    .frame(width: btn, height: btn)
                    .background(down ? darker(cYellow) : cYellow)
                    .clipShape(Circle())
                    .shadow(radius: down ? 1 : 4)
                    .offset(x: bx * d, y: by * d)
                    .gesture(
                        DragGesture(minimumDistance: 0)
                            .onChanged { _ in if !down { Haptics.tick(); emit(held | bit) } }
                            .onEnded { _ in emit(held & ~bit) }
                    )
                    .accessibilityLabel("C \(label)")
            }
        }
        .frame(width: size, height: size)
    }
}

/// Selector del mando de consola en RetroArch: «Automático (Mega Drive)», las
/// consolas, «Mega Drive (3)» y «RetroPad completo». La elección se recuerda
/// para el juego cargado (o hasta que RetroArch cargue uno).
struct RetroLayoutPicker: View {
    let link: ConnectedLink
    let onClose: () -> Void
    @ObservedObject private var state = LinkState.shared

    var body: some View {
        let game = link.game
        let path = (game?.path).flatMap { $0.isEmpty ? nil : $0 }
        let chosen = state.retroLayoutChoice.choiceFor(path)
        let autoName = RetroLayouts.byId(RetroLayouts.effective(console: game?.console, chosen: nil))?.name ?? RetroLayouts.retroPad.name
        NavigationView {
            List {
                Section(footer: Text((game?.title).flatMap { $0.isEmpty ? nil : tr("retro_layout_hint_game", $0) } ?? tr("retro_layout_hint_no_game"))) {
                    row(tr("retro_layout_auto", autoName), selected: chosen == nil) {
                        state.pickRetroLayout(path: path, id: nil)
                        onClose()
                    }
                    ForEach(RetroLayouts.all, id: \.id) { l in
                        row(l.id == RetroLayouts.retroPad.id ? tr("retro_layout_full") : l.name, selected: chosen == l.id) {
                            state.pickRetroLayout(path: path, id: l.id)
                            onClose()
                        }
                    }
                }
            }
            .navigationTitle(tr("retro_layout_picker_title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(tr("kb_close"), action: onClose)
                }
            }
        }
    }

    private func row(_ label: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack {
                Text(label).foregroundColor(Pepo.text)
                Spacer()
                if selected { Image(systemName: "checkmark").foregroundColor(Pepo.blue) }
            }
        }
    }
}
