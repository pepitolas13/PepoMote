import XCTest
@testable import PepoMote

/// Escala de la interfaz en iPad: mismos vectores que Android (UiScaleTest).
final class UiScaleTests: XCTestCase {
    private func sz(_ w: CGFloat, _ h: CGFloat) -> CGSize { CGSize(width: w, height: h) }

    func testVerticalNoEncogeYCreceEnIpad() {
        let base = UiScale.phonePortrait
        XCTAssertEqual(UiScale.factor(sz(375, 667), base: base, max: 1.5), 1)
        XCTAssertEqual(UiScale.factor(sz(393, 852), base: base, max: 1.5), 1)
        XCTAssertEqual(UiScale.factor(sz(440, 956), base: base, max: 1.5), 1)
        XCTAssertEqual(UiScale.factor(sz(744, 1133), base: base, max: 1.5), 1.185, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(820, 1180), base: base, max: 1.5), 1.234, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(1032, 1376), base: base, max: 1.5), 1.439, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(2000, 3000), base: base, max: 1.5), 1.5)
        XCTAssertEqual(UiScale.factor(sz(0, 0), base: base, max: 1.5), 1)
    }

    func testApaisado() {
        let base = UiScale.phoneLandscape
        XCTAssertEqual(UiScale.factor(sz(852, 393), base: base, max: 1.6), 1)
        XCTAssertEqual(UiScale.factor(sz(1180, 820), base: base, max: 1.6), 1.234, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(1376, 1032), base: base, max: 1.6), 1.439, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(5000, 3000), base: base, max: 1.6), 1.6)
    }

    func testSoloAncho() {
        XCTAssertEqual(UiScale.factor(width: 844, base: 956, max: 1.6), 1)
        XCTAssertEqual(UiScale.factor(width: 956, base: 956, max: 1.6), 1)
        XCTAssertEqual(UiScale.factor(width: 1180, base: 956, max: 1.6), 1.234, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(width: 1376, base: 956, max: 1.6), 1.439, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(width: 3000, base: 956, max: 1.6), 1.6)
    }

    func testMedidasDelMandoEnIphoneSonLasDeSiempre() {
        let m = RemoteMetrics(size: sz(393, 852))
        XCTAssertEqual(m.grow, 1)
        XCTAssertEqual(m.s, 1)
        XCTAssertEqual(m.cross, 168)
        XCTAssertEqual(m.big, 148)
        XCTAssertEqual(m.colW, 393)
        XCTAssertFalse(m.flexible)
        XCTAssertEqual(m.text(44), 44)
        // iPhone SE: se encoge a la vez, como hasta ahora
        let se = RemoteMetrics(size: sz(375, 667))
        XCTAssertEqual(se.s, 667.0 / 780.0, accuracy: 0.0001)
        XCTAssertEqual(se.cross, 168 * 667.0 / 780.0, accuracy: 0.01)
        XCTAssertEqual(se.text(44), 44, "el texto no se encoge en el SE")
        // iPad 11": crece, columna de 520·grow y huecos flexibles
        let pad = RemoteMetrics(size: sz(820, 1180))
        XCTAssertEqual(pad.grow, 1.234, accuracy: 0.001)
        XCTAssertEqual(pad.s, pad.grow)
        XCTAssertEqual(pad.colW, 520 * pad.grow, accuracy: 0.01)
        XCTAssertTrue(pad.flexible)
        XCTAssertEqual(pad.text(44), 44 * pad.grow, accuracy: 0.01)
        // GamePad en iPad 13" apaisado: los topes crecen con k
        let g = PadMetrics(size: sz(1376, 1032))
        XCTAssertEqual(g.k, 1.439, accuracy: 0.001)
        XCTAssertGreaterThan(g.padSize, 200)
        XCTAssertGreaterThan(g.shoulderW, 150)
        let phone = PadMetrics(size: sz(852, 393))
        XCTAssertEqual(phone.k, 1)
        XCTAssertLessThanOrEqual(phone.padSize, 200)
    }
}
