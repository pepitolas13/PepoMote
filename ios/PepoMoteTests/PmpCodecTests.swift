import XCTest
@testable import PepoMote

/// Paridad con los vectores dorados de protocol/vectors/ (los mismos que
/// verifican el receptor Rust y la app Android). Si esto falla, el codec ha
/// divergido del spec.
final class PmpCodecTests: XCTestCase {
    private func vector(_ name: String) throws -> String {
        let url = try XCTUnwrap(Bundle(for: PmpCodecTests.self).url(forResource: name, withExtension: "hex"), "falta el vector \(name)")
        let text = try String(contentsOf: url, encoding: .utf8)
        return text.filter { !$0.isWhitespace }
    }

    /// Hex (sin espacios) → bytes, para los vectores que se decodifican.
    private func bytes(_ hex: String) -> Data {
        let chars = Array(hex)
        var d = Data(capacity: chars.count / 2)
        var i = 0
        while i + 1 < chars.count {
            d.append(UInt8(String(chars[i]) + String(chars[i + 1]), radix: 16) ?? 0)
            i += 2
        }
        return d
    }

    private let session: UInt32 = 0xAABB_CCDD

    func testInputNeutral() throws {
        let p = PmpCodec.encodeInput(
            sessionId: session, seq: 7, tSensorUs: 1_000_000,
            quat: [1, 0, 0, 0], gyro: [0, 0, 0], accel: [0, 0, 0],
            buttons: 0, recenterCount: 0, batteryPct: 100, touchScrollDy: 0
        )
        XCTAssertEqual(p.count, PmpCodec.inputLen)
        XCTAssertEqual(p.hexString, try vector("input_neutral"))
    }

    func testInputMotion() throws {
        let p = PmpCodec.encodeInput(
            sessionId: session, seq: 8, tSensorUs: 2_000_000,
            quat: [0.5, -0.5, 0.5, -0.5], gyro: [1, -1, 0.5], accel: [-1, 2, -0.5],
            buttons: 0x41, recenterCount: 1, batteryPct: 50, touchScrollDy: -12
        )
        XCTAssertEqual(p.hexString, try vector("input_motion"))
    }

    func testInputButtonsAll() throws {
        let p = PmpCodec.encodeInput(
            sessionId: session, seq: 9, tSensorUs: 3_000_000,
            quat: [1, 0, 0, 0], gyro: [0, 0, 0], accel: [0, 0, 0],
            buttons: 0x0001_FFFF, recenterCount: 3, batteryPct: 87, touchScrollDy: -120
        )
        XCTAssertEqual(p.hexString, try vector("input_buttons_all"))
    }

    /// Emisor Nunchuk: flags bit1, stick en los bytes 6-7, C y Z.
    func testInputNunchuk() throws {
        let p = PmpCodec.encodeInput(
            sessionId: session, seq: 10, tSensorUs: 4_000_000,
            quat: [1, 0, 0, 0], gyro: [0, 0, 0], accel: [0, 0, 9.5],
            buttons: 0x60000, recenterCount: 0, batteryPct: 77, touchScrollDy: 0,
            flags: PmpCodec.flagQuatValid | PmpCodec.flagStickValid,
            stickX: 100, stickY: -50
        )
        XCTAssertEqual(p.hexString, try vector("input_nunchuk"))
    }

    /// GamePad de Wii U: 80 bytes con flagExt, stick izquierdo en 6-7, stick
    /// derecho y táctil en 72-77, reservados 78-79 a 0, botones Wii U.
    func testInputWiiU() throws {
        let buttons: UInt32 = 0x1FF8_0001
        XCTAssertEqual(buttons, Btn.a | Btn.x | Btn.y | Btn.l | Btn.r | Btn.zl | Btn.zr | Btn.stickL | Btn.stickR | Btn.mic | Btn.screen)
        let p = PmpCodec.encodeInput(
            sessionId: session, seq: 11, tSensorUs: 5_000_000,
            quat: [1, 0, 0, 0], gyro: [0, 0, 0], accel: [0, 0, 9.5],
            buttons: buttons, recenterCount: 2, batteryPct: 66, touchScrollDy: 0,
            flags: PmpCodec.flagQuatValid | PmpCodec.flagStickValid | PmpCodec.flagExt | PmpCodec.flagTouch,
            stickX: 100, stickY: -50, stick2X: -30, stick2Y: 120, touchX: 0x8000, touchY: 0x4000
        )
        XCTAssertEqual(p.count, PmpCodec.inputExtLen)
        XCTAssertEqual(p.hexString, try vector("input_wiiu"))
    }

