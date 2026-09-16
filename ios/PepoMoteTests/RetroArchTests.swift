import XCTest
@testable import PepoMote

/// Modo RetroArch: capacidad, mandos, rutas y teclas rápidas (mismos casos que Android).
final class RetroArchTests: XCTestCase {
    private func connected(
        mode: String = "retroarch",
        role: String = LinkState.roleWiimote,
        pad: String = "retropad",
        slot: Int = 0,
        supportsRetroArch: Bool = true,
        supportsSwitch: Bool = true
    ) -> UiLink {
        .connected(ConnectedLink(pcName: "PC", mode: mode, rttMs: nil, sensorHz: 0, slot: slot, role: role, player: slot + 1, supportsCemu: true, pad: pad, supportsSwitch: supportsSwitch, supportsRetroArch: supportsRetroArch))
    }

    func testCapabilityComesFromOkModes() {
        XCTAssertTrue(ControlClient.supportsRetroArch(["modes": ["pointer", "dolphin", "cemu", "switch", "retroarch"]]))
        XCTAssertFalse(ControlClient.supportsRetroArch(["modes": ["pointer", "dolphin", "cemu", "switch"]]))
        XCTAssertFalse(ControlClient.supportsRetroArch([:]))
    }

    func testOnlyTheRetroPadIsTheTwoStickScreen() {
        XCTAssertTrue(Route.isRetroArch(connected()))
        XCTAssertTrue(Route.isRetroPad(connected()))
        XCTAssertEqual(Route.route(connected(), .none), .gamePad)
        XCTAssertTrue(Route.forcesLandscape(connected(), .none))
        XCTAssertTrue(Route.extendedOperative(connected(), .none))
        XCTAssertEqual(Route.wantedMode(connected(), .none), "retroarch")
        // mando de NES y pistola: los layouts de Wii de siempre
        XCTAssertEqual(Route.route(connected(pad: "nes"), .none), .wii)
        XCTAssertTrue(Route.retroNes(connected(pad: "nes")))
        XCTAssertTrue(Route.forcesLandscape(connected(pad: "nes"), .none), "el mando de NES va fijo en apaisado")
        XCTAssertFalse(Route.extendedOperative(connected(pad: "nes"), .none))
        XCTAssertEqual(Route.route(connected(pad: "gun"), .none), .wii)
        XCTAssertTrue(Route.retroGun(connected(pad: "gun")))
        XCTAssertFalse(Route.forcesLandscape(connected(pad: "gun"), .none), "la pistola se sostiene derecha")
        // la cruceta nunca va girada en RetroArch: el receptor la mapea tal como se ve
        XCTAssertFalse(Route.sidewaysDpad(connected(pad: "nes")))
        XCTAssertFalse(Route.sidewaysDpad(connected(pad: "gun")))
        // un receptor sin RetroArch nunca da mando de RetroArch
        XCTAssertFalse(Route.isRetroArch(connected(supportsRetroArch: false)))
        XCTAssertFalse(Route.isRetroArch(connected(role: "nunchuk")))
        XCTAssertEqual(Route.route(connected(role: "nunchuk"), .none), .nunchuk)
    }

    func testPendingIntentOpensTheRetroPadOptimistically() {
        XCTAssertEqual(PadIntent.retroArch.mode, "retroarch")
        XCTAssertEqual(Route.wantedMode(connected(mode: "pointer"), .retroArch), "retroarch")
        XCTAssertEqual(Route.route(.connecting, .retroArch), .gamePad)
        XCTAssertEqual(Route.route(connected(mode: "pointer"), .retroArch), .gamePad)
        XCTAssertEqual(Route.route(.disconnected, .retroArch), .wii)
        XCTAssertFalse(Route.extendedOperative(connected(mode: "pointer"), .retroArch))
    }

    func testRejectedRetroArchRequestExplainsWhy() {
        let old = connected(mode: "pointer", supportsRetroArch: false)
        let rejected = Route.afterModeEcho(.retroArch, "pointer", old)
        XCTAssertEqual(rejected.intent, .none)
        XCTAssertEqual(rejected.warning, Route.warnNeedsRetroArch)
        XCTAssertEqual(Route.afterModeEcho(.retroArch, "pointer", connected(mode: "pointer", slot: 1)).warning, Route.warnPlayer1)
        XCTAssertNil(Route.afterModeEcho(.retroArch, "retroarch", connected()).warning)
        XCTAssertNil(Route.afterModeEcho(.retroArch, "dolphin", connected(), byPc: true).warning)
        XCTAssertEqual(Route.afterModeEcho(.switchMode, "pointer", connected(mode: "pointer", supportsSwitch: false)).warning, Route.warnNeedsSwitch)
    }

    func testModeRequestSetsTheIntentAndOnlyRetroArchPadsSurvive() {
        let state = LinkState()
        state.publish(.connected(ConnectedLink(pcName: "PC", mode: "pointer", rttMs: nil, sensorHz: 0, supportsRetroArch: true)))
        var requested: String?
        state.sendMode = { requested = $0 }
        state.requestMode(LinkState.modeRetroArch)
        XCTAssertEqual(requested, LinkState.modeRetroArch)
        XCTAssertEqual(state.intent, .retroArch)
        for pad in ["retropad", "nes", "gun"] {
            state.publish(.connected(ConnectedLink(pcName: "PC", mode: "retroarch", rttMs: nil, sensorHz: 0, pad: pad, supportsRetroArch: true)))
            XCTAssertEqual(state.link.connected?.pad, pad)
        }
        for bad in ["pro", "wiimote", "gamepad", "joycons"] {
            state.publish(.connected(ConnectedLink(pcName: "PC", mode: "retroarch", rttMs: nil, sensorHz: 0, pad: bad, supportsRetroArch: true)))
            XCTAssertEqual(state.link.connected?.pad, "retropad")
            state.updateConnected { $0.pad = bad }
            XCTAssertEqual(state.link.connected?.pad, "retropad")
        }
        // fuera de RetroArch sus mandos no se tocan
        state.publish(.connected(ConnectedLink(pcName: "PC", mode: "cemu", rttMs: nil, sensorHz: 0, pad: "wiimote")))
        XCTAssertEqual(state.link.connected?.pad, "wiimote")
    }

    func testRetroPadPreferenceRejectsInvalidValues() {
        let old = UserDefaults.standard.object(forKey: AppPrefs.retroPadKey)
        defer { UserDefaults.standard.set(old, forKey: AppPrefs.retroPadKey) }
        AppPrefs.retroPad = "gun"
        XCTAssertEqual(AppPrefs.retroPad, "gun")
        AppPrefs.retroPad = "pro"
        XCTAssertEqual(AppPrefs.retroPad, "retropad")
        UserDefaults.standard.set("nes", forKey: AppPrefs.retroPadKey)
        XCTAssertEqual(AppPrefs.retroPad, "nes")
        XCTAssertNotEqual(AppPrefs.retroPadKey, AppPrefs.switchPadKey)
    }

    func testHotkeyTableIsWellFormed() {
        XCTAssertEqual(Set(retroHotkeys.map(\.name)).count, retroHotkeys.count)
        XCTAssertTrue(retroHotkeys.first { $0.name == "rewind" }?.hold ?? false)
        XCTAssertFalse(retroHotkeys.first { $0.name == "save_state" }?.hold ?? true)
        XCTAssertEqual(retroHotkeys.count, 8)
    }
}
