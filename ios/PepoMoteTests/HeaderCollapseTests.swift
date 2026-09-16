import XCTest
@testable import PepoMote

/// Cabecera plegable de los mandos apaisados: cuándo se pliega sola, cuándo se
/// abre sola y cuánto mide su pastilla (mismos casos que Android
/// `HeaderCollapseTest`).
final class HeaderCollapseTests: XCTestCase {
    func testSePliegaSolaSoloConElMandoEnJuego() {
        XCTAssertTrue(HeaderCollapse.autoCollapses(connected: true, screenReader: false))
        XCTAssertFalse(HeaderCollapse.autoCollapses(connected: false, screenReader: false), "sin PC, «Salir» se queda a la vista")
        XCTAssertFalse(HeaderCollapse.autoCollapses(connected: true, screenReader: true), "con lector de pantalla, nunca sola")
        XCTAssertFalse(HeaderCollapse.autoCollapses(connected: false, screenReader: true))
        XCTAssertEqual(HeaderCollapse.autoSeconds, 4.0, accuracy: 0.001, "cuatro segundos, los 4000 ms de Android")
    }

    func testSoloSeAbreSolaAlQuedarseSinPC() {
        XCTAssertTrue(HeaderCollapse.expandsOn(.disconnected))
        XCTAssertFalse(
            HeaderCollapse.expandsOn(.reconnecting(pcName: "SALON", attempt: 2)),
            "reconectando el mando sigue en la mano: la tarjeta taparía los botones"
        )
        XCTAssertFalse(HeaderCollapse.expandsOn(.connecting))
        XCTAssertFalse(HeaderCollapse.expandsOn(.failed(code: "bad_code", msg: "…")))
        XCTAssertFalse(HeaderCollapse.expandsOn(.connected(ConnectedLink(pcName: "SALON", mode: LinkState.modeDolphin, rttMs: 12, sensorHz: 100))))
    }

    func testLaPastillaNiSeEncogeNiSeDesborda() {
        XCTAssertEqual(HeaderCollapse.handleWidth(textWidth: 0), 96, accuracy: 0.01, "nunca menos de 96")
        XCTAssertEqual(HeaderCollapse.handleWidth(textWidth: 40), 96, accuracy: 0.01)
        XCTAssertEqual(HeaderCollapse.handleWidth(textWidth: 100), 144, accuracy: 0.01, "texto + 44")
        XCTAssertEqual(HeaderCollapse.handleWidth(textWidth: 400), 240, accuracy: 0.01, "nunca más de 240")
        // La caja táctil es de botón (44) y la píldora que se ve, más baja
        XCTAssertEqual(HeaderCollapse.handleHeight, 44)
        XCTAssertEqual(HeaderCollapse.pillHeight, 26)
        XCTAssertLessThan(HeaderCollapse.pillHeight, HeaderCollapse.handleHeight)
    }
}