    /// Sin flagExt el bloque Wii U no existe: 72 bytes, pase lo que se pase.
    func testExtensionSoloConSuFlag() throws {
        let p = PmpCodec.encodeInput(
            sessionId: session, seq: 10, tSensorUs: 4_000_000,
            quat: [1, 0, 0, 0], gyro: [0, 0, 0], accel: [0, 0, 9.5],
            buttons: 0x60000, recenterCount: 0, batteryPct: 77, touchScrollDy: 0,
            flags: PmpCodec.flagQuatValid | PmpCodec.flagStickValid,
            stickX: 100, stickY: -50, stick2X: -30, stick2Y: 120, touchX: 0x8000, touchY: 0x4000
        )
        XCTAssertEqual(p.count, PmpCodec.inputLen)
        XCTAssertEqual(p.hexString, try vector("input_nunchuk"))
        // Con flagExt y el táctil en el tope: u16 sin signo, reservados a 0
        let ext = PmpCodec.encodeInput(
            sessionId: 1, seq: 1, tSensorUs: 1,
            quat: [1, 0, 0, 0], gyro: [0, 0, 0], accel: [0, 0, 0],
            buttons: 0, recenterCount: 0, batteryPct: 100, touchScrollDy: 0,
            flags: PmpCodec.flagExt, stick2X: -1, stick2Y: 2, touchX: 65535, touchY: 70000
        )
        XCTAssertEqual(ext.count, PmpCodec.inputExtLen)
        XCTAssertEqual(ext.subdata(in: 72..<80).hexString, "ff02ffffffff0000")
    }

    func testPingPong() throws {
        XCTAssertEqual(PmpCodec.encodePing(sessionId: session, tUs: 0x0102_0304_0506_0708).hexString, try vector("ping"))
        XCTAssertEqual(PmpCodec.encodePong(sessionId: session, tUs: 0x0102_0304_0506_0708).hexString, try vector("pong"))
    }

    func testParseDePingYPong() {
        let ping = PmpCodec.encodePing(sessionId: session, tUs: 123_456_789)
        XCTAssertEqual(PmpCodec.packetType(ping), PmpCodec.typePing)
        XCTAssertEqual(PmpCodec.pingSession(ping), session)
        XCTAssertEqual(PmpCodec.pingT(ping), 123_456_789)
        XCTAssertNil(PmpCodec.packetType(Data("PMPDISCOVER1".utf8)), "sin magic no es PMP")
        XCTAssertNil(PmpCodec.packetType(Data([0x50, 0x4D, 0x50])), "demasiado corto")
        // Un trozo de Data con startIndex ≠ 0 se lee igual
        var padded = Data([0xAA, 0xBB])
        padded.append(ping)
        XCTAssertEqual(PmpCodec.packetType(padded.subdata(in: 2..<padded.count)), PmpCodec.typePing)
        XCTAssertEqual(PmpCodec.pingT(padded[2...]), 123_456_789)
    }

    /// RUMBLE del receptor (§4.5): los dos vectores dorados se decodifican a
    /// sus valores, y ni 19 ni 21 bytes, ni otro magic, ni un PING (que mide
    /// lo mismo) pasan por RUMBLE.
    func testRumbleVectors() throws {
        let on = bytes(try vector("rumble_on"))
        XCTAssertEqual(on.count, PmpCodec.rumbleLen)
        XCTAssertEqual(PmpCodec.packetType(on), PmpCodec.typeRumble)
        let r = try XCTUnwrap(PmpCodec.decodeRumble(on))
        XCTAssertEqual(r, PmpCodec.Rumble(sessionId: session, seq: 42, strong: 255, weak: 128, ttlMs: 400))
        let off = try XCTUnwrap(PmpCodec.decodeRumble(bytes(try vector("rumble_off"))))
        XCTAssertEqual(off, PmpCodec.Rumble(sessionId: session, seq: 43, strong: 0, weak: 0, ttlMs: 0))
        XCTAssertNil(PmpCodec.decodeRumble(on.subdata(in: 0..<19)), "19 bytes")
        var long = on
        long.append(0)
        XCTAssertNil(PmpCodec.decodeRumble(long), "21 bytes")
        var badMagic = on
        badMagic[0] = 0x00
        XCTAssertNil(PmpCodec.decodeRumble(badMagic), "otro magic")
        XCTAssertNil(PmpCodec.decodeRumble(PmpCodec.encodePing(sessionId: session, tUs: 1)), "un PING no es RUMBLE")
        // Un trozo de Data con startIndex ≠ 0 se lee igual
        var padded = Data([0xAA, 0xBB])
        padded.append(on)
        XCTAssertEqual(PmpCodec.decodeRumble(padded[2...]), r)
    }

    func testRecorteDeCampos() {
        let p = PmpCodec.encodeInput(
            sessionId: 1, seq: 1, tSensorUs: 1,
            quat: [1, 0, 0, 0], gyro: [0, 0, 0], accel: [0, 0, 0],
            buttons: 0, recenterCount: 0x1FF, batteryPct: 150, touchScrollDy: 40_000,
            flags: PmpCodec.flagStickValid, stickX: 300, stickY: -300
        )
        XCTAssertEqual(Int8(bitPattern: p[6]), 127)
        XCTAssertEqual(Int8(bitPattern: p[7]), -127)
        XCTAssertEqual(p[68], 0xFF)
        XCTAssertEqual(p[69], 100)
        XCTAssertEqual(UInt16(p[70]) | (UInt16(p[71]) << 8), 0x7FFF)
    }
}
