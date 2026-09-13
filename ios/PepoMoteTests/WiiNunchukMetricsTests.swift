import XCTest
@testable import PepoMote

/// Medidas del mando + Nunchuk apaisado: cabe todo en iPhone y iPad (mismos números que Android).
final class WiiNunchukMetricsTests: XCTestCase {
    private func sz(_ w: CGFloat, _ h: CGFloat) -> CGSize { CGSize(width: w, height: h) }

    private func assertFits(_ m: WiiNunchukMetrics, _ w: CGFloat, _ h: CGFloat) {
        // stick a la izquierda, columna central y cruceta + A a la derecha, sin solaparse
        XCTAssertLessThanOrEqual(m.margin + m.stick + m.centerMin, w - m.margin - m.cross - m.gap - m.a + 0.01, "ancho \(w)")
        // el stick cabe bajo las pastillas Z/C
        XCTAssertLessThanOrEqual(m.headerH + m.pillH + m.gap + m.stick + m.margin, h + 0.01, "alto \(h)")
        XCTAssertGreaterThanOrEqual(m.stick, 120)
    }

    func testIphone() {
        let m = WiiNunchukMetrics(size: sz(852, 393))
        XCTAssertEqual(m.s, 1)
        XCTAssertEqual(m.stick, 280, accuracy: 0.5)
        XCTAssertEqual(m.cross, 150, accuracy: 0.5)
        XCTAssertEqual(m.a, 148, accuracy: 0.5)
        assertFits(m, 852, 393)
        // iPhone estrecho (SE de lado): el stick cede sitio al centro
        let narrow = WiiNunchukMetrics(size: sz(667, 375))
        XCTAssertEqual(narrow.stick, 174, accuracy: 0.5)
        assertFits(narrow, 667, 375)
        // iPhone 16 con las zonas seguras
        assertFits(WiiNunchukMetrics(size: sz(734, 393)), 734, 393)
    }

    func testIpad() {
        let m = WiiNunchukMetrics(size: sz(1180, 820))
        XCTAssertGreaterThan(m.s, 1.5)
        XCTAssertGreaterThan(m.stick, 280)
        XCTAssertGreaterThan(m.cross, 200)
        XCTAssertGreaterThan(m.a, 200)
        assertFits(m, 1180, 820)
        assertFits(WiiNunchukMetrics(size: sz(1376, 1032)), 1376, 1032)
    }
}
