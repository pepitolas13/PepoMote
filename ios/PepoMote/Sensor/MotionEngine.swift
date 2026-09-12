import CoreMotion
import Foundation
import UIKit

/// Qué emula este móvil: decide qué lleva cada paquete INPUT.
enum SenderKind {
    /// Wiimote (puntero / Dolphin): 72 bytes, sin stick.
    case wiimote
    /// Nunchuk: 72 bytes, stick en los bytes 6-7 (flagStickValid).
    case nunchuk
    /// GamePad / Pro Controller de Wii U (Cemu): 80 bytes con el bloque de
    /// extensión y los sensores remapeados al marco del mando apaisado.
    case gamepad
}

/// Sensores por CoreMotion (deviceMotion, marco `xArbitraryZVertical`: Z
/// vertical, rumbo arbitrario, como el GAME_ROTATION_VECTOR de Android). Un
/// paquete INPUT por muestra, con tope de 250 Hz y media ponderada del gyro
/// entre envíos. Los ejes del dispositivo coinciden con los de Android (X a
/// la derecha, Y hacia arriba del móvil, Z saliendo de la pantalla); el
/// acelerómetro se convierte a la convención de Android (en reposo boca
/// arriba, +9,81 en Z): −(gravedad + aceleración del usuario) · g.
final class MotionEngine {
    private let lock = NSLock()
    private var kindV: SenderKind
    private var rotationV: Int = Frame.rotation0

    /// Qué se emula. El servicio lo fija al conectar; la pantalla GamePad lo
    /// pone en `.gamepad` al entrar y lo restaura al salir.
    var kind: SenderKind {
        get { lock.lock(); defer { lock.unlock() }; return kindV }
        set { lock.lock(); kindV = newValue; lock.unlock() }
    }

    /// Rotación de la pantalla (valores de `Frame`) mientras se es GamePad.
    var rotation: Int {
        get { lock.lock(); defer { lock.unlock() }; return rotationV }
        set { lock.lock(); rotationV = newValue; lock.unlock() }
    }

    private let sessionId: UInt32
    private let onPacket: (Data) -> Void
    private let manager = CMMotionManager()
    private let sync = DispatchQueue(label: "pepomote.sensors", qos: .userInteractive)
    private let opQueue: OperationQueue = {
        let q = OperationQueue()
        q.maxConcurrentOperationCount = 1
        q.qualityOfService = .userInteractive
        return q
    }()

    private var quat: [Float] = [1, 0, 0, 0]
    private var gyro: [Float] = [0, 0, 0]
    private var accel: [Float] = [0, 0, 0]
    private var seq: UInt32 = 0
    private var hasRotationVector = false
    private var lastSendNs: Int64 = 0
    private var lastSampleNs: Int64 = 0
    /// Media ponderada por tiempo del gyro entre envíos (∑ω·Δt y ∑Δt).
    private var gyroSum: [Float] = [0, 0, 0]
    private var gyroSumNs: Int64 = 0
    private var lastGyroNs: Int64 = 0
    private var batteryPct = 100
    private var batteryReadAtNs: Int64 = 0
    private var hzWindowStartNs: Int64 = 0
    private var hzCount = 0
    private var keepalive: DispatchSourceTimer?
    private var lastSensorHzV: Float = 0

    /// Frecuencia real del sensor (media de 1 s).
    var lastSensorHz: Float {
        lock.lock(); defer { lock.unlock() }
        return lastSensorHzV
    }

    init(sessionId: UInt32, kind: SenderKind = .wiimote, onPacket: @escaping (Data) -> Void) {
        self.sessionId = sessionId
        kindV = kind
        self.onPacket = onPacket
        opQueue.underlyingQueue = sync
    }

    func start() {
        DispatchQueue.main.async { UIDevice.current.isBatteryMonitoringEnabled = true }
        if manager.isDeviceMotionAvailable {
            hasRotationVector = true
            manager.deviceMotionUpdateInterval = 1.0 / 250.0
            manager.startDeviceMotionUpdates(using: .xArbitraryZVertical, to: opQueue) { [weak self] motion, _ in
                guard let self, let motion else { return }
                self.onMotion(motion)
            }
        } else if manager.isAccelerometerAvailable {
            // Sin giroscopio: solo acelerómetro y sin quaternion (el receptor lo sabe por los flags)
            manager.accelerometerUpdateInterval = 1.0 / 100.0
            manager.startAccelerometerUpdates(to: opQueue) { [weak self] data, _ in
                guard let self, let data else { return }
                let a = data.acceleration
                self.accel = [Float(-a.x * 9.80665), Float(-a.y * 9.80665), Float(-a.z * 9.80665)]
                self.onSample(tNs: Int64(data.timestamp * 1e9))
            }
        }
        // Keepalive 1 Hz aunque el sensor calle (PROTOCOL.md §4.1)
        let t = DispatchSource.makeTimerSource(queue: sync)
        t.schedule(deadline: .now() + 1, repeating: 1)
        t.setEventHandler { [weak self] in
            guard let self else { return }
            let now = Int64(DispatchTime.now().uptimeNanoseconds)
            if now - self.lastSendNs >= 1_000_000_000 {
                self.sendPacket(tSensorNs: self.lastSampleNs > 0 ? self.lastSampleNs : now)
                self.lastSendNs = now
            }
        }
        t.resume()
        keepalive = t
    }

