import Foundation

/// Lo que la máquina de la vibración le manda al motor. `level` es el nivel
/// crudo 0..1 del receptor, SIN la escala del ajuste (esa se aplica al
/// mandar la orden, para que cambiar el ajuste en Ajustes valga al instante).
enum RumbleCommand: Equatable {
    case start(Float)
    case change(Float)
    case stop
}

/// Máquina de estados pura de la vibración de los juegos (contrato §2),
/// calcada en Kotlin (`RumbleTrack.kt`) y en Rust: los RUMBLE del receptor
/// entran por `apply`, el reloj por `tick`, y sale la orden para el motor (o
/// nada). Sin hilos ni motor: los tests la recorren con un reloj de mentira.
///
/// Es ESTADO, no evento: el receptor repite el RUMBLE cada 100 ms mientras
/// vibra, y un refresco solo alarga la caducidad. Si deja de llegar (Wi-Fi
/// caída, receptor cerrado) `tick` para solo al vencer `ttl`.
final class RumbleTrack {
    /// Sin `ttl` en el paquete (0) se usa lo que manda el receptor entre refrescos.
    static let defaultTtlMs: UInt64 = 400

    private var lastSeq: UInt32?
    private var active = false
    private var level: Float = 0
    private var expiresAtMs: UInt64?

    /// Un RUMBLE del receptor. `seq` viejo o repetido (con desbordamiento: la
    /// diferencia como entero con signo de 32 bits ≤ 0) → nil sin tocar nada.
    func apply(seq: UInt32, strong: UInt8, weak: UInt8, ttlMs: UInt16, nowMs: UInt64) -> RumbleCommand? {
        if let last = lastSeq, Int32(bitPattern: seq &- last) <= 0 { return nil }
        lastSeq = seq
        let raw = Float(max(strong, weak)) / 255
        if raw == 0 {
            expiresAtMs = nil
            if active {
                active = false
                level = 0
                return .stop
            }
            return nil
        }
        let ttl: UInt64 = ttlMs == 0 ? RumbleTrack.defaultTtlMs : UInt64(ttlMs)
        expiresAtMs = nowMs &+ ttl
        if !active {
            active = true
            level = raw
            return .start(raw)
        }
        // Un cambio menor que medio paso de 8 bits es el mismo nivel
        if abs(raw - level) > 0.5 / 255 {
            level = raw
            return .change(raw)
        }
        return nil
    }

    /// El reloj: vencido el `ttl` sin refresco, se para.
    func tick(nowMs: UInt64) -> RumbleCommand? {
        guard active, let expires = expiresAtMs, nowMs >= expires else { return nil }
        active = false
        level = 0
        expiresAtMs = nil
        return .stop
    }

    /// Borra todo (`lastSeq` incluido: otra sesión numera desde cero). Al
    /// perder el enlace, cambiar de sesión, ir a segundo plano o pasar a puntero.
    func reset() -> RumbleCommand? {
        let wasActive = active
        lastSeq = nil
        active = false
        level = 0
        expiresAtMs = nil
        return wasActive ? .stop : nil
    }
}

/// Ajuste «Vibración en los juegos»: cuánto de lo que pide el juego llega al
/// motor. Se guarda como cadena (la misma en Android y Linux); un valor
/// desconocido se lee como `normal`, que es el de serie.
enum RumblePref: String {
    case high
    case normal
    case low
    case off

    /// En el orden de los chips de Ajustes.
    static let all: [RumblePref] = [.high, .normal, .low, .off]

    /// Escala que multiplica el nivel crudo del receptor.
    var factor: Float {
        switch self {
        case .high: return 1
        case .normal: return 0.65
        case .low: return 0.35
        case .off: return 0
        }
    }

    /// Escala de un valor guardado (nil o desconocido → normal).
    static func scale(_ raw: String?) -> Float {
        RumblePref.parse(raw).factor
    }

    /// El valor guardado, saneado (nil o desconocido → "normal").
    static func normalize(_ raw: String?) -> String {
        RumblePref.parse(raw).rawValue
    }

    private static func parse(_ raw: String?) -> RumblePref {
        guard let raw = raw, let pref = RumblePref(rawValue: raw) else { return .normal }
        return pref
    }
}
