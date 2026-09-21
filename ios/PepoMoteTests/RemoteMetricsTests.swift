import XCTest
@testable import PepoMote

/// Medidas del mando vertical: en un iPhone normal y en iPad nada cambia; en
/// un iPhone bajo todo encoge a la vez hasta que la B cabe. Mismos vectores
/// que Android (WiiRemoteMetricsTest); lo fijo también suma 52 (aquí botón
/// multimedia 36 + hueco mínimo 4 + margen 12).
final class RemoteMetricsTests: XCTestCase {
    private func sz(_ w: CGFloat, _ h: CGFloat) -> CGSize { CGSize(width: w, height: h) }

    private func assertFits(_ m: RemoteMetrics) {
        XCTAssertLessThanOrEqual(m.bodyMin, m.bodyH + 0.01, "cuerpo \(m.bodyMin) en \(m.bodyH)")
        XCTAssertEqual(m.overflow, 0, accuracy: 0.001)
    }

    func testIphoneNormalNoCambia() {
        // iPhone 16: cabecera sola (44), con chips (86) y con multimedia abierta: escala 1 exacta
        let m = RemoteMetrics.forScreen(sz(393, 852), headerH: 44)
        XCTAssertEqual(m.s, 1)
        XCTAssertEqual(m.grow, 1)
        XCTAssertFalse(m.flexible)
        XCTAssertEqual(m.cross, 168)
        XCTAssertEqual(m.big, 148)
        XCTAssertEqual(m.one, 52)
        XCTAssertEqual(m.trigger, 88)
        XCTAssertEqual(m.gap(18), 18)
        XCTAssertEqual(m.text(20), 20)
        assertFits(m)
        XCTAssertEqual(RemoteMetrics.forScreen(sz(393, 852), headerH: 86).s, 1)
        XCTAssertEqual(RemoteMetrics.forScreen(sz(393, 852), headerH: 86, mediaOpen: true).s, 1)
        // El iPhone más grande, igual
        XCTAssertEqual(RemoteMetrics.forScreen(sz(440, 956), headerH: 130, mediaOpen: true).s, 1)
    }

    func testIphoneSeEncogeParaQueQuepaLaB() {
        // iPhone SE en modo puntero: lo justo para que quepa (antes, siempre 667/780)
        let m = RemoteMetrics.forScreen(sz(375, 667), headerH: 44)
        XCTAssertEqual(m.s, 0.944, accuracy: 0.001)
        XCTAssertEqual(m.trigger, 83.1, accuracy: 0.1)
        XCTAssertEqual(m.one, 49.1, accuracy: 0.1)
        XCTAssertEqual(m.text(20), 20, "el texto no se encoge")
        assertFits(m)
        // Con la fila multimedia abierta encoge un poco más en vez de echar la B fuera
        let open = RemoteMetrics.forScreen(sz(375, 667), headerH: 44, mediaOpen: true)
        XCTAssertEqual(open.s, 0.870, accuracy: 0.001)
        assertFits(open)
        // Con los chips de modo (cabecera 86)
        let chips = RemoteMetrics.forScreen(sz(375, 667), headerH: 86)
        XCTAssertEqual(chips.s, 0.874, accuracy: 0.001)
        assertFits(chips)
        // Wii U como Mando de Wii: chips + selector «En Cemu soy» con su ayuda (~200)
        let selector = RemoteMetrics.forScreen(sz(375, 667), headerH: 200)
        XCTAssertEqual(selector.s, 0.682, accuracy: 0.001)
        assertFits(selector)
    }

    func testPorDebajoDelSueloSeRecortaElCuerpoNoLaB() {
        // iPad a media pantalla: ni a 0,6 cabe; la pantalla recorta el cuerpo por abajo y la B sigue entera
        let m = RemoteMetrics.forScreen(sz(375, 440), headerH: 44)
        XCTAssertEqual(m.s, RemoteMetrics.floor)
        XCTAssertEqual(m.overflow, 22.4, accuracy: 0.1)
        XCTAssertEqual(m.trigger, 88 * RemoteMetrics.floor, accuracy: 0.001)
    }

    func testIpadCreceYNuncaEncoge() {
        // iPad 11": misma escala que UiScale, huecos flexibles, texto grande
        let grow = UiScale.factor(sz(820, 1180), base: UiScale.remoteBase, fixed: UiScale.remoteFixed)
        let m = RemoteMetrics.forScreen(sz(820, 1180), headerH: 86)
        XCTAssertEqual(grow, 1.589, accuracy: 0.001)
        XCTAssertEqual(m.s, grow)
        XCTAssertTrue(m.flexible)
        XCTAssertEqual(m.cross, 168 * grow, accuracy: 0.001)
        XCTAssertEqual(m.text(20), 20 * grow, accuracy: 0.001)
        XCTAssertEqual(RemoteMetrics.forScreen(sz(820, 1180), headerH: 86, mediaOpen: true).s, grow)
        // Un iPad pequeño con cabecera alta y multimedia abierta: crece igual, no encoge
        let small = RemoteMetrics.forScreen(sz(600, 830), headerH: 160, mediaOpen: true)
        XCTAssertGreaterThan(small.grow, 1)
        XCTAssertEqual(small.s, UiScale.factor(sz(600, 830), base: UiScale.remoteBase, fixed: UiScale.remoteFixed))
    }
}
