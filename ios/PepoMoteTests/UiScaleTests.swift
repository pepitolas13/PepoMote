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
        let f = RemoteFrame(size: sz(393, 852))
        XCTAssertEqual(f.grow, 1)
        XCTAssertEqual(f.colW, 393)
        XCTAssertFalse(f.flexible)
        XCTAssertEqual(f.text(44), 44)
        let m = RemoteMetrics.forScreen(sz(393, 852), headerH: 44)
        XCTAssertEqual(m.s, 1)
        XCTAssertEqual(m.cross, 168)
        XCTAssertEqual(m.big, 148)
        XCTAssertEqual(m.text(44), 44)
        // iPad 11": crece, columna de 520·grow y huecos flexibles
        let pad = RemoteFrame(size: sz(820, 1180))
        XCTAssertEqual(pad.grow, 1.589, accuracy: 0.001)
        XCTAssertEqual(pad.colW, 820, "520·grow (826) ya no cabe: la columna llena la pantalla")
        XCTAssertTrue(pad.flexible)
        XCTAssertEqual(pad.text(44), 44 * pad.grow, accuracy: 0.01)
        XCTAssertEqual(RemoteMetrics.forScreen(sz(820, 1180), headerH: 86).s, pad.grow)
        // iPad 13": la A pasa de 148 a 281 pt y la columna casi llena la pantalla
        let big = RemoteMetrics.forScreen(sz(1032, 1376), headerH: 44)
        XCTAssertEqual(big.big, 281, accuracy: 1)
        XCTAssertEqual(RemoteFrame(size: sz(1032, 1376)).colW, 987, accuracy: 1)
        // NES apaisado en iPad 13"
        let nes = LandscapeMetrics(size: sz(1376, 1032))
        XCTAssertEqual(nes.s, 1.966, accuracy: 0.001)
        XCTAssertEqual(nes.cross, 190 * nes.s, accuracy: 0.01)
        XCTAssertEqual(LandscapeMetrics(size: sz(852, 393)).big, 92)
    }
}
