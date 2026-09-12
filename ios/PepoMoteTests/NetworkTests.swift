import Network
import UIKit
import XCTest
@testable import PepoMote

/// Receptor falso en 127.0.0.1 (TCP por líneas) para probar los clientes de
/// control y de pantalla de extremo a extremo, sin PC.
final class FakeReceiver {
    let listener: NWListener
    private(set) var port: UInt16 = 0
    private let queue = DispatchQueue(label: "fake-receiver")
    private var conn: NWConnection?
    private var buffer = Data()
    private var lineWaiters: [(String) -> Void] = []
    private var lines: [String] = []
    private var rawWaiters: [(Data) -> Void] = []
    private var raw = Data()
    /// Tras la primera línea, el resto de bytes llega crudo (confirmaciones de un byte).
    var rawMode = false

    init() throws {
        let l = try NWListener(using: .tcp, on: .any)
        listener = l
        let box = PortBox()
        let e = XCTestExpectation(description: "port")
        l.stateUpdateHandler = { st in
            if case .ready = st, let lp = l.port {
                box.port = lp.rawValue
                e.fulfill()
            }
        }
        l.newConnectionHandler = { [weak self] c in self?.accept(c) }
        l.start(queue: queue)
        _ = XCTWaiter().wait(for: [e], timeout: 5)
        port = box.port
    }

    private final class PortBox {
        var port: UInt16 = 0
    }

    private func accept(_ c: NWConnection) {
        conn?.cancel()
        conn = c
        buffer = Data()
        raw = Data()
        c.start(queue: queue)
        receive(c)
    }

    private func receive(_ c: NWConnection) {
        c.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, complete, _ in
            guard let self, self.conn === c else { return }
            if let data, !data.isEmpty {
                if self.rawMode {
                    self.raw.append(data)
                    self.flushRaw()
                } else {
                    self.buffer.append(data)
                    while let nl = self.buffer.firstIndex(of: 0x0A) {
                        let line = String(decoding: self.buffer[self.buffer.startIndex..<nl], as: UTF8.self)
                        self.buffer.removeSubrange(self.buffer.startIndex...nl)
                        self.lines.append(line)
                    }
                    self.flushLines()
                    if self.rawMode, !self.buffer.isEmpty {
                        self.raw.append(self.buffer)
                        self.buffer = Data()
                        self.flushRaw()
                    }
                }
            }
            if !complete { self.receive(c) }
        }
    }

    private func flushLines() {
        while !lines.isEmpty, !lineWaiters.isEmpty {
            let l = lines.removeFirst()
            lineWaiters.removeFirst()(l)
        }
    }

    private func flushRaw() {
        while !raw.isEmpty, !rawWaiters.isEmpty {
            let b = raw.prefix(1)
            raw.removeFirst()
            rawWaiters.removeFirst()(Data(b))
        }
    }

    /// Siguiente línea que manda el cliente (o nil si no llega a tiempo).
    func nextLine(timeout: TimeInterval = 5) -> String? {
        var out: String?
        let e = XCTestExpectation(description: "line")
        queue.async {
            self.lineWaiters.append { l in
                out = l
                e.fulfill()
            }
            self.flushLines()
        }
        _ = XCTWaiter().wait(for: [e], timeout: timeout)
        return out
    }

    /// Siguiente byte crudo (confirmación) o nil.
    func nextByte(timeout: TimeInterval = 5) -> UInt8? {
        var out: UInt8?
        let e = XCTestExpectation(description: "byte")
        queue.async {
            self.rawWaiters.append { d in
                out = d.first
                e.fulfill()
            }
            self.flushRaw()
        }
        _ = XCTWaiter().wait(for: [e], timeout: timeout)
        return out
    }

    /// Cuántos bytes crudos hay sin consumir (tras una pequeña espera).
    func pendingBytes(wait: TimeInterval = 0.3) -> Int {
        Thread.sleep(forTimeInterval: wait)
        var n = 0
        queue.sync { n = raw.count }
        return n
    }

    func send(_ data: Data) {
        queue.async { self.conn?.send(content: data, completion: .contentProcessed { _ in }) }
    }

    func sendLine(_ s: String) { send(Data((s + "\n").utf8)) }

    func closeConnection() {
        queue.async {
            self.conn?.cancel()
            self.conn = nil
        }
    }

    func stop() {
        closeConnection()
        listener.cancel()
    }
}

