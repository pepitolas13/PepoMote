import XCTest
@testable import PepoMote

/// Los mismos vectores que Android (FullScreenMetricsTest).
final class FullScreenMetricsTests: XCTestCase {
    private func sz(_ w: CGFloat, _ h: CGFloat) -> CGSize { CGSize(width: w, height: h) }

    private func assertRect(_ r: FitRect, _ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat, line: UInt = #line) {
        XCTAssertEqual(r.x, x, accuracy: 0.5, "x", line: line)
        XCTAssertEqual(r.y, y, accuracy: 0.5, "y", line: line)
        XCTAssertEqual(r.w, w, accuracy: 0.5, "w", line: line)
        XCTAssertEqual(r.h, h, accuracy: 0.5, "h", line: line)
    }

    func testLaImagenSeAjustaYCentra() {
        // iPhone 16 apaisado con un fotograma 854×480: bandas a los lados
        assertRect(FullScreenMetrics.fitRect(container: sz(852, 393), image: sz(854, 480)), 76.4, 0, 699.2, 393)
        // vertical (no debería pasar, pero no rompe): bandas arriba y abajo
        assertRect(FullScreenMetrics.fitRect(container: sz(393, 852), image: sz(854, 480)), 0, 315.5, 393, 220.9)
        // sin fotograma: 16:9
        assertRect(FullScreenMetrics.fitRect(container: sz(2000, 1000), image: nil), 111.1, 0, 1777.8, 1000)
        // contenedor sin tamaño: vacío
        XCTAssertEqual(FullScreenMetrics.fitRect(container: sz(0, 393), image: sz(854, 480)), FitRect(x: 0, y: 0, w: 0, h: 0))
        // proporción exacta: llena todo
        XCTAssertEqual(FullScreenMetrics.fitRect(container: sz(1280, 720), image: sz(1280, 720)), FitRect(x: 0, y: 0, w: 1280, h: 720))
    }

    func testElTactilSoloSobreLaImagen() {
        let r1 = FullScreenMetrics.fitRect(container: sz(852, 393), image: sz(854, 480))
        XCTAssertTrue(FullScreenMetrics.contains(r1, CGPoint(x: 76.4, y: 0)))
        XCTAssertTrue(FullScreenMetrics.contains(r1, CGPoint(x: 426, y: 196)))
        XCTAssertFalse(FullScreenMetrics.contains(r1, CGPoint(x: 50, y: 10)))
        XCTAssertFalse(FullScreenMetrics.contains(r1, CGPoint(x: 800, y: 100)))
        XCTAssertFalse(FullScreenMetrics.contains(FitRect(x: 0, y: 0, w: 0, h: 0), CGPoint(x: 0, y: 0)))
    }

    func testLaFraccionVaDe0A65535() {
        let r2 = FullScreenMetrics.fitRect(container: sz(1708, 960), image: sz(854, 480))
        XCTAssertEqual(r2, FitRect(x: 0, y: 0, w: 1708, h: 960))
        XCTAssertTrue(FullScreenMetrics.fraction(r2, CGPoint(x: 0, y: 0)) == (0, 0))
        XCTAssertTrue(FullScreenMetrics.fraction(r2, CGPoint(x: 1708, y: 960)) == (65535, 65535))
        XCTAssertTrue(FullScreenMetrics.fraction(r2, CGPoint(x: 854, y: 480)) == (32768, 32768))
        // el vector dorado del táctil (0x8000, 0x4000)
        XCTAssertTrue(FullScreenMetrics.fraction(r2, CGPoint(x: 854, y: 240)) == (32768, 16384))
        // recortada a los bordes
        XCTAssertTrue(FullScreenMetrics.fraction(r2, CGPoint(x: -5, y: 2000)) == (0, 65535))
        XCTAssertTrue(FullScreenMetrics.fraction(FitRect(x: 0, y: 0, w: 0, h: 0), CGPoint(x: 10, y: 10)) == (0, 0))
    }

    func testElTamanoPedidoAlPc() {
        XCTAssertTrue(FullScreenMetrics.streamRequest(containerPxW: 2340, containerPxH: 1080) == (854, 480))
        XCTAssertTrue(FullScreenMetrics.streamRequest(containerPxW: 720, containerPxH: 405) == (720, 405))
        XCTAssertTrue(FullScreenMetrics.streamRequest(containerPxW: 0, containerPxH: 0) == (854, 480))
    }
}
