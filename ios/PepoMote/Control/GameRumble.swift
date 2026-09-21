import CoreHaptics
import Foundation
import UIKit

/// Motor de la vibración que piden los juegos (RUMBLE del receptor,
/// PROTOCOL.md §4.5), aparte de la háptica de los botones (`Haptics`). Solo
/// en el hilo principal: el servicio salta a la cola principal antes de
/// llamar aquí, y así ningún error del motor toca el hilo de red.
///
/// Con Core Haptics (iPhone 8 y posteriores): un patrón continuo de 30 s en
/// bucle, que se para al instante y cambia de intensidad sin cortes; si el
/// sistema reinicia el motor (llamada, memoria) se rearma solo. Sin Core
/// Haptics pero con Taptic (iPhone 7): un impacto fuerte cada 80 ms mientras
/// dure, lo más parecido a una vibración continua que da ese motor. Un iPad
/// no tiene motor: aquí no se hace nada y Ajustes lo dice.
final class GameRumble {
    static let shared = GameRumble()

    /// Hay motor que mover: Core Haptics, o el Taptic de un iPhone anterior.
    let hasMotor: Bool
    private let coreHaptics: Bool
    private var engine: CHHapticEngine?
    /// El motor está arrancado. El sistema lo para en segundo plano y al
    /// reiniciarse; un jugador sobre un motor parado falla, así que se arranca
    /// solo cuando hace falta.
    private var engineRunning = false
    private var player: CHHapticAdvancedPatternPlayer?
    /// Nivel efectivo (0..1) que suena ahora; 0 = parado. Con él se rehace el
    /// patrón tras un reinicio del motor y lo lee cada impacto del Taptic.
    private var level: Float = 0
    private let impact = UIImpactFeedbackGenerator(style: .heavy)
    private var impactTimer: Timer?
    private var pulseEnd: DispatchWorkItem?
    private var backgroundObserver: NSObjectProtocol?

    private init() {
        coreHaptics = CHHapticEngine.capabilitiesForHardware().supportsHaptics
        hasMotor = coreHaptics || UIDevice.current.userInterfaceIdiom == .phone
        // En segundo plano iOS para el motor de todas formas: se deja todo en orden
        backgroundObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.didEnterBackgroundNotification, object: nil, queue: .main
        ) { [weak self] _ in
            self?.stop()
        }
    }

    // MARK: - Interfaz común (contrato §4)

    /// Empieza a vibrar a `level` (0..1, ya con la escala del ajuste). Con 0
    /// (ajuste Apagada) no suena nada.
    func start(level: Float) {
        let l = GameRumble.clamp(level)
        pulseEnd?.cancel()
        pulseEnd = nil
        guard hasMotor, l > 0 else {
            stop()
            return
        }
        self.level = l
        if coreHaptics {
            startPlayer(l)
        } else {
            startImpacts(l)
        }
    }

    /// Cambia la intensidad sin cortar la vibración.
    func change(level: Float) {
        let l = GameRumble.clamp(level)
        guard hasMotor else { return }
        if l <= 0 {
            stop()
            return
        }
        if self.level <= 0 {
            start(level: l)
            return
        }
        self.level = l
        guard coreHaptics else { return } // Taptic: el temporizador lee `level` en cada impacto
        guard let p = player else {
            startPlayer(l)
            return
        }
        do {
            try p.sendParameters([GameRumble.intensityControl(l)], atTime: CHHapticTimeImmediate)
        } catch {
            // El jugador ya no vale (motor reiniciado por el sistema): patrón nuevo
            startPlayer(l)
        }
    }

    /// Para del todo.
    func stop() {
        pulseEnd?.cancel()
        pulseEnd = nil
        level = 0
        stopPlayer()
        impactTimer?.invalidate()
        impactTimer = nil
    }

    /// Un pulso de `ms` (el botón «Probar» de Ajustes); con nivel 0 no hace nada.
    func pulse(level: Float, ms: Int = 300) {
        guard hasMotor, level > 0 else { return }
        start(level: level)
        let work = DispatchWorkItem { [weak self] in
            self?.stop()
        }
        pulseEnd = work
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(max(1, ms)), execute: work)
    }

    // MARK: - Core Haptics

    /// El motor, creado la primera vez y arrancado si estaba parado; nil si
    /// no se puede (entonces no hay vibración, sin más).
    private func runningEngine() -> CHHapticEngine? {
        guard coreHaptics else { return nil }
        if engine == nil {
            do {
                let e = try CHHapticEngine()
                e.playsHapticsOnly = true
                e.resetHandler = { [weak self] in
                    DispatchQueue.main.async { self?.onEngineReset() }
                }
                e.stoppedHandler = { [weak self] _ in
                    DispatchQueue.main.async { self?.onEngineStopped() }
                }
                engine = e
                engineRunning = false
            } catch {
                return nil
            }
        }
        guard let e = engine else { return nil }
        if !engineRunning {
            do {
                try e.start()
                engineRunning = true
            } catch {
                return nil
            }
        }
        return e
    }

    /// El sistema reinició el motor: los jugadores ya no valen; se arranca de
    /// nuevo al primer uso y, si estaba vibrando, se rehace el patrón ya.
    private func onEngineReset() {
        player = nil
        engineRunning = false
        if level > 0 { startPlayer(level) }
    }

    /// El motor se paró (segundo plano, interrupción): el jugador se suelta.
    private func onEngineStopped() {
        player = nil
        engineRunning = false
    }

    private static func intensityControl(_ level: Float) -> CHHapticDynamicParameter {
        CHHapticDynamicParameter(parameterID: .hapticIntensityControl, value: level, relativeTime: 0)
    }

    /// Patrón continuo de 30 s en bucle. La intensidad del evento es 1 y el
    /// nivel va en el parámetro dinámico, porque `hapticIntensityControl`
    /// MULTIPLICA la del evento: así `change` manda el nivel absoluto.
    private func startPlayer(_ level: Float) {
        stopPlayer()
        guard let e = runningEngine() else { return }
        let intensity = CHHapticEventParameter(parameterID: .hapticIntensity, value: 1)
        let sharpness = CHHapticEventParameter(parameterID: .hapticSharpness, value: 0.4)
        let event = CHHapticEvent(eventType: .hapticContinuous, parameters: [intensity, sharpness], relativeTime: 0, duration: 30)
        do {
            let pattern = try CHHapticPattern(events: [event], parameters: [GameRumble.intensityControl(level)])
            let p = try e.makeAdvancedPlayer(with: pattern)
            p.loopEnabled = true
            try p.start(atTime: CHHapticTimeImmediate)
            player = p
        } catch {
            player = nil
        }
    }

    private func stopPlayer() {
        if let p = player {
            try? p.stop(atTime: CHHapticTimeImmediate)
        }
        player = nil
    }

    // MARK: - Taptic (sin Core Haptics)

    /// Un impacto fuerte cada 80 ms mientras dure, con la intensidad del nivel.
    private func startImpacts(_ level: Float) {
        impact.prepare()
        impact.impactOccurred(intensity: CGFloat(level))
        guard impactTimer == nil else { return }
        let t = Timer(timeInterval: 0.08, repeats: true) { [weak self] _ in
            guard let self = self, self.level > 0 else { return }
            self.impact.impactOccurred(intensity: CGFloat(self.level))
        }
        RunLoop.main.add(t, forMode: .common)
        impactTimer = t
    }

    private static func clamp(_ v: Float) -> Float {
        if v.isNaN { return 0 }
        return Swift.min(Swift.max(v, 0), 1)
    }
}