final class ControlClientTests: XCTestCase {
    func testHelloOkModoAvisoYCierre() throws {
        let rx = try FakeReceiver()
        defer { rx.stop() }
        let okE = expectation(description: "ok")
        let modeE = expectation(description: "mode")
        let padE = expectation(description: "pad")
        let noticeE = expectation(description: "notice")
        let closedE = expectation(description: "closed")
        var got: ControlClient.Ok?
        var mode: (String, Bool)?
        var pad: String?
        var notice: String?
        let client = ControlClient(
            host: "127.0.0.1", port: Int(rx.port), token: "tok", deviceName: "iPad de prueba", deviceModel: "Apple iPad", role: "wiimote",
            callbacks: ControlClient.Callbacks(
                onOk: { got = $0; okE.fulfill() },
                onError: { code, msg in XCTFail("error \(code): \(msg)") },
                onModeChanged: { m, byPc in mode = (m, byPc); modeE.fulfill() },
                onPadChanged: { pad = $0; padE.fulfill() },
                onNotice: { notice = $0; noticeE.fulfill() },
                onClosed: { closedE.fulfill() }
            )
        )
        let hello = try XCTUnwrap(rx.nextLine())
        let obj = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(hello.utf8)) as? [String: Any])
        XCTAssertEqual(obj["m"] as? String, "hello")
        XCTAssertEqual(obj["pv"] as? Int, 1)
        XCTAssertEqual(obj["token"] as? String, "tok")
        XCTAssertEqual(obj["name"] as? String, "iPad de prueba")
        XCTAssertEqual(obj["model"] as? String, "Apple iPad")
        XCTAssertNil(obj["role"], "wiimote no se manda (receptores antiguos)")
        rx.sendLine("{\"m\":\"ok\",\"session_id\":4294967295,\"udp_port\":26761,\"mode\":\"pointer\",\"slot\":1,\"name\":\"SALON\",\"role\":\"wiimote\",\"player\":2,\"modes\":[\"pointer\",\"dolphin\",\"cemu\"],\"pad\":\"pro\"}")
        wait(for: [okE], timeout: 5)
        let ok = try XCTUnwrap(got)
        XCTAssertEqual(ok.sessionId, 0xFFFF_FFFF)
        XCTAssertEqual(ok.udpPort, 26761)
        XCTAssertEqual(ok.slot, 1)
        XCTAssertEqual(ok.player, 2)
        XCTAssertEqual(ok.name, "SALON")
        XCTAssertEqual(ok.pad, "pro")
        XCTAssertTrue(ok.supportsCemu)
        // Ping del receptor → pong con el mismo t
        rx.sendLine("{\"m\":\"ping\",\"t\":42}")
        let pong = try XCTUnwrap(rx.nextLine())
        XCTAssertTrue(pong.contains("\"pong\"") && pong.contains("42"), pong)
        rx.sendLine("{\"m\":\"mode\",\"mode\":\"cemu\",\"by\":\"pc\"}")
        rx.sendLine("{\"m\":\"pad\",\"pad\":\"wiimote\"}")
        rx.sendLine("no es json")
        rx.sendLine("{\"m\":\"notice\",\"text\":\"Cemu está abierto\"}")
        wait(for: [modeE, padE, noticeE], timeout: 5)
        XCTAssertEqual(mode?.0, "cemu")
        XCTAssertEqual(mode?.1, true)
        XCTAssertEqual(pad, "wiimote")
        XCTAssertEqual(notice, "Cemu está abierto")
        client.sendMode("dolphin")
        XCTAssertEqual(rx.nextLine(), "{\"m\":\"mode\",\"mode\":\"dolphin\"}")
        client.sendText("Link\n")
        XCTAssertEqual(rx.nextLine(), "{\"m\":\"text\",\"text\":\"Link\\n\"}")
        client.close()
        XCTAssertEqual(rx.nextLine(), "{\"m\":\"bye\"}")
        wait(for: [closedE], timeout: 5)
    }

    func testErrDelReceptor() throws {
        let rx = try FakeReceiver()
        defer { rx.stop() }
        let errE = expectation(description: "err")
        let closedE = expectation(description: "closed")
        var code: String?
        let client = ControlClient(
            host: "127.0.0.1", port: Int(rx.port), token: "viejo", deviceName: "x", deviceModel: "y", role: "nunchuk",
            callbacks: ControlClient.Callbacks(
                onOk: { _ in XCTFail("no debería haber ok") },
                onError: { c, _ in code = c; errE.fulfill() },
                onModeChanged: { _, _ in }, onPadChanged: { _ in }, onNotice: { _ in },
                onClosed: { closedE.fulfill() }
            )
        )
        let hello = try XCTUnwrap(rx.nextLine())
        XCTAssertTrue(hello.contains("\"role\":\"nunchuk\""), hello)
        rx.sendLine("{\"m\":\"err\",\"code\":\"bad_token\",\"msg\":\"Vuelve a escanear el QR\"}")
        wait(for: [errE, closedE], timeout: 5)
        XCTAssertEqual(code, "bad_token")
        _ = client
    }

    func testSupportsCemu() {
        XCTAssertTrue(ControlClient.supportsCemu(["modes": ["pointer", "cemu"]]))
        XCTAssertFalse(ControlClient.supportsCemu(["modes": ["pointer", "dolphin"]]))
        XCTAssertFalse(ControlClient.supportsCemu([:]))
    }
}

