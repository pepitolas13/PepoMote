import Foundation

/// Bits de botones (PROTOCOL.md §4.2). Los mismos que Android y el receptor.
enum Btn {
    static let a: UInt32 = 1 << 0
    static let b: UInt32 = 1 << 1
    static let dpadUp: UInt32 = 1 << 2
    static let dpadDown: UInt32 = 1 << 3
    static let dpadLeft: UInt32 = 1 << 4
    static let dpadRight: UInt32 = 1 << 5
    static let plus: UInt32 = 1 << 6
    static let minus: UInt32 = 1 << 7
    static let home: UInt32 = 1 << 8
    static let one: UInt32 = 1 << 9
    static let two: UInt32 = 1 << 10
    static let mediaVolUp: UInt32 = 1 << 11
    static let mediaVolDown: UInt32 = 1 << 12
    static let mediaMute: UInt32 = 1 << 13
    static let mediaPlayPause: UInt32 = 1 << 14
    static let mediaNext: UInt32 = 1 << 15
    static let mediaPrev: UInt32 = 1 << 16
    /// Nunchuk (móvil de la otra mano).
    static let c: UInt32 = 1 << 17
    static let z: UInt32 = 1 << 18
    /// Wii U GamePad / Pro Controller (modo Cemu).
    static let x: UInt32 = 1 << 19
    static let y: UInt32 = 1 << 20
    static let l: UInt32 = 1 << 21
    static let r: UInt32 = 1 << 22
    static let zl: UInt32 = 1 << 23
    static let zr: UInt32 = 1 << 24
    /// Click del stick izquierdo / derecho (L3 / R3).
    static let stickL: UInt32 = 1 << 25
    static let stickR: UInt32 = 1 << 26
    /// Soplar al micrófono del GamePad.
    static let mic: UInt32 = 1 << 27
    /// Cambiar la vista TV ↔ pantalla del GamePad (función de Cemu).
    static let screen: UInt32 = 1 << 28
    /// Precisión (modo puntero): mientras se mantiene, el cursor va al 40 %.
    static let precision: UInt32 = 1 << 29
}

/// Reloj + temporizador del latch (cola principal en la app; virtual en tests).
protocol LatchScheduler {
    /// Milisegundos monótonos.
    func now() -> Int64
    /// Ejecuta `block` pasados `afterMs` ms, salvo que la tarea se cancele antes.
    func schedule(afterMs: Int64, _ block: @escaping () -> Void) -> LatchTask
}

/// Tarea programada cancelable.
final class LatchTask {
    private(set) var cancelled = false
    func cancel() { cancelled = true }
}

/// Temporizadores en la cola principal (la UI escribe los botones desde ahí).
final class MainQueueScheduler: LatchScheduler {
    func now() -> Int64 {
        Int64(DispatchTime.now().uptimeNanoseconds / 1_000_000)
    }

    func schedule(afterMs: Int64, _ block: @escaping () -> Void) -> LatchTask {
        let task = LatchTask()
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(Int(max(0, afterMs)))) {
            if !task.cancelled { block() }
        }
        return task
    }
}

/// Estado de botones compartido entre la UI (escribe) y el hilo de sensores
/// (lee). Las pulsaciones pasan por `PressLatch`: un toque, por corto que sea,
/// dura lo suficiente en el cable para llegar.
final class ButtonState {
    static let shared = ButtonState()

    /// Dedo en la pantalla táctil del GamePad: instantánea inmutable (x, y en
    /// 0..65535, origen arriba-izquierda).
    struct Touch {
        let x: Int
        let y: Int
        let down: Bool
    }

    private let lock = NSLock()
    private var mask: UInt32 = 0
    private var recenter: Int = 0
    private var scrollAcc: Int = 0
    private var stickV = (x: 0, y: 0)
    private var stick2V = (x: 0, y: 0)
    private var touchV = Touch(x: 0, y: 0, down: false)
    private var latch: PressLatch!

    init(scheduler: LatchScheduler = MainQueueScheduler()) {
        latch = PressLatch(scheduler: scheduler) { [weak self] bit, down in
            self?.wire(bit, down)
        }
    }

    private func wire(_ bit: UInt32, _ down: Bool) {
        lock.lock()
        if down { mask |= bit } else { mask &= ~bit }
        lock.unlock()
    }

    /// Pulsación física/táctil: el flanco de bajada sale al instante.
    func set(_ bit: UInt32, _ down: Bool) {
        latch.set(bit, down)
    }

    func current() -> UInt32 {
        lock.lock(); defer { lock.unlock() }
        return mask
    }

    func bumpRecenter() {
        lock.lock(); recenter &+= 1; lock.unlock()
    }

    func recenterCount() -> Int {
        lock.lock(); defer { lock.unlock() }
        return recenter & 0xFF
    }

    func addScroll(_ dyPx: Int) {
        lock.lock(); scrollAcc += dyPx; lock.unlock()
    }

    /// Vacía el acumulador de scroll (lo llama el hilo de sensores por paquete).
    func drainScroll() -> Int {
        lock.lock(); defer { lock.unlock() }
        let v = scrollAcc
        scrollAcc = 0
        return min(max(v, -32768), 32767)
    }

    /// Stick del Nunchuk o izquierdo del GamePad, −127..127, +x derecha, +y arriba.
    func setStick(_ x: Int, _ y: Int) {
        lock.lock(); stickV = (Self.clamp127(x), Self.clamp127(y)); lock.unlock()
    }

    func stickX() -> Int { lock.lock(); defer { lock.unlock() }; return stickV.x }
    func stickY() -> Int { lock.lock(); defer { lock.unlock() }; return stickV.y }

    /// Stick derecho del GamePad (solo lo escribe la pantalla GamePad).
    func setStick2(_ x: Int, _ y: Int) {
        lock.lock(); stick2V = (Self.clamp127(x), Self.clamp127(y)); lock.unlock()
    }

    func stickRX() -> Int { lock.lock(); defer { lock.unlock() }; return stick2V.x }
    func stickRY() -> Int { lock.lock(); defer { lock.unlock() }; return stick2V.y }

    /// Pantalla táctil del GamePad: x, y en 0..65535 (se recortan) y `down` = hay dedo.
    func setTouch(_ x: Int, _ y: Int, _ down: Bool) {
        lock.lock()
        touchV = Touch(x: min(max(x, 0), 65535), y: min(max(y, 0), 65535), down: down)
        lock.unlock()
    }

    func touch() -> Touch {
        lock.lock(); defer { lock.unlock() }
        return touchV
    }

    /// Todo suelto: botones, scroll, ambos sticks y táctil.
    func reset() {
        latch.reset()
        lock.lock()
        mask = 0
        scrollAcc = 0
        stickV = (0, 0)
        stick2V = (0, 0)
        touchV = Touch(x: 0, y: 0, down: false)
        lock.unlock()
    }

    private static func clamp127(_ v: Int) -> Int { min(max(v, -127), 127) }
}