    func stop() {
        manager.stopDeviceMotionUpdates()
        manager.stopAccelerometerUpdates()
        keepalive?.cancel()
        keepalive = nil
    }

    private func onMotion(_ m: CMDeviceMotion) {
        let tNs = Int64(m.timestamp * 1e9)
        let q = m.attitude.quaternion
        quat = [Float(q.w), Float(q.x), Float(q.y), Float(q.z)]
        let g = m.gravity
        let u = m.userAcceleration
        accel = [Float(-(g.x + u.x) * 9.80665), Float(-(g.y + u.y) * 9.80665), Float(-(g.z + u.z) * 9.80665)]
        let r = m.rotationRate
        let sample: [Float] = [Float(r.x), Float(r.y), Float(r.z)]
        trackHz(tNs)
        // Media ponderada por tiempo de las muestras entre envíos; con un hueco raro se reinicia
        let gap = tNs - lastGyroNs
        if lastGyroNs != 0, gap >= 1, gap <= 50_000_000 {
            for i in 0..<3 { gyroSum[i] += sample[i] * Float(gap) }
            gyroSumNs += gap
        } else {
            gyroSum = [0, 0, 0]
            gyroSumNs = 0
        }
        lastGyroNs = tNs
        // Tope de 250 Hz del protocolo
        if tNs - lastSendNs >= 3_900_000 {
            if gyroSumNs > 0 {
                for i in 0..<3 { gyro[i] = gyroSum[i] / Float(gyroSumNs) }
            } else {
                gyro = sample
            }
            gyroSum = [0, 0, 0]
            gyroSumNs = 0
            onSample(tNs: tNs)
        }
    }

    private func onSample(tNs: Int64) {
        lastSampleNs = tNs
        lastSendNs = Int64(DispatchTime.now().uptimeNanoseconds)
        sendPacket(tSensorNs: tNs)
    }

    private func sendPacket(tSensorNs: Int64) {
        seq &+= 1
        let quatFlag: UInt8 = hasRotationVector ? PmpCodec.flagQuatValid : 0
        let bs = ButtonState.shared
        let tUs = UInt64(max(0, tSensorNs / 1000))
        let packet: Data
        switch kind {
        case .gamepad:
            let rot = rotation
            let touch = bs.touch()
            packet = PmpCodec.encodeInput(
                sessionId: sessionId, seq: seq, tSensorUs: tUs,
                quat: Frame.remapQuat(quat, rot), gyro: Frame.remapGyro(gyro, rot), accel: Frame.remapAccel(accel, rot),
                buttons: bs.current(), recenterCount: bs.recenterCount(), batteryPct: battery(),
                touchScrollDy: bs.drainScroll(),
                flags: quatFlag | PmpCodec.flagStickValid | PmpCodec.flagExt | (touch.down ? PmpCodec.flagTouch : 0),
                stickX: bs.stickX(), stickY: bs.stickY(), stick2X: bs.stickRX(), stick2Y: bs.stickRY(),
                touchX: touch.x, touchY: touch.y
            )
        case .nunchuk:
            packet = PmpCodec.encodeInput(
                sessionId: sessionId, seq: seq, tSensorUs: tUs,
                quat: quat, gyro: gyro, accel: accel,
                buttons: bs.current(), recenterCount: bs.recenterCount(), batteryPct: battery(),
                touchScrollDy: bs.drainScroll(),
                flags: quatFlag | PmpCodec.flagStickValid,
                stickX: bs.stickX(), stickY: bs.stickY()
            )
        case .wiimote:
            packet = PmpCodec.encodeInput(
                sessionId: sessionId, seq: seq, tSensorUs: tUs,
                quat: quat, gyro: gyro, accel: accel,
                buttons: bs.current(), recenterCount: bs.recenterCount(), batteryPct: battery(),
                touchScrollDy: bs.drainScroll(),
                flags: quatFlag
            )
        }
        onPacket(packet)
    }

    private func trackHz(_ tNs: Int64) {
        if hzWindowStartNs == 0 { hzWindowStartNs = tNs }
        hzCount += 1
        let elapsed = tNs - hzWindowStartNs
        if elapsed > 1_000_000_000 {
            let hz = Float(hzCount) * 1e9 / Float(elapsed)
            lock.lock(); lastSensorHzV = hz; lock.unlock()
            hzWindowStartNs = tNs
            hzCount = 0
        }
    }

    private func battery() -> Int {
        let now = Int64(DispatchTime.now().uptimeNanoseconds)
        if now - batteryReadAtNs > 5_000_000_000 {
            batteryReadAtNs = now
            let level = UIDevice.current.batteryLevel // −1 si no se sabe (simulador)
            if level >= 0 { batteryPct = Int((level * 100).rounded()) }
        }
        return batteryPct
    }
}
