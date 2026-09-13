import XCTest
@testable import PepoMote

/// Medidas del GamePad: mismos vectores que Android (PadMetricsTest). Con
/// pantalla un iPhone mide lo de siempre y un iPad llena la columna lateral;
/// sin pantalla, en fila en el móvil y apilado (columnas anchas) en el iPad.
final class PadMetricsTests: XCTestCase {
    private func sz(_ w: CGFloat, _ h: CGFloat) -> CGSize { CGSize(width: w, height: h) }

    func testConPantallaUnIphoneMideLoDeSiempreYUnIpadLlenaLaColumna() {
        let phone = PadMetrics(size: sz(852, 393))
        XCTAssertEqual(phone.k, 1)
        XCTAssertFalse(phone.row)
        XCTAssertFalse(phone.noScreen)
        XCTAssertEqual(phone.padSize, 114.4, accuracy: 0.5)
        XCTAssertEqual(phone.sideW, 247.1, accuracy: 0.5)
        XCTAssertEqual(phone.touchW, 345.8, accuracy: 0.5)
        XCTAssertEqual(phone.faceBtn, 44, accuracy: 0.5)
        XCTAssertEqual(phone.clickSize, 34.3, accuracy: 0.5)
        XCTAssertEqual(phone.roundBtn, 40.9, accuracy: 0.5)
        XCTAssertEqual(phone.pillW, 66, accuracy: 0.5)
        XCTAssertEqual(phone.crossGlyph, 14)
        XCTAssertEqual(phone.shoulderH, 27.1, accuracy: 0.5)

        let ipad13 = PadMetrics(size: sz(1376, 1032))
        XCTAssertEqual(ipad13.k, 1.439, accuracy: 0.001)
        XCTAssertFalse(ipad13.row)
        XCTAssertEqual(ipad13.padSize, 329.7, accuracy: 0.5, "antes 247: el stick y L3 llenan la columna")
        XCTAssertEqual(ipad13.sideW, 399.0, accuracy: 0.5)
        XCTAssertEqual(ipad13.touchW, 565.9, accuracy: 0.5)
        XCTAssertEqual(ipad13.faceBtn, 126.8, accuracy: 0.5)
        XCTAssertEqual(ipad13.crossGlyph, 39.6, accuracy: 0.5)
        XCTAssertEqual(PadMetrics(size: sz(1180, 820)).padSize, 281.9, accuracy: 0.5)
        XCTAssertEqual(PadMetrics(size: sz(1133, 744)).padSize, 269.6, accuracy: 0.5)
    }

    func testSinPantallaEnFilaEnElMovilYApiladoEnElIpad() {
        let phone = PadMetrics(size: sz(852, 393), noScreen: true)
        XCTAssertTrue(phone.noScreen)
        XCTAssertTrue(phone.row)
        XCTAssertEqual(phone.padSize, 190.5, accuracy: 0.5)
        XCTAssertEqual(phone.sideW, 387, accuracy: 0.5)
        XCTAssertEqual(phone.centerW, 66, accuracy: 0.5)
        XCTAssertEqual(phone.faceBtn, 73.3, accuracy: 0.5)
        XCTAssertEqual(phone.clickSize, 44, accuracy: 0.5)
        XCTAssertEqual(phone.roundBtn, 40.9, accuracy: 0.5)
        XCTAssertEqual(phone.pillW, 66, accuracy: 0.5)
        // iPhone 16 con las zonas seguras: sigue en fila
        let safe = PadMetrics(size: sz(734, 393), noScreen: true)
        XCTAssertTrue(safe.row)
        XCTAssertEqual(safe.padSize, 161, accuracy: 0.5)
        XCTAssertEqual(safe.centerW, 66, accuracy: 0.5)

        let ipad13 = PadMetrics(size: sz(1376, 1032), noScreen: true)
        XCTAssertFalse(ipad13.row)
        XCTAssertEqual(ipad13.padSize, 377.6, accuracy: 0.5)
        XCTAssertEqual(ipad13.sideW, 446.9, accuracy: 0.5)
        XCTAssertEqual(ipad13.centerW, 470.2, accuracy: 0.5)
        let ipad11 = PadMetrics(size: sz(1180, 820), noScreen: true)
        XCTAssertFalse(ipad11.row)
        XCTAssertEqual(ipad11.padSize, 305.6, accuracy: 0.5, "lo limita la altura")
        let mini = PadMetrics(size: sz(1133, 744), noScreen: true)
        XCTAssertFalse(mini.row)
        XCTAssertEqual(mini.padSize, 269.6, accuracy: 0.5)
    }

    func testProControllerNuncaTienePantalla() {
        let phone = PadMetrics(size: sz(852, 393), pro: true)
        XCTAssertTrue(phone.noScreen)
        XCTAssertTrue(phone.pro)
        XCTAssertTrue(phone.row)
        XCTAssertEqual(phone.padSize, 196.8, accuracy: 0.5)
        XCTAssertEqual(phone.centerW, 40.9, accuracy: 0.5, "solo −, Home y + en columna")
        XCTAssertEqual(phone.pillW, 0)
        let ipad13 = PadMetrics(size: sz(1376, 1032), pro: true)
        XCTAssertFalse(ipad13.row)
        XCTAssertEqual(ipad13.padSize, 403.4, accuracy: 0.5, "lo limita la altura")
        XCTAssertEqual(ipad13.centerW, 260.2, accuracy: 0.5)
    }
}
