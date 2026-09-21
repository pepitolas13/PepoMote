import XCTest
@testable import PepoMote

final class ReceiverCapabilitiesTests: XCTestCase {
    func testFrameRotationRequiresExplicitCapability() {
        XCTAssertFalse(ReceiverCapabilities().frameRotation)
        XCTAssertFalse(ReceiverCapabilities(ok: ["platform": "android"]).frameRotation)
        XCTAssertTrue(ReceiverCapabilities(ok: ["frame_rotation": true]).frameRotation)
    }
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

    /// `ok.rumble` se guarda tal cual (Ajustes lo traduce); ausente, o no
    /// cadena, es nil: receptor anterior a 1.10.5.
    func testLaVibracionDelReceptorSeGuardaTalCual() {
        XCTAssertNil(ReceiverCapabilities().rumble)
        XCTAssertNil(ReceiverCapabilities(ok: ["platform": "desktop"]).rumble)
        XCTAssertNil(ReceiverCapabilities(ok: ["rumble": 1]).rumble)
        XCTAssertEqual(ReceiverCapabilities(ok: ["rumble": "ready"]).rumble, "ready")
        XCTAssertEqual(ReceiverCapabilities(ok: ["platform": "android", "rumble": "unsupported"]).rumble, "unsupported")
        let state = LinkState()
        state.publish(.connected(ConnectedLink(pcName: "PC", mode: "dolphin", rttMs: nil, sensorHz: 0, receiver: ReceiverCapabilities(ok: ["rumble": "driver"]))))
        XCTAssertEqual(state.link.connected?.rumble, "driver")
        state.updateConnected { $0.mode = "pointer" }
        XCTAssertEqual(state.link.connected?.rumble, "driver", "los ecos de modo lo conservan")
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
