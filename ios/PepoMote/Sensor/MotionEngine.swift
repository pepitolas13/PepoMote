import Foundation
import UIKit

/// Qué emula este móvil: decide qué lleva cada paquete INPUT.
enum SenderKind {
    /// Wiimote (puntero / Dolphin): 72 bytes, sin stick.
    case wiimote
    /// Nunchuk: 72 bytes, stick en los bytes 6-7 (flagStickValid).
    case nunchuk
    /// Mando de Wii con Nunchuk en el mismo móvil, apaisado: 72 bytes con el
    /// stick (flagStickValid) y los botones del mando y del Nunchuk (C/Z) a la
    /// vez, y los sensores remapeados al marco apaisado como el GamePad (el
    /// puntero de Dolphin sigue apuntando con el móvil de lado).
    case wiiNunchuk
    /// GamePad / Pro Controller de Wii U (Cemu): 80 bytes con el bloque de
    /// extensión y los sensores remapeados al marco del mando apaisado.
    case gamepad
    /// Switch usa la misma extensión, siempre sin pantalla táctil.
    case switchPad
}

/// Sensores por CoreMotion (deviceMotion, marco `xArbitraryZVertical`: Z
/// vertical, rumbo arbitrario, como el GAME_ROTATION_VECTOR de Android). Un
/// paquete INPUT por muestra, con tope de 250 Hz y un envío de botones a
/// ~100 Hz si las muestras fallan. Se conserva la media ponderada del gyro
/// entre envíos. Los ejes del dispositivo coinciden con los de Android (X a
/// la derecha, Y hacia arriba del móvil, Z saliendo de la pantalla); el
/// acelerómetro se convierte a la convención de Android (en reposo boca
/// arriba, +9,81 en Z): −(gravedad + aceleración del usuario) · g.
final class MotionEngine {
    private let lock = NSLock()
    private var kindV: SenderKind
    private var rotationV: Int = Frame.rotation0
    // Set once before start(), from the receiver's negotiated capability.
    var reportFrameRotation = false
    private var holdRotation = Frame.rotation0

    /// Qué se emula. El servicio lo fija al conectar; la pantalla GamePad lo
    /// pone en `.gamepad` al entrar y lo restaura al salir.
    var kind: SenderKind {
        get { lock.lock(); defer { lock.unlock() }; return kindV }
        set { lock.lock(); kindV = newValue; lock.unlock() }
    }

    /// Rotación de la pantalla (valores de `Frame`): la fija el GamePad, el
    /// mando + Nunchuk y el mando de lado (NES, con `Route.sidewaysRotation`);
    /// en vertical vale `Frame.rotation0` y no cambia nada.
    var rotation: Int {
        get { lock.lock(); defer { lock.unlock() }; return rotationV }
        set { lock.lock(); rotationV = newValue; lock.unlock() }
    }

    /// Teclado abierto en modo puntero: la pose se congela (ver `holdPose`).
    /// Lo pone y lo quita la pantalla del mando.
    var pointerHold: Bool {
        get { lock.lock(); defer { lock.unlock() }; return pointerHoldV }
        set { lock.lock(); pointerHoldV = newValue; lock.unlock() }
    }

    /// Mando universal: mover el móvil mueve el stick derecho. Apagado, el
    /// paquete sale con el giroscopio a cero y el juego no se entera de que
    /// el móvil se mueve (el cuaternión y el acelerómetro viajan igual: en
    /// ese modo el receptor ni los mira, así que no hay que tocar el
    /// protocolo ni el PC). Lo apaga SOLO la pantalla del mando universal con
    /// el modo confirmado, y lo devuelve al salir: Switch y RetroArch
    /// comparten `.switchPad` y sí apuntan con el giro.
    var padAim: Bool {
        get { lock.lock(); defer { lock.unlock() }; return padAimV }
        set { lock.lock(); padAimV = newValue; lock.unlock() }
    }

    private var padAimV = true
    private var pointerHoldV = false
    private var holding = false
    private var releaseAtNs: Int64 = 0
    private var holdQuat: [Float] = [1, 0, 0, 0]
    private var holdAccel: [Float] = [0, 0, 0]

