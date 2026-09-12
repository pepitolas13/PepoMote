import Foundation

/// Garantiza que cada pulsación dure al menos `minPressMs` EN EL CABLE.
///
/// Un toque dura 20-40 ms: 5-10 paquetes a 250 Hz. Una ráfaga de pérdida
/// Wi-Fi de ese tamaño se lo traga entero, y el juego (60 Hz, a veces con
/// antirrebote) también puede no verlo. Con 70 ms van ~18 paquetes y el toque
/// llega siempre. El flanco de BAJADA (el que marca la latencia) sale al
/// instante; solo se retrasa el de subida, y solo en toques más cortos que el
/// mínimo. Mantener pulsado no cambia nada.
///
/// Dos toques muy seguidos no se funden: si el segundo llega con el primero
/// aún retenido, se suelta al momento y se re-pulsa tras `minGapMs`, para que
/// el receptor vea los dos flancos. Mismo comportamiento (y mismos tests) que
/// PressLatch.kt en Android.
final class PressLatch {
    private final class Bit {
        var downAt: Int64 = 0
        var release: LatchTask?
        var press: LatchTask?
        var releaseWhenPressed = false
    }

    private let scheduler: LatchScheduler
    private let minPressMs: Int64
    private let minGapMs: Int64
    private let onWire: (UInt32, Bool) -> Void
    private var bits: [UInt32: Bit] = [:]
    private let lock = NSRecursiveLock()

    init(
        scheduler: LatchScheduler,
        minPressMs: Int64 = 70,
        minGapMs: Int64 = 10,
        onWire: @escaping (UInt32, Bool) -> Void
    ) {
        self.scheduler = scheduler
        self.minPressMs = minPressMs
        self.minGapMs = minGapMs
        self.onWire = onWire
    }

    func set(_ bit: UInt32, _ down: Bool) {
        lock.lock(); defer { lock.unlock() }
        let b: Bit
        if let existing = bits[bit] {
            b = existing
        } else {
            b = Bit()
            bits[bit] = b
        }
        if down { physicalDown(bit, b) } else { physicalUp(bit, b) }
    }

    private func physicalDown(_ bit: UInt32, _ b: Bit) {
        b.releaseWhenPressed = false
        if b.press != nil { return } // re-pulsación ya en cola: se mantendrá
        guard let held = b.release else {
            press(bit, b)
            return
        }
        // El toque anterior sigue retenido: soltar YA y re-pulsar tras un hueco
        held.cancel()
        b.release = nil
        onWire(bit, false)
        var task: LatchTask?
        task = scheduler.schedule(afterMs: minGapMs) { [weak self] in
            guard let self, let mine = task else { return }
            self.lock.lock(); defer { self.lock.unlock() }
            if b.press === mine {
                b.press = nil
                self.press(bit, b)
            }
        }
        b.press = task
    }

    private func physicalUp(_ bit: UInt32, _ b: Bit) {
        if b.press != nil {
            // El down aún está en cola: se soltará al ejecutarse
            b.releaseWhenPressed = true
            return
        }
        let remaining = b.downAt + minPressMs - scheduler.now()
        if remaining <= 0 {
            b.release?.cancel()
            b.release = nil
            onWire(bit, false)
        } else {
            scheduleRelease(bit, b, remaining)
        }
    }

    private func press(_ bit: UInt32, _ b: Bit) {
        b.downAt = scheduler.now()
        onWire(bit, true)
        if b.releaseWhenPressed {
            b.releaseWhenPressed = false
            scheduleRelease(bit, b, minPressMs)
        }
    }

    private func scheduleRelease(_ bit: UInt32, _ b: Bit, _ delayMs: Int64) {
        b.release?.cancel()
        var task: LatchTask?
        task = scheduler.schedule(afterMs: delayMs) { [weak self] in
            guard let self, let mine = task else { return }
            self.lock.lock(); defer { self.lock.unlock() }
            if b.release === mine {
                b.release = nil
                self.onWire(bit, false)
            }
        }
        b.release = task
    }

    /// Cancela todo lo pendiente (nueva conexión): nada queda retenido.
    func reset() {
        lock.lock(); defer { lock.unlock() }
        for b in bits.values {
            b.release?.cancel()
            b.press?.cancel()
        }
        bits.removeAll()
    }
}
