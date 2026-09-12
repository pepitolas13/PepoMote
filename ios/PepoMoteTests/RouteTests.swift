import XCTest
@testable import PepoMote

/// Routing del mando: función pura de enlace + intención (mismos casos que Android).
final class RouteTests: XCTestCase {
    private func connected(
        mode: String = LinkState.modePointer,
        role: String = LinkState.roleWiimote,
        pad: String = LinkState.padGamepad,
        slot: Int = 0,
        supportsCemu: Bool = true
    ) -> UiLink {
        .connected(ConnectedLink(pcName: "PC", mode: mode, rttMs: nil, sensorHz: 0, slot: slot, role: role, player: slot + 1, supportsCemu: supportsCemu, pad: pad))
    }

    func testGamePadConCemuConfirmado() {
        XCTAssertEqual(Route.route(connected(mode: "cemu"), .none), .gamePad)
        XCTAssertEqual(Route.route(connected(mode: "cemu", pad: "pro", slot: 1), .none), .gamePad)
        XCTAssertEqual(Route.route(connected(mode: "cemu"), .wiiU), .gamePad)
    }

    func testMandoDeWiiDentroDeWiiUEsLayoutWii() {
        XCTAssertEqual(Route.route(connected(mode: "cemu", pad: "wiimote"), .none), .wii)
    }

    func testGamePadOptimistaConIntencionPendiente() {
        XCTAssertEqual(Route.route(.connecting, .wiiU), .gamePad)
        XCTAssertEqual(Route.route(connected(mode: "pointer"), .wiiU), .gamePad)
        XCTAssertEqual(Route.route(connected(mode: "dolphin"), .wiiU), .gamePad)
    }

    func testNunchukSiempreNunchuk() {
        XCTAssertEqual(Route.route(connected(role: "nunchuk"), .none), .nunchuk)
        XCTAssertEqual(Route.route(connected(mode: "cemu", role: "nunchuk", pad: "nunchuk"), .none), .nunchuk)
        XCTAssertEqual(Route.route(connected(role: "nunchuk"), .wiiU), .nunchuk)
    }

    func testEnCualquierOtroCasoLayoutWii() {
        XCTAssertEqual(Route.route(connected(mode: "pointer"), .none), .wii)
        XCTAssertEqual(Route.route(connected(mode: "dolphin"), .none), .wii)
        XCTAssertEqual(Route.route(.connecting, .none), .wii)
        XCTAssertEqual(Route.route(.disconnected, .none), .wii)
        XCTAssertEqual(Route.route(.disconnected, .wiiU), .wii)
        XCTAssertEqual(Route.route(.failed(code: "io", msg: "x"), .wiiU), .wii)
    }

    func testReconectandoElGamePadSeQuedaSiHayIntencion() {
        let re = UiLink.reconnecting(pcName: "PC", attempt: 2)
        XCTAssertEqual(Route.route(re, .wiiU), .gamePad)
        XCTAssertEqual(Route.route(re, .none), .wii)
    }

    func testEcoCemuConsumeLaIntencionSinAviso() {
        let out = Route.afterModeEcho(.wiiU, "cemu", connected(mode: "cemu"))
        XCTAssertEqual(out.intent, .none)
        XCTAssertNil(out.warning)
    }

    func testDifusionDelPcConsumeLaIntencionSinAviso() {
        let out = Route.afterModeEcho(.wiiU, "dolphin", connected(mode: "dolphin", slot: 1), byPc: true)
        XCTAssertEqual(out.intent, .none)
        XCTAssertNil(out.warning)
        XCTAssertEqual(Route.afterModeEcho(.none, "pointer", connected(), byPc: true).intent, .none)
    }

    func testEcoPointerConIntencionPendienteVuelveAWiiConAviso() {
        let link = connected(mode: "pointer", supportsCemu: false)
        let out = Route.afterModeEcho(.wiiU, "pointer", link)
        XCTAssertEqual(out.intent, .none)
        XCTAssertEqual(out.warning, Route.warnNeeds13)
        XCTAssertEqual(Route.route(link, out.intent), .wii)
    }

    func testEcoDeOtroModoSiendoJugador2AvisaQueSoloElJugador1CambiaElModo() {
        let link = connected(mode: "pointer", slot: 1, supportsCemu: true)
        let out = Route.afterModeEcho(.wiiU, "pointer", link)
        XCTAssertEqual(out.intent, .none)
        XCTAssertEqual(out.warning, Route.warnPlayer1)
        XCTAssertEqual(Route.route(link, out.intent), .wii)
    }

    func testEcoSinIntencionNoCambiaNada() {
        let out = Route.afterModeEcho(.none, "pointer", connected(mode: "pointer"))
        XCTAssertEqual(out.intent, .none)
        XCTAssertNil(out.warning)
        XCTAssertNil(Route.afterModeEcho(.none, "cemu", connected(mode: "cemu")).warning)
    }

    func testEcoAntesDelOkConIntencionAvisaDePcAntiguo() {
        let out = Route.afterModeEcho(.wiiU, "pointer", .connecting)
        XCTAssertEqual(out.intent, .none)
        XCTAssertEqual(out.warning, Route.warnNeeds13)
    }

    func testIsGamePad() {
        XCTAssertTrue(Route.isGamePad(connected(mode: "cemu")))
        XCTAssertTrue(Route.isGamePad(connected(mode: "cemu", pad: "pro", slot: 2)))
        XCTAssertFalse(Route.isGamePad(connected(mode: "cemu", pad: "wiimote")))
        XCTAssertFalse(Route.isGamePad(connected(mode: "cemu", role: "nunchuk", pad: "nunchuk")))
        XCTAssertFalse(Route.isGamePad(connected(mode: "dolphin")))
        XCTAssertFalse(Route.isGamePad(.connecting))
    }

    /// Los avisos existen en los dos idiomas.
    func testLosAvisosTienenTexto() {
        for code in ["es", "en"] {
            let b = L10n.bundle(for: code)
            XCTAssertNotEqual(b.localizedString(forKey: Route.warnNeeds13, value: "", table: nil), "")
            XCTAssertNotEqual(b.localizedString(forKey: Route.warnPlayer1, value: "", table: nil), "")
        }
    }
}