    private let sessionId: UInt32
    private let onPacket: (Data) -> Void
    private let source: MotionSource
    static let staleMotionNs: Int64 = 50_000_000
    /// Gracia tras cerrar el teclado antes de volver a los sensores reales.
    static let holdGraceNs: Int64 = 300_000_000
    private var running = false
    private var generation = 0
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
    private var lastSampleAtNs: Int64 = 0
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

    init(sessionId: UInt32, kind: SenderKind = .wiimote, source: MotionSource = CoreMotionSource(), onPacket: @escaping (Data) -> Void) {
        self.source = source
        self.sessionId = sessionId
        kindV = kind
        self.onPacket = onPacket
        opQueue.underlyingQueue = sync
    }

    func start() {
        DispatchQueue.main.async { UIDevice.current.isBatteryMonitoringEnabled = true }
        sync.sync {
            guard !running else { return }
            running = true
            generation += 1
            let gen = generation
            resetMotion()
            source.start(to: opQueue) { [weak self] reading in
                guard let self else { return }
                self.sync.async {
                    guard self.running, self.generation == gen else { return }
                    self.onReading(reading)
                }
            }
            let timer = DispatchSource.makeTimerSource(queue: sync)
            timer.schedule(deadline: .now(), repeating: .milliseconds(10), leeway: .milliseconds(1))
            timer.setEventHandler { [weak self] in
                guard let self, self.running, self.generation == gen else { return }
                let now = Int64(DispatchTime.now().uptimeNanoseconds)
                // Leave the 250 Hz sensor path alone; never depend on it for input.
                if now - self.lastSendNs < 9_000_000 { return }
                if self.lastSampleAtNs == 0 || now - self.lastSampleAtNs >= Self.staleMotionNs {
                    self.gyro = [0, 0, 0]
                    self.accel = [0, 0, 0]
                    self.gyroSum = [0, 0, 0]
                    self.gyroSumNs = 0
                    self.lastGyroNs = 0
                    self.lock.lock(); self.lastSensorHzV = 0; self.lock.unlock()
                }
                self.lastSendNs = now
                self.sendPacket(tSensorNs: now)
            }
            timer.resume()
            keepalive = timer
        }
    }

    func stop() {
        // This drains any active send before returning. Queued source callbacks
        // and cancelled-timer callbacks also check their session generation.
        sync.sync {
            guard running else { return }
            running = false
            generation += 1
            keepalive?.cancel()
            keepalive = nil
            source.stop()
            resetMotion()
        }
    }

    private func resetMotion() {
        quat = [1, 0, 0, 0]
        gyro = [0, 0, 0]
        accel = [0, 0, 0]
        hasRotationVector = false
        lastSendNs = 0
        lastSampleAtNs = 0
        lastGyroNs = 0
        gyroSum = [0, 0, 0]
        gyroSumNs = 0
        hzWindowStartNs = 0
        hzCount = 0
        lock.lock(); lastSensorHzV = 0; lock.unlock()
    }

    private func onReading(_ reading: MotionReading) {
        let now = Int64(DispatchTime.now().uptimeNanoseconds)
        let tNs = reading.timestampNs
        // A delayed sample is not fresh motion, even when it just reached our queue.
        guard tNs > 0, now - tNs < Self.staleMotionNs, tNs <= now + 1_000_000 else { return }
        lastSampleAtNs = tNs
        accel = reading.accel
        if let quaternion = reading.quaternion {
            quat = quaternion
            hasRotationVector = true
        }
        trackHz(tNs)
        let sample = reading.quaternion == nil ? [Float](repeating: 0, count: 3) : reading.gyro
        let gap = tNs - lastGyroNs
        if reading.quaternion != nil, lastGyroNs != 0, gap >= 1, gap <= Self.staleMotionNs {
            for i in 0..<3 { gyroSum[i] += sample[i] * Float(gap) }
            gyroSumNs += gap
        } else {
            gyroSum = [0, 0, 0]
            gyroSumNs = 0
        }
        lastGyroNs = reading.quaternion == nil ? 0 : tNs
        gyro = sample
        // Compare wall-clock send times; fallback packets use that same clock.
        guard now - lastSendNs >= 3_900_000 else { return }
        if gyroSumNs > 0 {
            for i in 0..<3 { gyro[i] = gyroSum[i] / Float(gyroSumNs) }
        }
        gyroSum = [0, 0, 0]
        gyroSumNs = 0
        lastSendNs = now
        sendPacket(tSensorNs: now)
    }

