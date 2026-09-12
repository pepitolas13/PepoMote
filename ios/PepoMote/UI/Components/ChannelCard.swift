import SwiftUI

// Glifos propios, dibujados a mano — nada de iconografía ajena.
enum ChannelGlyph {
    case qr, pad, pointer, stick, gamePad, gear
}

/// Tarjeta del inicio: glifo, título y subtítulo.
struct ChannelCard: View {
    let title: String
    let subtitle: String
    let glyph: ChannelGlyph
    var accent: Color = Pepo.blue
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 2) {
                GlyphView(glyph: glyph, accent: accent)
                    .frame(width: 52, height: 52)
                    .padding(.bottom, 4)
                Text(title)
                    .font(PepoFont.titleMedium())
                    .foregroundColor(Pepo.text)
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                Text(subtitle)
                    .pepoBody()
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                    .minimumScaleFactor(0.8)
            }
            .padding(14)
            .frame(maxWidth: .infinity)
            .aspectRatio(1, contentMode: .fit)
            .pepoCard()
        }
        .buttonStyle(.plain)
    }
}

struct GlyphView: View {
    let glyph: ChannelGlyph
    let accent: Color

    var body: some View {
        Canvas { ctx, size in
            let w = min(size.width, size.height)
            let ox = (size.width - w) / 2
            let oy = (size.height - w) / 2
            ctx.translateBy(x: ox, y: oy)
            switch glyph {
            case .qr: GlyphView.qr(&ctx, w, accent)
            case .pad: GlyphView.pad(&ctx, w, accent)
            case .pointer: GlyphView.pointer(&ctx, w, accent)
            case .stick: GlyphView.stick(&ctx, w, accent)
            case .gamePad: GlyphView.gamePad(&ctx, w, accent)
            case .gear: GlyphView.gear(&ctx, w, accent)
            }
        }
    }

    private static func roundRect(_ ctx: inout GraphicsContext, _ r: CGRect, _ radius: CGFloat, _ color: Color) {
        ctx.fill(Path(roundedRect: r, cornerRadius: radius), with: .color(color))
    }

    private static func circle(_ ctx: inout GraphicsContext, _ c: CGPoint, _ r: CGFloat, _ color: Color) {
        ctx.fill(Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: 2 * r, height: 2 * r)), with: .color(color))
    }

    private static func ring(_ ctx: inout GraphicsContext, _ c: CGPoint, _ r: CGFloat, _ width: CGFloat, _ color: Color) {
        ctx.stroke(Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: 2 * r, height: 2 * r)), with: .color(color), lineWidth: width)
    }

    private static func qr(_ ctx: inout GraphicsContext, _ w: CGFloat, _ accent: Color) {
        let cell = w / 5
        let positions: [(Int, Int)] = [
            (0, 0), (1, 0), (3, 0), (4, 0),
            (0, 1), (4, 1),
            (2, 2),
            (0, 3), (3, 3),
            (0, 4), (1, 4), (4, 4),
        ]
        for (x, y) in positions {
            let r = CGRect(x: CGFloat(x) * cell + cell * 0.1, y: CGFloat(y) * cell + cell * 0.1, width: cell * 0.8, height: cell * 0.8)
            roundRect(&ctx, r, cell * 0.2, accent)
        }
    }

    private static func pad(_ ctx: inout GraphicsContext, _ w: CGFloat, _ accent: Color) {
        let arm = w * 0.3
        let thick = w * 0.28
        let r = thick * 0.35
        roundRect(&ctx, CGRect(x: (w - thick) / 2, y: (w - (arm * 2 + thick)) / 2, width: thick, height: arm * 2 + thick), r, accent)
        roundRect(&ctx, CGRect(x: (w - (arm * 2 + thick)) / 2, y: (w - thick) / 2, width: arm * 2 + thick, height: thick), r, accent)
    }

    private static func pointer(_ ctx: inout GraphicsContext, _ w: CGFloat, _ accent: Color) {
        let c = CGPoint(x: w / 2, y: w / 2)
        ring(&ctx, c, w * 0.42, w * 0.10, accent)
        circle(&ctx, c, w * 0.12, accent)
    }

    /// Stick empujado arriba-derecha: aro, vástago y pomo.
    private static func stick(_ ctx: inout GraphicsContext, _ w: CGFloat, _ accent: Color) {
        let c = CGPoint(x: w / 2, y: w / 2)
        ring(&ctx, c, w * 0.42, w * 0.07, accent)
        let knob = CGPoint(x: c.x + w * 0.15, y: c.y - w * 0.15)
        var p = Path()
        p.move(to: c)
        p.addLine(to: knob)
        ctx.stroke(p, with: .color(accent), style: StrokeStyle(lineWidth: w * 0.11, lineCap: .round))
        circle(&ctx, knob, w * 0.17, accent)
    }

    /// GamePad de Wii U: cuerpo apaisado con la pantalla en medio y un stick a cada lado.
    private static func gamePad(_ ctx: inout GraphicsContext, _ w: CGFloat, _ accent: Color) {
        let stroke = w * 0.08
        let bodyH = w * 0.64
        let top = (w - bodyH) / 2
        let body = CGRect(x: stroke / 2, y: top + stroke / 2, width: w - stroke, height: bodyH - stroke)
        ctx.stroke(Path(roundedRect: body, cornerRadius: w * 0.14), with: .color(accent), lineWidth: stroke)
        roundRect(&ctx, CGRect(x: w * 0.31, y: top + bodyH * 0.24, width: w * 0.38, height: bodyH * 0.52), w * 0.04, accent)
        circle(&ctx, CGPoint(x: w * 0.17, y: top + bodyH * 0.40), w * 0.075, accent)
        circle(&ctx, CGPoint(x: w * 0.83, y: top + bodyH * 0.40), w * 0.075, accent)
        circle(&ctx, CGPoint(x: w * 0.17, y: top + bodyH * 0.72), w * 0.04, accent)
        circle(&ctx, CGPoint(x: w * 0.83, y: top + bodyH * 0.72), w * 0.04, accent)
    }

    private static func gear(_ ctx: inout GraphicsContext, _ w: CGFloat, _ accent: Color) {
        let c = CGPoint(x: w / 2, y: w / 2)
        let rOut = w * 0.42
        let rIn = w * 0.16
        // Dientes: 8 rectángulos rotados
        for i in 0..<8 {
            var tooth = ctx
            tooth.translateBy(x: c.x, y: c.y)
            tooth.rotate(by: .degrees(Double(i) * 45))
            tooth.translateBy(x: -c.x, y: -c.y)
            tooth.fill(
                Path(roundedRect: CGRect(x: c.x - w * 0.06, y: c.y - rOut, width: w * 0.12, height: w * 0.20), cornerRadius: w * 0.03),
                with: .color(accent)
            )
        }
        ring(&ctx, c, rOut * 0.72, w * 0.10, accent)
        circle(&ctx, c, rIn, accent)
    }
}
