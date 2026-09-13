import XCTest
@testable import PepoMote

/// Escala de la interfaz en iPad: mismos vectores que Android (UiScaleTest).
final class UiScaleTests: XCTestCase {
    private func sz(_ w: CGFloat, _ h: CGFloat) -> CGSize { CGSize(width: w, height: h) }

    func testMandoVerticalNoEncogeYCreceConLaPantalla() {
        let b = UiScale.remoteBase
        let f = UiScale.remoteFixed
        XCTAssertEqual(UiScale.factor(sz(375, 667), base: b, fixed: f), 1)
        XCTAssertEqual(UiScale.factor(sz(393, 852), base: b, fixed: f), 1)
        XCTAssertEqual(UiScale.factor(sz(440, 956), base: b, fixed: f), 1, "el iPhone más grande sigue en 1")
        XCTAssertEqual(UiScale.factor(sz(744, 1133), base: b, fixed: f), 1.514, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(820, 1180), base: b, fixed: f), 1.589, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(1032, 1376), base: b, fixed: f), 1.899, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(2000, 3000), base: b, fixed: f), 2)
        XCTAssertEqual(UiScale.factor(sz(0, 0), base: b, fixed: f), 1)
    }

    func testMandoApaisado() {
        let b = UiScale.landscapeBase
        XCTAssertEqual(UiScale.factor(sz(852, 393), base: b), 1)
        XCTAssertEqual(UiScale.factor(sz(956, 440), base: b), 1)
        XCTAssertEqual(UiScale.factor(sz(1133, 744), base: b), 1.619, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(1180, 820), base: b), 1.686, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(1376, 1032), base: b), 1.966, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(5000, 3000), base: b), 2)
    }

    func testGamePad() {
        let b = UiScale.phoneLandscape
        XCTAssertEqual(UiScale.factor(sz(852, 393), base: b), 1)
        XCTAssertEqual(UiScale.factor(sz(1133, 744), base: b), 1.185, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(1180, 820), base: b), 1.234, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(1376, 1032), base: b), 1.439, accuracy: 0.001)
        XCTAssertEqual(UiScale.factor(sz(3000, 1500), base: b), 2)
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
        XCTAssertEqual(pad.grow, 1.589, accuracy: 0.001)
        XCTAssertEqual(pad.s, pad.grow)
        XCTAssertEqual(pad.colW, 520 * pad.grow, accuracy: 0.01)
        XCTAssertTrue(pad.flexible)
        XCTAssertEqual(pad.text(44), 44 * pad.grow, accuracy: 0.01)
        // iPad 13": la A pasa de 148 a 281 pt y la columna casi llena la pantalla
        let big = RemoteMetrics(size: sz(1032, 1376))
        XCTAssertEqual(big.big, 281, accuracy: 1)
        XCTAssertEqual(big.colW, 987, accuracy: 1)
        // NES apaisado en iPad 13"
        let nes = LandscapeMetrics(size: sz(1376, 1032))
        XCTAssertEqual(nes.s, 1.966, accuracy: 0.001)
        XCTAssertEqual(nes.cross, 190 * nes.s, accuracy: 0.01)
        XCTAssertEqual(LandscapeMetrics(size: sz(852, 393)).big, 92)
    }
}
