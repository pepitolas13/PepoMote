import Foundation

/// Reconexión automática tras una caída con la sesión viva (Wi-Fi que se va,
/// receptor reiniciado…): función pura, testeada en ReconnectTests. Espera
/// creciente entre intentos y rendición a los dos minutos; entre intentos el
/// servicio busca el PC por si cambió de IP.
enum Reconnect {
    private static let delaysMs: [Int64] = [1_000, 2_000, 4_000, 8_000, 15_000]

    /// Sin conseguir volver en este tiempo, la conexión se da por perdida.
    static let giveUpMs: Int64 = 120_000

    /// Espera antes del intento `attempt` (1 = el primero): 1, 2, 4, 8, 15, 15… s.
    static func delayMs(_ attempt: Int) -> Int64 {
        delaysMs[min(max(attempt - 1, 0), delaysMs.count - 1)]
    }

    /// Ya toca rendirse: la caída fue en `droppedAtMs` y ahora es `nowMs` (mismo reloj).
    static func giveUp(_ droppedAtMs: Int64, _ nowMs: Int64) -> Bool {
        nowMs - droppedAtMs >= giveUpMs
    }
}
