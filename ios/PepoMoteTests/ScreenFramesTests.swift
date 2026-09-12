import XCTest
@testable import PepoMote

/// Parser de tramas del canal de pantalla (PROTOCOL.md §4.4).
final class ScreenFramesTests: XCTestCase {
    /// `"PMPS"` · tipo · longitud u32 LE · carga (la longitud se puede falsear).
    static func frame(_ type: Int, _ payload: [UInt8], length: Int? = nil) -> [UInt8] {
        let len = UInt32(truncatingIfNeeded: length ?? payload.count)
        var out = Array("PMPS".utf8)
        out.append(UInt8(type))
        out.append(UInt8(len & 0xFF))
        out.append(UInt8((len >> 8) & 0xFF))
        out.append(UInt8((len >> 16) & 0xFF))
        out.append(UInt8((len >> 24) & 0xFF))
        out.append(contentsOf: payload)
        return out
    }

    private func bytes(_ n: Int, seed: Int = 1) -> [UInt8] {
        (0..<n).map { UInt8(($0 * 31 + seed) & 0xFF) }
    }

    private let keepalive = ScreenFramesTests.frame(ScreenFrames.typeKeepalive, [])

    func testCabecera() {
        let p = ScreenFrames()
        p.push(Self.frame(ScreenFrames.typeStatus, Array("hola".utf8)))
        let f = p.next()
        XCTAssertNotNil(f)
        XCTAssertEqual(f?.type, ScreenFrames.typeStatus)
        XCTAssertEqual(f?.payload.count, 4)
        XCTAssertEqual(String(decoding: f!.payload, as: UTF8.self), "hola")
        XCTAssertNil(p.next())
        XCTAssertEqual(p.pending, 0)
        XCTAssertEqual(p.resyncs, 0)
    }

    func testLongitudEsU32LittleEndian() {
        let payload = bytes(0x010203) // 66051 bytes
        let raw = Self.frame(ScreenFrames.typeImage, payload)
        XCTAssertEqual(raw[5], 0x03)
        XCTAssertEqual(raw[6], 0x02)
        XCTAssertEqual(raw[7], 0x01)
        XCTAssertEqual(raw[8], 0x00)
        let p = ScreenFrames()
        p.push(raw)
        let f = p.next()!
        XCTAssertEqual(f.type, ScreenFrames.typeImage)
        XCTAssertEqual([UInt8](f.payload), payload)
    }

    func testTramaPartidaEnVariosTrozos() {
        let payload = bytes(1000)
        let raw = Self.frame(ScreenFrames.typeImage, payload)
        let p = ScreenFrames()
        let cuts = [1, 3, 4, 8, 9, 500, 999, raw.count]
        var from = 0
        for to in cuts {
            p.push(raw, from, to - from)
            from = to
            if to < raw.count { XCTAssertNil(p.next(), "con \(to) bytes aún no hay trama") }
        }
        let f = p.next()
        XCTAssertNotNil(f)
        XCTAssertEqual([UInt8](f!.payload), payload)
        XCTAssertNil(p.next())
        XCTAssertEqual(p.resyncs, 0)
    }

    func testMagicIncorrectoResincroniza() {
        let payload = Array("ok".utf8)
        let p = ScreenFrames()
        let garbage = Array("XYZP! ".utf8)
        p.push(garbage)
        p.push(Self.frame(ScreenFrames.typeStatus, payload))
        let f = p.next()
        XCTAssertNotNil(f)
        XCTAssertEqual(f?.type, ScreenFrames.typeStatus)
        XCTAssertEqual([UInt8](f!.payload), payload)
        XCTAssertEqual(p.resyncs, 1)
        XCTAssertEqual(p.dropped, garbage.count)
        p.push(keepalive)
        XCTAssertEqual(p.next()?.type, ScreenFrames.typeKeepalive)
        XCTAssertEqual(p.resyncs, 1)
    }

    func testLongitudAbsurdaEsBasura() {
        let p = ScreenFrames()
        p.push(Self.frame(ScreenFrames.typeImage, [], length: -1))
        XCTAssertNil(p.next())
        p.push(Self.frame(ScreenFrames.typeStatus, Array("bien".utf8)))
        let f = p.next()!
        XCTAssertEqual(String(decoding: f.payload, as: UTF8.self), "bien")
        XCTAssertEqual(p.resyncs, 1)
        XCTAssertEqual(p.dropped, ScreenFrames.headerLen)
    }

    func testCargaMayorQueElTopeEsBasura() {
        let p = ScreenFrames(maxPayload: 100)
        p.push(Self.frame(ScreenFrames.typeImage, [UInt8](repeating: 0, count: 101)))
        XCTAssertNil(p.next())
        XCTAssertTrue(p.resyncs >= 1)
        p.push(Self.frame(ScreenFrames.typeImage, [UInt8](repeating: 0, count: 100)))
        XCTAssertEqual(p.next()?.payload.count, 100)
    }

