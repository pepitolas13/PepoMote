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

    func testGameMessageAndLayoutChoice() {
        let g = RetroGame.parse(["m": "game", "console": "md", "system": "Mega Drive", "core": "Genesis Plus GX", "title": "Cave Story", "path": "C:\\r\\cs.zip"])
        XCTAssertEqual(g?.console, "md")
        XCTAssertEqual(g?.title, "Cave Story")
        XCTAssertEqual(g?.core, "Genesis Plus GX")
        XCTAssertNil(RetroGame.parse(["m": "game", "console": NSNull(), "system": "", "core": "", "title": "", "path": ""]), "sin juego")
        XCTAssertNil(RetroGame.parse(["m": "game", "console": NSNull(), "core": "dosbox_pure", "title": "game", "path": "g.zip"])?.console)
        // elección por juego y global, con tope
        var c = RetroLayoutChoice()
        XCTAssertNil(c.choiceFor("a.zip"))
        c = c.pick(path: nil, id: "md3")
        XCTAssertEqual(c.choiceFor("a.zip"), "md3")
        c = c.pick(path: "a.zip", id: "nes")
        XCTAssertEqual(c.choiceFor("a.zip"), "nes")
        XCTAssertEqual(c.choiceFor("b.zip"), "md3")
        c = c.pick(path: "a.zip", id: nil)
        XCTAssertNil(c.choiceFor("a.zip"))
        XCTAssertNil(c.choiceFor(nil))
        for i in 0..<60 { c = c.pick(path: "g\(i).zip", id: "snes") }
        XCTAssertEqual(c.byPath.count, RetroLayoutChoice.cap)
        XCTAssertNil(c.choiceFor("g0.zip"))
        XCTAssertEqual(c.choiceFor("g59.zip"), "snes")
        XCTAssertEqual(RetroLayoutChoice.decode(c.encode()), c)
        XCTAssertEqual(RetroLayoutChoice.decode("{no"), RetroLayoutChoice())
        // la plantilla efectiva
        XCTAssertEqual(RetroLayouts.effective(console: nil, chosen: nil), "retropad")
        XCTAssertEqual(RetroLayouts.effective(console: "md", chosen: nil), "md")
        XCTAssertEqual(RetroLayouts.effective(console: "md", chosen: "nes"), "nes")
        XCTAssertEqual(RetroLayouts.effective(console: "dos", chosen: nil), "retropad")
        XCTAssertEqual(RetroLayouts.effective(console: "md3", chosen: nil), "retropad")
        let state = LinkState()
        var link = ConnectedLink(pcName: "PC", mode: LinkState.modeRetroArch, rttMs: nil, sensorHz: 0, pad: LinkState.padRetroPad)
        link.game = g
        XCTAssertEqual(state.effectiveRetroLayout(link), "md")
        link.pad = LinkState.padNes
        XCTAssertEqual(state.effectiveRetroLayout(link), "retropad", "solo siendo el RetroPad")
        XCTAssertEqual(state.wouldBeRetroLayout(link), "md", "lo que tocaría, para el selector")
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
        XCTAssertFalse(Route.forcesLandscape(connected(pad: "nes"), .none), "el mando Wii permite vertical y ambos apaisados")
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

    func testPointerKeyboardAndSensorRotation() {
        for pad in ["nes", "gun"] {
            let link = connected(pad: pad)
            for rotation in [Frame.rotation0, Frame.rotation90, Frame.rotation180, Frame.rotation270] {
                XCTAssertEqual(Route.sidewaysRotation(link, displayRotation: rotation), rotation)
            }
            XCTAssertTrue(Route.holdsPointerForKeyboard(link))
            XCTAssertFalse(Route.clicksBeforeKeyboard(link, true))
            XCTAssertFalse(Route.holdsPointerForKeyboard(connected(pad: pad, slot: 1)))
            XCTAssertFalse(Route.holdsPointerForKeyboard(connected(role: "nunchuk", pad: pad)))
            var android = link.connected!
            android.receiver = ReceiverCapabilities(ok: ["platform": "android"])
            XCTAssertFalse(Route.holdsPointerForKeyboard(.connected(android)))
        }
        XCTAssertFalse(Route.holdsPointerForKeyboard(connected()))
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
