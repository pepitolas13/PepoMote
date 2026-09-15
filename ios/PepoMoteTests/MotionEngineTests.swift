import XCTest
@testable import PepoMote

private final class ManualMotionSource: MotionSource {
    var handlers: [(MotionReading) -> Void] = []
    var starts = 0
    var stops = 0

    func start(to queue: OperationQueue, onReading: @escaping (MotionReading) -> Void) {
        starts += 1
        handlers.append(onReading)
    }

    func stop() { stops += 1 }

    func emit(gyro: [Float], accel: [Float], quaternion: [Float]? = nil, generation: Int? = nil) {
        handlers[generation ?? handlers.count - 1](MotionReading(
            timestampNs: Int64(DispatchTime.now().uptimeNanoseconds),
            quaternion: quaternion, gyro: gyro, accel: accel
        ))
    }
}

private final class PacketRecorder {
    private let lock = NSLock()
    private var stored: [Data] = []
    func append(_ packet: Data) { lock.lock(); stored.append(packet); lock.unlock() }
    var packets: [Data] { lock.lock(); defer { lock.unlock() }; return stored }
}

final class MotionEngineTests: XCTestCase {
    private func float(_ packet: Data, _ offset: Int) -> Float {
        let bits = packet.subdata(in: offset..<offset + 4).enumerated().reduce(UInt32(0)) { $0 | UInt32($1.element) << ($1.offset * 8) }
        return Float(bitPattern: bits)
    }

    private func runFor(_ seconds: Double) {
        let elapsed = expectation(description: "packet interval")
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { elapsed.fulfill() }
        wait(for: [elapsed], timeout: seconds + 2)
    }

    func testNoSensorCallbacksStillSendButtonsSticksAndReleases() throws {
        let source = ManualMotionSource()
        let recorder = PacketRecorder()
        let engine = MotionEngine(sessionId: 4, kind: .switchPad, source: source, onPacket: recorder.append)
        let buttons = ButtonState.shared
        buttons.reset()
        defer { engine.stop(); buttons.reset() }
        engine.start()
        runFor(0.06)
        XCTAssertGreaterThanOrEqual(recorder.packets.count, 4)
        buttons.set(Btn.a, true)
        buttons.setStick(100, -40)
        buttons.setStick2(-25, 90)
        runFor(0.03)
        let down = try XCTUnwrap(recorder.packets.last)
        XCTAssertEqual(down.count, 80)
        XCTAssertNotEqual(down[64] & UInt8(Btn.a), 0)
        XCTAssertEqual(Int8(bitPattern: down[6]), 100)
        XCTAssertEqual(Int8(bitPattern: down[73]), 90)
        XCTAssertEqual(down[5] & PmpCodec.flagQuatValid, 0)
        for offset in stride(from: 40, through: 60, by: 4) { XCTAssertEqual(float(down, offset), 0) }
        buttons.set(Btn.a, false)
        runFor(0.1)
        XCTAssertEqual(try XCTUnwrap(recorder.packets.last)[64] & UInt8(Btn.a), 0)
    }

    func testAvailableAccelerometerSendsRealValuesWithoutAngularVelocity() throws {
        let source = ManualMotionSource()
        let recorder = PacketRecorder()
        let engine = MotionEngine(sessionId: 4, source: source, onPacket: recorder.append)
        defer { engine.stop() }
        engine.start()
        source.emit(gyro: [0, 0, 0], accel: [1, 2, 9.8])
        runFor(0.025)
        let packet = try XCTUnwrap(recorder.packets.last)
        XCTAssertEqual(packet[5] & PmpCodec.flagQuatValid, 0)
        XCTAssertEqual(float(packet, 40), 0)
        XCTAssertEqual(float(packet, 52), 1)
        XCTAssertEqual(float(packet, 56), 2)
        XCTAssertEqual(float(packet, 60), 9.8, accuracy: 0.001)
    }

    func testStalledReadingsClearGyroAndKeepLastOrientation() throws {
        let source = ManualMotionSource()
        let recorder = PacketRecorder()
        let engine = MotionEngine(sessionId: 4, source: source, onPacket: recorder.append)
        defer { engine.stop() }
        engine.start()
        source.emit(gyro: [1, 2, 3], accel: [0, 0, 9.8], quaternion: [0.5, 0.5, 0.5, 0.5])
        runFor(0.1)
        let packet = try XCTUnwrap(recorder.packets.last)
        for offset in stride(from: 40, through: 60, by: 4) { XCTAssertEqual(float(packet, offset), 0) }
        for offset in stride(from: 24, through: 36, by: 4) { XCTAssertEqual(float(packet, offset), 0.5) }
        XCTAssertEqual(engine.lastSensorHz, 0)
    }

    func testStopAndRestartDiscardOldCallbacksAndDoNotDuplicateTimer() throws {
        let source = ManualMotionSource()
        let recorder = PacketRecorder()
        let engine = MotionEngine(sessionId: 4, source: source, onPacket: recorder.append)
        defer { engine.stop() }
        engine.start()
        engine.start()
        XCTAssertEqual(source.starts, 1)
        runFor(0.03)
        engine.stop()
        let stoppedCount = recorder.packets.count
        source.emit(gyro: [1, 2, 3], accel: [0, 0, 9.8], quaternion: [0.5, 0.5, 0.5, 0.5])
        runFor(0.04)
        XCTAssertEqual(recorder.packets.count, stoppedCount)
        engine.start()
        source.emit(gyro: [8, 8, 8], accel: [9, 9, 9], quaternion: [0.5, 0.5, 0.5, 0.5], generation: 0)
        runFor(0.04)
        let packet = try XCTUnwrap(recorder.packets.last)
        XCTAssertGreaterThan(recorder.packets.count, stoppedCount)
        XCTAssertEqual(packet[5] & PmpCodec.flagQuatValid, 0)
        XCTAssertEqual(float(packet, 24), 1)
        XCTAssertEqual(float(packet, 40), 0)
        XCTAssertEqual(source.starts, 2)
    }
}
