import SwiftUI

// MARK: - Cruceta

/// Cruceta de una sola pieza, como la del Mando de Wii: una cruz con las
/// puntas redondeadas y una marca en relieve en cada brazo, sin flechas. El
/// dedo se sigue mientras está apoyado (`DpadModel`): deslizarlo cambia de
/// dirección sin levantarlo, las esquinas entre dos brazos son diagonales y
/// el centro muerto es pequeño. El brazo pulsado se ilumina; cada dirección
/// nueva hace clic (háptica y sonido), como en un mando de verdad. En modo
/// puntero el receptor hace ↑/↓ = flechas del PC y ←/→ = atrás/adelante del
/// navegador. Con `sideways` (mando de lado en un juego) manda los botones
/// del mando girado con el IR a la izquierda (`Btn.sideways`).
struct PadCross: View {
    let size: CGFloat
    var sideways = false
    /// Direcciones en pantalla iluminadas (`DpadModel`).
    @State private var dirs = 0
    /// Bits del mando que tenemos pulsados.
    @State private var held: UInt32 = 0

    var body: some View {
        let g = DpadGeometry(half: size / 2)
        let t = max(2, g.a * 0.14)
        ZStack {
            PlusShape()
                .fill(Pepo.card)
                .shadow(color: Color.black.opacity(0.16), radius: 3, x: 0, y: 1.5)
            ForEach(0..<4, id: \.self) { dir in
                if dirs & DpadGeometry.flags[dir] != 0 {
                    ArmShape(dir: dir).fill(Pepo.glow)
                }
            }
            PlusShape().stroke(Pepo.cardBorder, lineWidth: 1.5)
            ForEach(0..<4, id: \.self) { dir in
                let m = g.mark(dir: dir, thickness: t)
                RoundedRectangle(cornerRadius: t / 2, style: .continuous)
                    .fill(Pepo.cardBorder)
                    .frame(width: m.w, height: m.h)
                    .offset(x: m.dx, y: m.dy)
            }
        }
        .frame(width: size, height: size)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0, coordinateSpace: .local)
                .onChanged { v in apply(dx: v.location.x - size / 2, dy: v.location.y - size / 2, half: size / 2) }
                .onEnded { _ in release() }
        )
        .onDisappear { release() }
    }

    private func apply(dx: CGFloat, dy: CGFloat, half: CGFloat) {
        let d = DpadModel.dirs(Float(dx), Float(dy), half: Float(half))
        let bits = DpadModel.buttons(d, sideways: sideways)
        if bits == held { return }
        for bit in DpadModel.buttonBits {
            let was = held & bit != 0
            let now = bits & bit != 0
            if was != now { ButtonState.shared.set(bit, now) }
        }
        // Algo nuevo pulsado (no solo soltado): clic
        if bits & ~held != 0 {
            Haptics.tap()
            UiSounds.shared.blip()
        }
        held = bits
        dirs = d
    }

    private func release() {
        for bit in DpadModel.buttonBits where held & bit != 0 {
            ButtonState.shared.set(bit, false)
        }
        held = 0
        dirs = 0
    }
}

/// Geometría de la cruz (los mismos números que en Android y en el móvil
/// Linux): media anchura `half` → brazos de media anchura `a` (un tercio),
/// puntas redondeadas con `rOut` y esquinas interiores con `rIn`.
struct DpadGeometry {
    let half: CGFloat
    var a: CGFloat { half / 3 }
    var rOut: CGFloat { a * 0.55 }
    var rIn: CGFloat { a * 0.28 }

    /// Dirección de `DpadModel` de cada brazo (0 ↑, 1 →, 2 ↓, 3 ←).
    static let flags = [DpadModel.up, DpadModel.right, DpadModel.down, DpadModel.left]

    /// Contorno completo: 12 esquinas en sentido horario desde la interior superior izquierda.
    func outline(center c: CGPoint) -> Path {
        let h = half
        let pts: [(CGFloat, CGFloat)] = [
            (-a, -a), (-a, -h), (a, -h), (a, -a), (h, -a), (h, a),
            (a, a), (a, h), (-a, h), (-a, a), (-h, a), (-h, -a),
        ]
        let radii = (0..<12).map { $0 % 3 == 0 ? rIn : rOut }
        return roundedPolygon(pts.map { CGPoint(x: c.x + $0.0, y: c.y + $0.1) }, radii)
    }

    /// Brazo `dir` (0 ↑, 1 →, 2 ↓, 3 ←): de la arista del cuadrado central a la punta.
    func arm(center c: CGPoint, dir: Int) -> Path {
        let up: [(CGFloat, CGFloat)] = [(-a, -a), (-a, -half), (a, -half), (a, -a)]
        let pts = up.map { p -> CGPoint in
            var q = p
            for _ in 0..<dir { q = (-q.1, q.0) } // 90 grados en sentido horario
            return CGPoint(x: c.x + q.0, y: c.y + q.1)
        }
        return roundedPolygon(pts, [0, rOut, rOut, 0])
    }

    /// Marca en relieve del brazo `dir`, a lo largo del brazo y cerca de la
    /// punta: centro respecto al de la cruz y tamaño.
    func mark(dir: Int, thickness t: CGFloat) -> (dx: CGFloat, dy: CGFloat, w: CGFloat, h: CGFloat) {
        let len = a * 0.7
        let at = half * 0.72
        switch dir {
        case 0: return (0, -at, t, len)
        case 1: return (at, 0, len, t)
        case 2: return (0, at, t, len)
        default: return (-at, 0, len, t)
        }
    }
}

/// Polígono con cada esquina redondeada (curva cuadrática; radio ≤ la mitad del lado más corto).
func roundedPolygon(_ p: [CGPoint], _ r: [CGFloat]) -> Path {
    var path = Path()
    let n = p.count
    for i in 0..<n {
        let prev = p[(i + n - 1) % n]
        let cur = p[i]
        let next = p[(i + 1) % n]
        let rr = min(r[i], min(polyDist(prev, cur), polyDist(cur, next)) / 2)
        let start = polyTowards(cur, prev, rr)
        let end = polyTowards(cur, next, rr)
        if i == 0 { path.move(to: start) } else { path.addLine(to: start) }
        path.addQuadCurve(to: end, control: cur)
    }
    path.closeSubpath()
    return path
}

private func polyDist(_ a: CGPoint, _ b: CGPoint) -> CGFloat {
    ((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)).squareRoot()
}

private func polyTowards(_ from: CGPoint, _ to: CGPoint, _ d: CGFloat) -> CGPoint {
    let len = polyDist(from, to)
    if len == 0 { return from }
    return CGPoint(x: from.x + (to.x - from.x) * d / len, y: from.y + (to.y - from.y) * d / len)
}

/// La cruz entera (fondo, borde y sombra).
struct PlusShape: Shape {
    func path(in rect: CGRect) -> Path {
        DpadGeometry(half: rect.width / 2).outline(center: CGPoint(x: rect.midX, y: rect.midY))
    }
}

/// Un brazo de la cruz (el pulsado se ilumina).
struct ArmShape: Shape {
    let dir: Int

    func path(in rect: CGRect) -> Path {
        DpadGeometry(half: rect.width / 2).arm(center: CGPoint(x: rect.midX, y: rect.midY), dir: dir)
    }
}