    /// Congela la pose mientras el teclado está abierto: el último cuaternión
    /// repetido y el giroscopio a cero.
    ///
    /// NO se deja de emitir. Los flancos de botón solo viajan montados en un
    /// paquete: callarse justo después del clic del botón «Teclado» podría
    /// dejar el botón izquierdo PULSADO en el PC (el receptor no tiene
    /// vigilante de cadencia). Y un hueco de más de 0,25 s hace que el motor
    /// del receptor resiembre su estado, con lo que el cursor daría un salto
    /// al volver.
    ///
    /// Emitiendo quieto pasa lo contrario: el motor del receptor lo ve
    /// parado, congela por su cuenta y deja libre el ratón de verdad, y al
    /// soltar el puntero sigue donde estaba. Funciona con cualquier receptor
    /// publicado, sin tocar el protocolo. Al soltar quedan `holdGraceNs` de
    /// gracia para que el PC teclee ANTES de que el cursor se mueva.
    private func holdPose(_ nowNs: Int64) {
        if pointerHold {
            releaseAtNs = nowNs + MotionEngine.holdGraceNs
        } else if !holding || nowNs >= releaseAtNs {
            holding = false
            return
        }
        if !holding {
            holding = true
            holdQuat = quat
            holdAccel = accel
        }
        quat = holdQuat
        accel = holdAccel
        gyro = [0, 0, 0]
    }

    private func sendPacket(tSensorNs: Int64) {
        let requestedRotation = rotation
        let wasHolding = holding
        holdPose(tSensorNs)
        if holding && !wasHolding { holdRotation = requestedRotation }
        let packetRotation = holding ? holdRotation : requestedRotation
        seq &+= 1
        let senderKind = kind
        let aim = padAim
        let frameFlag = reportFrameRotation && senderKind != .nunchuk ? UInt8((packetRotation & 3) << 5) : 0
        let quatFlag: UInt8 = (hasRotationVector ? PmpCodec.flagQuatValid : 0) | frameFlag
        let bs = ButtonState.shared
        let tUs = UInt64(max(0, tSensorNs / 1000))
        let packet: Data
        switch senderKind {
        case .gamepad, .switchPad:
            let rot = packetRotation
            let touch = senderKind == .gamepad ? bs.touch() : ButtonState.Touch(x: 0, y: 0, down: false)
            // Con el giro apagado, a cero solo en el paquete: la media
            // ponderada sigue viva y volver a encender no hereda ni un hueco
            // ni una muestra vieja
            let gyroOut: [Float] = aim ? Frame.remapGyro(gyro, rot) : [0, 0, 0]
            packet = PmpCodec.encodeInput(
                sessionId: sessionId, seq: seq, tSensorUs: tUs,
                quat: Frame.remapQuat(quat, rot), gyro: gyroOut, accel: Frame.remapAccel(accel, rot),
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
        case .wiiNunchuk:
            // Como el Nunchuk (stick en la trama) pero con el móvil de lado:
            // sensores al marco apaisado (contrato §4), igual que el GamePad
            let rot = packetRotation
            packet = PmpCodec.encodeInput(
                sessionId: sessionId, seq: seq, tSensorUs: tUs,
                quat: Frame.remapQuat(quat, rot), gyro: Frame.remapGyro(gyro, rot), accel: Frame.remapAccel(accel, rot),
                buttons: bs.current(), recenterCount: bs.recenterCount(), batteryPct: battery(),
                touchScrollDy: bs.drainScroll(),
                flags: quatFlag | PmpCodec.flagStickValid,
                stickX: bs.stickX(), stickY: bs.stickY()
            )
        case .wiimote:
            // Móvil de lado (NES): sensores girados según diga la pantalla
            // (Route.sidewaysRotation); en vertical (rotation0), tal cual
            let rot = packetRotation
            packet = PmpCodec.encodeInput(
                sessionId: sessionId, seq: seq, tSensorUs: tUs,
                quat: Frame.remapQuat(quat, rot), gyro: Frame.remapGyro(gyro, rot), accel: Frame.remapAccel(accel, rot),
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