    func testKeepaliveSinCarga() {
        XCTAssertEqual(keepalive.count, ScreenFrames.headerLen)
        let p = ScreenFrames()
        p.push(keepalive)
        p.push(keepalive)
        let f1 = p.next()!
        XCTAssertEqual(f1.type, ScreenFrames.typeKeepalive)
        XCTAssertEqual(f1.payload.count, 0)
        XCTAssertEqual(p.next()?.type, ScreenFrames.typeKeepalive)
        XCTAssertNil(p.next())
        p.push(keepalive, 0, 8)
        XCTAssertNil(p.next())
        p.push(keepalive, 8, 1)
        XCTAssertEqual(p.next()?.type, ScreenFrames.typeKeepalive)
        XCTAssertEqual(p.pending, 0)
    }

    func testVariasTramasSeguidasEnUnTrozo() {
        let img = bytes(300, seed: 7)
        let fourth = Self.frame(ScreenFrames.typeImage, img)
        let all = keepalive + Self.frame(ScreenFrames.typeStatus, Array("Esperando…".utf8)) + Self.frame(ScreenFrames.typeImage, img) + Array(fourth[0..<20])
        let p = ScreenFrames()
        p.push(all)
        XCTAssertEqual(p.next()?.type, ScreenFrames.typeKeepalive)
        let st = p.next()!
        XCTAssertEqual(st.type, ScreenFrames.typeStatus)
        XCTAssertEqual(String(decoding: st.payload, as: UTF8.self), "Esperando…")
        let im = p.next()!
        XCTAssertEqual(im.type, ScreenFrames.typeImage)
        XCTAssertEqual([UInt8](im.payload), img)
        XCTAssertNil(p.next())
        XCTAssertEqual(p.pending, 20)
        p.push(fourth, 20, fourth.count - 20)
        XCTAssertEqual([UInt8](p.next()!.payload), img)
        XCTAssertNil(p.next())
        XCTAssertEqual(p.resyncs, 0)
    }

    func testLineaDeRespuestaYLuegoTramas() {
        let p = ScreenFrames()
        p.push(Array("{\"m\":\"scr".utf8))
        XCTAssertNil(p.takeLine())
        p.push(Array("een\",\"ok\":true}\r\n".utf8) + keepalive)
        XCTAssertEqual(p.takeLine(), "{\"m\":\"screen\",\"ok\":true}")
        XCTAssertNil(p.takeLine())
        XCTAssertEqual(p.next()?.type, ScreenFrames.typeKeepalive)
        XCTAssertEqual(p.pending, 0)
    }

    func testElBufferCreceConTramasGrandesYSeReutiliza() {
        let big = bytes(700_000, seed: 3)
        let raw = Self.frame(ScreenFrames.typeImage, big)
        let p = ScreenFrames()
        var from = 0
        while from < raw.count {
            let n = min(65_536, raw.count - from)
            p.push(raw, from, n)
            from += n
        }
        let f = p.next()!
        XCTAssertEqual(f.payload.count, big.count)
        XCTAssertEqual([UInt8](f.payload), big)
        for i in 0..<500 {
            p.push(Self.frame(ScreenFrames.typeStatus, Array("n\(i)".utf8)))
            XCTAssertEqual(String(decoding: p.next()!.payload, as: UTF8.self), "n\(i)")
        }
        XCTAssertEqual(p.pending, 0)
        XCTAssertEqual(p.resyncs, 0)
    }

    func testResetVaciaLoAcumulado() {
        let p = ScreenFrames()
        p.push(Self.frame(ScreenFrames.typeStatus, Array("a".utf8)), 0, 5)
        XCTAssertEqual(p.pending, 5)
        p.reset()
        XCTAssertEqual(p.pending, 0)
        p.push(Self.frame(ScreenFrames.typeStatus, Array("b".utf8)))
        XCTAssertEqual(String(decoding: p.next()!.payload, as: UTF8.self), "b")
    }

    func testByteDeConfirmacion() {
        XCTAssertEqual(ScreenFrames.ack, 0x01)
    }

    func testRespuestaJsonSinParserGeneral() {
        XCTAssertTrue(ScreenClient.replyOk("{\"m\":\"screen\",\"ok\":true}"))
        XCTAssertTrue(ScreenClient.replyOk("{ \"ok\" : true , \"m\" : \"screen\" }"))
        XCTAssertFalse(ScreenClient.replyOk("{\"m\":\"screen\",\"ok\":false}"))
        XCTAssertFalse(ScreenClient.replyOk("{\"m\":\"err\",\"code\":\"bad_session\",\"msg\":\"no\"}"))
        XCTAssertEqual(ScreenClient.jsonString("{\"msg\":\"a \\\"b\\\" \\\\ c\\n\\/\"}", "msg"), "a \"b\" \\ c\n/")
        XCTAssertEqual(ScreenClient.jsonString("{\"msg\":\"\\u00f1\"}", "msg"), "ñ")
        XCTAssertNil(ScreenClient.jsonString("{\"m\":\"err\"}", "msg"))
        XCTAssertEqual(
            ScreenClient.helloLine(sessionId: 0xFFFF_FFFF, w: 640, h: 360, q: 70),
            "{\"m\":\"screen\",\"session_id\":4294967295,\"w\":640,\"h\":360,\"q\":70}"
        )
    }
}
