import AVFoundation
import Foundation

/// Sonidos UI sintetizados propios (los mismos WAV que Android,
/// assets/sounds-src/gen_sounds.py). Se mezclan con la música del sistema y
/// no la interrumpen.
final class UiSounds {
    static let shared = UiSounds()

    /// Interruptor global (Ajustes). La háptica no se toca, solo el audio.
    var enabled = true

    private var players: [String: AVAudioPlayer] = [:]
    private var ready = false

    func prepare() {
        if ready { return }
        ready = true
        enabled = AppPrefs.soundsEnabled
        do {
            try AVAudioSession.sharedInstance().setCategory(.ambient, options: [.mixWithOthers])
            try AVAudioSession.sharedInstance().setActive(true)
        } catch {
            // sin sesión de audio: la app sigue, sin sonidos
        }
        for name in ["ui_blip", "ui_pop", "ui_tick", "ui_connect", "ui_disconnect"] {
            guard let url = Bundle.main.url(forResource: name, withExtension: "wav") else { continue }
            if let p = try? AVAudioPlayer(contentsOf: url) {
                p.prepareToPlay()
                players[name] = p
            }
        }
    }

    private func play(_ name: String, _ vol: Float = 0.8) {
        guard enabled, let p = players[name] else { return }
        p.volume = vol
        p.currentTime = 0
        p.play()
    }

    func blip() { play("ui_blip") }
    func pop() { play("ui_pop") }
    func tick() { play("ui_tick") }
    func connect() { play("ui_connect") }
    func disconnect() { play("ui_disconnect", 0.6) }
}
