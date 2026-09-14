import XCTest
@testable import PepoMote

/// Cruceta de una pieza: dónde está el dedo → direcciones; y su mapeo a botones (mismos casos que Android).
final class DpadModelTests: XCTestCase {
    private let half: Float = 90 // brazos de media anchura 30, centro muerto de radio 18

    private func d(_ x: Float, _ y: Float) -> Int { DpadModel.dirs(x, y, half: half) }

    func testCentroMuertoPequeno() {
        XCTAssertEqual(d(0, 0), 0)
        XCTAssertEqual(d(10, 10), 0) // a 14 del centro
        XCTAssertEqual(d(-17, 0), 0)
        // Justo fuera del círculo: ya es una dirección (antes, todo el cuadrado central de 60 era muerto)
        XCTAssertEqual(d(19, 0), DpadModel.right)
        XCTAssertEqual(d(12, 14), DpadModel.down) // a 18,4: en el cuadrado central manda el eje dominante
        XCTAssertEqual(d(15, 12), DpadModel.right)
        XCTAssertEqual(d(-25, 0), DpadModel.left)
        XCTAssertEqual(d(0, -25), DpadModel.up)
    }

    func testSobreUnBrazoSoloSuDireccion() {
        XCTAssertEqual(d(0, -60), DpadModel.up)
        XCTAssertEqual(d(60, 0), DpadModel.right)
        XCTAssertEqual(d(60, 20), DpadModel.right, "el dedo descentrado en el brazo no añade la diagonal")
        XCTAssertEqual(d(60, -30), DpadModel.right, "hasta el borde del brazo")
        XCTAssertEqual(d(20, 60), DpadModel.down)
        XCTAssertEqual(d(-60, -29), DpadModel.left)
        // Fuera de la cruceta por la punta: sigue siendo esa dirección (el pulgar resbala)
        XCTAssertEqual(d(200, 5), DpadModel.right)
        XCTAssertEqual(d(-10, -400), DpadModel.up)
    }

    func testLasEsquinasSonDiagonales() {
        XCTAssertEqual(d(50, 50), DpadModel.down | DpadModel.right)
        XCTAssertEqual(d(-40, 35), DpadModel.down | DpadModel.left)
        XCTAssertEqual(d(-35, -40), DpadModel.up | DpadModel.left)
        XCTAssertEqual(d(31, -31), DpadModel.up | DpadModel.right, "justo pasada la esquina del cuadrado central")
        XCTAssertEqual(d(200, 200), DpadModel.down | DpadModel.right, "también fuera de la cruceta")
    }

    func testBotonesDelMandoYDeLado() {
        XCTAssertEqual(DpadModel.buttons(DpadModel.up, sideways: false), Btn.dpadUp)
        XCTAssertEqual(DpadModel.buttons(DpadModel.down | DpadModel.left, sideways: false), Btn.dpadDown | Btn.dpadLeft)
        XCTAssertEqual(DpadModel.buttons(0, sideways: true), 0)
        // De lado (IR a la izquierda): arriba en pantalla es el RIGHT del mando…
        XCTAssertEqual(DpadModel.buttons(DpadModel.up, sideways: true), Btn.dpadRight)
        XCTAssertEqual(DpadModel.buttons(DpadModel.up | DpadModel.right, sideways: true), Btn.dpadRight | Btn.dpadDown)
        let all = DpadModel.up | DpadModel.down | DpadModel.left | DpadModel.right
        XCTAssertEqual(DpadModel.buttons(all, sideways: true), Btn.dpadUp | Btn.dpadDown | Btn.dpadLeft | Btn.dpadRight)
    }

    func testContornoDeLaCruz() {
        // 12 esquinas → 12 curvas; el brazo ↑ acaba en la punta y el → es el ↑ girado
        let g = DpadGeometry(half: 90)
        XCTAssertEqual(g.a, 30)
        let outline = g.outline(center: CGPoint(x: 100, y: 100)).boundingRect
        XCTAssertEqual(outline.minX, 10, accuracy: 0.01)
        XCTAssertEqual(outline.maxY, 190, accuracy: 0.01)
        let up = g.arm(center: CGPoint(x: 100, y: 100), dir: 0).boundingRect
        XCTAssertEqual(up.minY, 10, accuracy: 0.01)
        XCTAssertEqual(up.maxY, 70, accuracy: 0.01)
        XCTAssertEqual(up.width, 60, accuracy: 0.01)
        let right = g.arm(center: CGPoint(x: 100, y: 100), dir: 1).boundingRect
        XCTAssertEqual(right.maxX, 190, accuracy: 0.01)
        XCTAssertEqual(right.minX, 130, accuracy: 0.01)
        XCTAssertEqual(right.height, 60, accuracy: 0.01)
        let m = g.mark(dir: 3, thickness: 4)
        XCTAssertEqual(m.dx, -64.8, accuracy: 0.01)
        XCTAssertEqual(m.w, 21, accuracy: 0.01)
        XCTAssertEqual(m.h, 4)
    }
}
