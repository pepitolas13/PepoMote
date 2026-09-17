import XCTest
@testable import PepoMote

final class ReceiverCapabilitiesTests: XCTestCase {
    func testElPcAntiguoConservaPistolaYTeclado() {
        let cases: [[String: Any]] = [[:], ["platform": "desktop"], ["platform": "otro", "text_input": "false"]]
        for ok in cases {
            let receiver = ReceiverCapabilities(ok: ok)
            XCTAssertEqual(receiver.platform, "desktop")
            XCTAssertTrue(receiver.textInput)
            XCTAssertTrue(receiver.supportsPointer)
            XCTAssertEqual(receiver.retroPads, ["retropad", "nes", "gun"])
            XCTAssertEqual(receiver.restoredPad("gun"), "gun")
        }
        XCTAssertFalse(ReceiverCapabilities(ok: ["text_input": false]).textInput)
    }

    func testAndroidNoAdmiteTecladoNiPistolaAunqueElCampoDeTextoFalte() {
        let cases: [[String: Any]] = [["platform": "android"], ["platform": "ANDROID", "text_input": true]]
        for ok in cases {
            let receiver = ReceiverCapabilities(ok: ok)
            XCTAssertEqual(receiver.platform, "android")
            XCTAssertFalse(receiver.textInput)
            XCTAssertFalse(receiver.supportsPointer)
            XCTAssertEqual(receiver.retroPads, ["retropad", "nes"])
            XCTAssertEqual(receiver.restoredPad("gun"), "retropad")
            XCTAssertEqual(receiver.restoredPad("nes"), "nes")
            XCTAssertEqual(receiver.restoredPad("pro"), "pro")
        }
    }

    func testRestaurarLaPistolaEnAndroidNoCambiaLaPreferenciaDelPc() {
        let previous = UserDefaults.standard.object(forKey: AppPrefs.retroPadKey)
        defer { UserDefaults.standard.set(previous, forKey: AppPrefs.retroPadKey) }
        AppPrefs.retroPad = "gun"
        let receiver = ReceiverCapabilities(ok: ["platform": "android", "text_input": false])
        XCTAssertEqual(receiver.restoredPad(AppPrefs.retroPad), "retropad")
        XCTAssertEqual(AppPrefs.retroPad, "gun")
        XCTAssertEqual(ReceiverCapabilities().restoredPad(AppPrefs.retroPad), "gun")
    }

    func testCambiarModoYPadConservaCapacidadesYRutasSinVeloPendiente() {
        let state = LinkState()
        let receiver = ReceiverCapabilities(ok: ["platform": "android", "text_input": false])
        state.publish(.connected(ConnectedLink(pcName: "Android", mode: "dolphin", rttMs: nil, sensorHz: 0, supportsRetroArch: true, receiver: receiver)))
        state.sendMode = { _ in }
        state.requestMode("retroarch")
        state.updateConnected { $0.mode = "retroarch"; $0.pad = "gun" }
        state.resolveIntent(mode: "retroarch", byPc: false)
        XCTAssertEqual(state.intent, .none)
        XCTAssertEqual(state.link.connected?.pad, "retropad")
        XCTAssertEqual(Route.route(state.link, state.intent), .gamePad)
        XCTAssertTrue(Route.extendedOperative(state.link, state.intent))
        XCTAssertFalse(state.link.connected?.hasKeyboard ?? true)
        for mode in ["switch", "dolphin", "retroarch"] {
            state.updateConnected { $0.mode = mode }
            XCTAssertEqual(state.link.connected?.receiver, receiver)
        }
        state.updateConnected { $0.pad = "nes"; $0.player = 2 }
        XCTAssertEqual(state.link.connected?.receiver, receiver)
        XCTAssertEqual(Route.route(state.link, .none), .wii)
        XCTAssertTrue(Route.retroNes(state.link))
        state.updateConnected { $0.pad = "gun" }
        XCTAssertEqual(state.link.connected?.pad, "retropad")
    }
}