final class ScreenClientTests: XCTestCase {
    private let jpeg: [UInt8] = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 0xFF, 0xD9]

    /// Decodificador falso: cualquier carga que no empiece por 0 «decodifica».
    private static func fakeDecode(_ d: Data) -> UIImage? {
        d.first == 0 ? nil : UIImage()
    }

    private func client(_ rx: FakeReceiver, fallback: Int = 5000, reply: Int = 3000, data: Int = 6000, retryDrop: Int = 1000, retryUnavailable: Int = 5000) -> ScreenClient {
        ScreenClient(
            host: "127.0.0.1", port: Int(rx.port), sessionId: 0x1234_5678, maxWidth: 854, maxHeight: 480,
            replyTimeoutMs: reply, dataTimeoutMs: data, shownFallbackMs: fallback,
            retryAfterDropMs: retryDrop, retryUnavailableMs: retryUnavailable, decode: ScreenClientTests.fakeDecode
        )
    }

    private func wait(for c: ScreenClient, timeout: TimeInterval = 5, _ pred: @escaping (ScreenClient) -> Bool) -> Bool {
        let end = Date().addingTimeInterval(timeout)
        while Date() < end {
            if pred(c) { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
        return false
    }

    func testLineaScreenOkImagenYConfirmacionTrasPintar() throws {
        let rx = try FakeReceiver()
        defer { rx.stop() }
        let c = client(rx)
        c.start()
        XCTAssertEqual(rx.nextLine(), "{\"m\":\"screen\",\"session_id\":305419896,\"w\":854,\"h\":480,\"q\":70}")
        rx.rawMode = true
        rx.sendLine("{\"m\":\"screen\",\"ok\":true}")
        rx.send(Data(ScreenFramesTests.frame(ScreenFrames.typeKeepalive, []) + ScreenFramesTests.frame(ScreenFrames.typeStatus, Array("Esperando…".utf8)) + ScreenFramesTests.frame(ScreenFrames.typeImage, jpeg)))
        XCTAssertTrue(wait(for: c) { $0.image != nil })
        XCTAssertEqual(c.image?.seq, 1)
        // Ni el keepalive ni el estado se confirman, y la imagen aún no está pintada
        XCTAssertEqual(rx.pendingBytes(), 0)
        c.shown(1)
        XCTAssertEqual(rx.nextByte(), 1)
        c.shown(1)
        XCTAssertEqual(rx.pendingBytes(), 0, "repetir el aviso no repite el byte")
        rx.send(Data(ScreenFramesTests.frame(ScreenFrames.typeImage, jpeg)))
        XCTAssertTrue(wait(for: c) { $0.image?.seq == 2 })
        c.shown(2)
        XCTAssertEqual(rx.nextByte(), 1)
        c.close()
        XCTAssertTrue(wait(for: c) { $0.image == nil && $0.fps == 0 })
    }

    func testSiNadiePintaLaImagenSeConfirmaIgualmente() throws {
        let rx = try FakeReceiver()
        defer { rx.stop() }
        let c = client(rx, fallback: 100)
        c.start()
        _ = rx.nextLine()
        rx.rawMode = true
        rx.sendLine("{\"m\":\"screen\",\"ok\":true}")
        rx.send(Data(ScreenFramesTests.frame(ScreenFrames.typeImage, jpeg)))
        XCTAssertEqual(rx.nextByte(), 1)
        c.close()
    }

    func testImagenQueNoDecodificaSeConfirmaSinPublicar() throws {
        let rx = try FakeReceiver()
        defer { rx.stop() }
        let c = client(rx)
        c.start()
        _ = rx.nextLine()
        rx.rawMode = true
        rx.sendLine("{\"m\":\"screen\",\"ok\":true}")
        rx.send(Data(ScreenFramesTests.frame(ScreenFrames.typeImage, [UInt8](repeating: 0, count: 5))))
        XCTAssertEqual(rx.nextByte(), 1)
        XCTAssertNil(c.image)
        c.close()
    }

    func testElEstadoQuitaLaImagen() throws {
        let rx = try FakeReceiver()
        defer { rx.stop() }
        let c = client(rx)
        c.start()
        _ = rx.nextLine()
        rx.rawMode = true
        rx.sendLine("{\"m\":\"screen\",\"ok\":true}")
        rx.send(Data(ScreenFramesTests.frame(ScreenFrames.typeImage, jpeg)))
        XCTAssertTrue(wait(for: c) { $0.image != nil })
        c.shown(1)
        XCTAssertEqual(rx.nextByte(), 1)
        rx.send(Data(ScreenFramesTests.frame(ScreenFrames.typeStatus, Array("Cemu no está abierto".utf8))))
        XCTAssertTrue(wait(for: c) { $0.status == "Cemu no está abierto" })
        XCTAssertNil(c.image)
        c.close()
    }

    func testReceptorMudoEsSinPantallaYReintenta() throws {
        let rx = try FakeReceiver()
        defer { rx.stop() }
        let c = client(rx, reply: 200, retryUnavailable: 200)
        c.start()
        _ = rx.nextLine() // y no contesta
        XCTAssertTrue(wait(for: c) { $0.status == ScreenClient.statusUnavailable })
        XCTAssertTrue(rx.nextLine()?.hasPrefix("{\"m\":\"screen\"") == true, "reintento con la misma línea")
        c.close()
    }

    func testErrDelReceptorEsSinPantallaConElMotivo() throws {
        let rx = try FakeReceiver()
        defer { rx.stop() }
        let c = client(rx)
        c.start()
        _ = rx.nextLine()
        rx.sendLine("{\"m\":\"err\",\"code\":\"bad_session\",\"msg\":\"sesión desconocida\"}")
        XCTAssertTrue(wait(for: c) { $0.status == ScreenClient.statusDetail + "sesión desconocida" })
        c.close()
    }

    func testCaidaTrasOkReconecta() throws {
        let rx = try FakeReceiver()
        defer { rx.stop() }
        let c = client(rx, retryDrop: 200)
        c.start()
        _ = rx.nextLine()
        rx.rawMode = true
        rx.sendLine("{\"m\":\"screen\",\"ok\":true}")
        rx.send(Data(ScreenFramesTests.frame(ScreenFrames.typeStatus, Array("hola".utf8))))
        XCTAssertTrue(wait(for: c) { $0.status == "hola" })
        rx.rawMode = false
        rx.closeConnection() // el receptor se cae
        XCTAssertTrue(wait(for: c) { $0.status == ScreenClient.statusReconnecting })
        XCTAssertTrue(rx.nextLine()?.contains("\"session_id\":305419896") == true)
        rx.rawMode = true
        rx.sendLine("{\"m\":\"screen\",\"ok\":true}")
        rx.send(Data(ScreenFramesTests.frame(ScreenFrames.typeImage, jpeg)))
        XCTAssertTrue(wait(for: c) { $0.image != nil })
        c.shown(c.image!.seq)
        XCTAssertEqual(rx.nextByte(), 1)
        c.close()
    }

    func testElKeepaliveMantieneVivaLaConexionYElSilencioLaMata() throws {
        let rx = try FakeReceiver()
        defer { rx.stop() }
        let c = client(rx, data: 300, retryDrop: 100)
        c.start()
        _ = rx.nextLine()
        rx.rawMode = true
        rx.sendLine("{\"m\":\"screen\",\"ok\":true}")
        for _ in 0..<4 {
            Thread.sleep(forTimeInterval: 0.15)
            rx.send(Data(ScreenFramesTests.frame(ScreenFrames.typeKeepalive, [])))
        }
        RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        XCTAssertNil(c.status)
        XCTAssertEqual(rx.pendingBytes(wait: 0.1), 0, "el keepalive no se confirma")
        rx.rawMode = false
        XCTAssertTrue(wait(for: c, timeout: 3) { $0.status == ScreenClient.statusReconnecting })
        XCTAssertTrue(rx.nextLine()?.hasPrefix("{\"m\":\"screen\"") == true)
        c.close()
    }
}
