import XCTest
@testable import PepoMote

final class SwitchTests: XCTestCase {
    func testModeRequestClearsButtonsBothSticksAndTouch() {
        let state = LinkState()
        let buttons = ButtonState.shared
        defer { buttons.reset() }
        state.publish(.connected(ConnectedLink(pcName: "PC", mode: "cemu", rttMs: nil, sensorHz: 0)))
        var requested: String?
        state.sendMode = { requested = $0 }
        buttons.set(Btn.a, true)
        buttons.setStick(90, -40)
        buttons.setStick2(-50, 80)
        buttons.setTouch(200, 300, true)
        state.requestMode(LinkState.modeSwitch)
        XCTAssertEqual(requested, LinkState.modeSwitch)
        XCTAssertEqual(state.intent, .switchMode)
        XCTAssertEqual(buttons.current(), 0)
        XCTAssertEqual(buttons.stickX(), 0)
        XCTAssertEqual(buttons.stickRX(), 0)
        XCTAssertFalse(buttons.touch().down)
    }

    func testSwitchCapabilityAndLegacyAssignments() {
        XCTAssertTrue(ControlClient.supportsSwitch(["modes": ["pointer", "dolphin", "cemu", "switch"]]))
        XCTAssertFalse(ControlClient.supportsSwitch(["modes": ["cemu"]]))
        XCTAssertFalse(ControlClient.supportsSwitch([:]))
        let state = LinkState()
        for old in ["joycons", "joycon_side", "joycon_r", "pro"] {
            state.publish(.connected(ConnectedLink(pcName: "PC", mode: "switch", rttMs: nil, sensorHz: 0, player: 3, pad: old)))
            XCTAssertEqual(state.link.connected?.pad, "pro")
            XCTAssertEqual(state.link.connected?.player, 3)
            state.updateConnected { $0.pad = old }
            XCTAssertEqual(state.link.connected?.pad, "pro")
        }
    }

    func testPadPreferencesStaySeparateAndRejectInvalidValues() {
        let oldCemu = UserDefaults.standard.object(forKey: AppPrefs.cemuPadKey)
        let oldSwitch = UserDefaults.standard.object(forKey: AppPrefs.switchPadKey)
        defer {
            UserDefaults.standard.set(oldCemu, forKey: AppPrefs.cemuPadKey)
            UserDefaults.standard.set(oldSwitch, forKey: AppPrefs.switchPadKey)
        }
        AppPrefs.cemuPad = LinkState.padWiimote
        AppPrefs.switchPad = "joycon_r"
        XCTAssertEqual(AppPrefs.cemuPad, LinkState.padWiimote)
        XCTAssertEqual(AppPrefs.switchPad, LinkState.padPro)
        for old in ["joycons", "joycon_side", "joycon_r"] {
            UserDefaults.standard.set(old, forKey: AppPrefs.switchPadKey)
            XCTAssertEqual(AppPrefs.switchPad, "pro", "old saved choices recover on first launch")
        }
        AppPrefs.switchPad = LinkState.padGamepad
        XCTAssertEqual(AppPrefs.switchPad, LinkState.padPro)
        XCTAssertEqual(AppPrefs.cemuPad, LinkState.padWiimote)
        AppPrefs.cemuPad = "joycons"
        XCTAssertEqual(AppPrefs.cemuPad, LinkState.padGamepad)
    }

    func testSwitchPacketHasCaptureBothSticksAndNoTouch() {
        let packet = PmpCodec.encodeInput(
            sessionId: 1, seq: 1, tSensorUs: 1,
            quat: [1, 0, 0, 0], gyro: [0, 0, 0], accel: [0, 0, 9.8],
            buttons: Btn.screen, recenterCount: 0, batteryPct: 100, touchScrollDy: 0,
            flags: PmpCodec.flagQuatValid | PmpCodec.flagStickValid | PmpCodec.flagExt,
            stickX: 110, stickY: -80, stick2X: -65, stick2Y: 120
        )
        XCTAssertEqual(packet.count, 80)
        XCTAssertEqual(Btn.screen, 1 << 28)
        XCTAssertEqual(Int8(bitPattern: packet[6]), 110)
        XCTAssertEqual(Int8(bitPattern: packet[7]), -80)
        XCTAssertEqual(Int8(bitPattern: packet[72]), -65)
        XCTAssertEqual(Int8(bitPattern: packet[73]), 120)
        XCTAssertEqual(packet[5] & PmpCodec.flagTouch, 0)
        XCTAssertEqual(packet.subdata(in: 74..<80), Data(repeating: 0, count: 6))
    }

    func testSwitchHelloAndLegacyPadEchoNormalizeToPro() throws {
        let receiver = try FakeReceiver()
        defer { receiver.stop() }
        let okEvent = expectation(description: "switch ok")
        let padEvent = expectation(description: "normalized Pro")
        var received: ControlClient.Ok?
        var assignment: (String, Int?)?
        let client = ControlClient(
            host: "127.0.0.1", port: Int(receiver.port), token: "test",
            deviceName: "iPhone", deviceModel: "Apple iPhone", role: "wiimote",
            callbacks: ControlClient.Callbacks(
                onOk: { received = $0; okEvent.fulfill() },
                onError: { code, message in XCTFail("\(code): \(message)") },
                onModeChanged: { _, _ in },
                onPadChanged: { pad, player in assignment = (pad, player); padEvent.fulfill() },
                onNotice: { _ in }, onClosed: {}
            ), pad: "joycon_r"
        )
        defer { client.close() }
        let hello = try XCTUnwrap(receiver.nextLine())
        let fields = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(hello.utf8)) as? [String: Any])
        XCTAssertEqual(fields["pad"] as? String, "pro")
        receiver.sendLine("{\"m\":\"ok\",\"session_id\":1,\"mode\":\"switch\",\"modes\":[\"cemu\",\"switch\"],\"pad\":\"joycon_side\",\"half\":null,\"side\":\"right\"}")
        wait(for: [okEvent], timeout: 5)
        XCTAssertEqual(received?.supportsSwitch, true)
        XCTAssertEqual(received?.pad, "pro")
        receiver.sendLine("{\"m\":\"pad\",\"pad\":\"joycon_r\",\"half\":\"right\",\"side\":null,\"player\":1}")
        wait(for: [padEvent], timeout: 5)
        XCTAssertEqual(assignment?.0, "pro")
        XCTAssertEqual(assignment?.1, 1)
    }
}
