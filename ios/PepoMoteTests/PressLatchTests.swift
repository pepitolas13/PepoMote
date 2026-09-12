import XCTest
@testable import PepoMote

/// Reloj virtual: los toques se simulan a tiempos exactos (mismos casos que Android).
final class PressLatchTests: XCTestCase {
    final class FakeScheduler: LatchScheduler {
        var t: Int64 = 0
        private var tasks: [(due: Int64, task: LatchTask, block: () -> Void)] = []

        func now() -> Int64 { t }

        func schedule(afterMs: Int64, _ block: @escaping () -> Void) -> LatchTask {
            let task = LatchTask()
            tasks.append((t + afterMs, task, block))
            return task
        }

        func advance(to end: Int64) {
            while true {
                let due = tasks.enumerated().filter { $0.element.due <= end }
                guard let next = due.min(by: { $0.element.due < $1.element.due }) else { break }
                tasks.remove(at: next.offset)
                t = next.element.due
                if !next.element.task.cancelled { next.element.block() }
            }
            t = end
        }
    }

    private var clock: FakeScheduler!
    private var wire: [(Int64, Bool)] = []
    private var latch: PressLatch!

    override func setUp() {
        super.setUp()
        clock = FakeScheduler()
        wire = []
        latch = PressLatch(scheduler: clock) { [unowned self] _, down in self.wire.append((self.clock.t, down)) }
    }

    private func down(_ at: Int64) {
        clock.advance(to: at)
        latch.set(1, true)
    }

    private func up(_ at: Int64) {
        clock.advance(to: at)
        latch.set(1, false)
    }

    private func assertWire(_ expected: [(Int64, Bool)], file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(wire.map { "\($0.0):\($0.1)" }, expected.map { "\($0.0):\($0.1)" }, file: file, line: line)
    }

    func testElFlancoDeBajadaSaleAlInstante() {
        down(0)
        assertWire([(0, true)])
    }

    func testUnToqueCortoDuraElMinimoEnElCable() {
        down(0)
        up(20)
        clock.advance(to: 500)
        assertWire([(0, true), (70, false)])
    }

    func testMantenerPulsadoSueltaCuandoSeSuelta() {
        down(0)
        up(300)
        clock.advance(to: 500)
        assertWire([(0, true), (300, false)])
    }

    func testDosToquesRapidosNoSeFunden() {
        down(0)
        up(20)
        down(40) // el primero aún retenido: se suelta ya y se re-pulsa a los 10 ms
        up(60)
        clock.advance(to: 500)
        assertWire([(0, true), (40, false), (50, true), (120, false)])
    }

    func testSoltarAntesDeQueSalgaLaRepulsacionTambienDuraElMinimo() {
        down(0)
        up(20)
        down(40)
        up(45) // el segundo down sigue en cola (sale a los 50)
        clock.advance(to: 500)
        assertWire([(0, true), (40, false), (50, true), (120, false)])
    }

    func testResetCancelaLoRetenido() {
        down(0)
        up(20)
        latch.reset()
        clock.advance(to: 500)
        assertWire([(0, true)]) // nadie suelta: lo hace ButtonState.reset()
    }

    func testButtonStateConElLatch() {
        let bs = ButtonState(scheduler: clock)
        bs.set(Btn.a, true)
        XCTAssertEqual(bs.current(), Btn.a)
        clock.advance(to: 20)
        bs.set(Btn.a, false)
        XCTAssertEqual(bs.current(), Btn.a, "retenido hasta los 70 ms")
        clock.advance(to: 100)
        XCTAssertEqual(bs.current(), 0)
        bs.addScroll(30)
        bs.addScroll(-10)
        XCTAssertEqual(bs.drainScroll(), 20)
        XCTAssertEqual(bs.drainScroll(), 0)
        bs.setStick(300, -5)
        XCTAssertEqual(bs.stickX(), 127)
        XCTAssertEqual(bs.stickY(), -5)
        bs.setTouch(70000, -3, true)
        XCTAssertEqual(bs.touch().x, 65535)
        XCTAssertEqual(bs.touch().y, 0)
        XCTAssertTrue(bs.touch().down)
        bs.bumpRecenter()
        XCTAssertEqual(bs.recenterCount(), 1)
        bs.set(Btn.b, true)
        bs.reset()
        XCTAssertEqual(bs.current(), 0)
        XCTAssertEqual(bs.stickX(), 0)
        XCTAssertFalse(bs.touch().down)
    }
}
