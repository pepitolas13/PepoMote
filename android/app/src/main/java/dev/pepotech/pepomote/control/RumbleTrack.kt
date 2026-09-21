package dev.pepotech.pepomote.control

/**
 * Lo que la máquina de la vibración le manda al motor. `level` es el nivel
 * crudo 0..1 del receptor, SIN la escala del ajuste (esa se aplica al mandar
 * la orden, para que cambiar el ajuste en Ajustes valga al instante).
 */
sealed class RumbleCommand {
    data class Start(val level: Float) : RumbleCommand()
    data class Change(val level: Float) : RumbleCommand()
    object Stop : RumbleCommand()
}

/**
 * Máquina de estados pura de la vibración de los juegos (PROTOCOL.md §4.5),
 * calcada de `RumbleTrack.swift` y del `rumble::Track` del receptor: los
 * RUMBLE entran por [apply], el reloj por [tick], y sale la orden para el
 * motor (o nada). Sin hilos ni motor: los tests la recorren con un reloj de
 * mentira.
 *
 * Es ESTADO, no evento: el receptor repite el RUMBLE cada 100 ms mientras
 * vibra, y un refresco solo alarga la caducidad. Si deja de llegar (Wi-Fi
 * caída, receptor cerrado) [tick] para solo al vencer el `ttl`.
 */
class RumbleTrack {
    private var lastSeq: Int? = null
    private var active = false
    private var level = 0f
    private var expiresAtMs: Long? = null

    /**
     * Un RUMBLE del receptor. `seq` viejo o repetido (con desbordamiento: la
     * resta como entero con signo de 32 bits ≤ 0) → null sin tocar nada.
     */
    fun apply(seq: Int, strong: Int, weak: Int, ttlMs: Int, nowMs: Long): RumbleCommand? {
        val last = lastSeq
        if (last != null && seq - last <= 0) return null
        lastSeq = seq
        val raw = maxOf(strong, weak).coerceIn(0, 255) / 255f
        if (raw == 0f) {
            expiresAtMs = null
            if (active) {
                active = false
                level = 0f
                return RumbleCommand.Stop
            }
            return null
        }
        val ttl = if (ttlMs == 0) DEFAULT_TTL_MS else ttlMs.toLong()
        expiresAtMs = nowMs + ttl
        if (!active) {
            active = true
            level = raw
            return RumbleCommand.Start(raw)
        }
        // Un cambio menor que medio paso de 8 bits es el mismo nivel
        if (kotlin.math.abs(raw - level) > 0.5f / 255f) {
            level = raw
            return RumbleCommand.Change(raw)
        }
        return null
    }

    /** El reloj: vencido el `ttl` sin refresco, se para. */
    fun tick(nowMs: Long): RumbleCommand? {
        val expires = expiresAtMs
        if (!active || expires == null || nowMs < expires) return null
        active = false
        level = 0f
        expiresAtMs = null
        return RumbleCommand.Stop
    }

    /**
     * Borra todo (`lastSeq` incluido: otra sesión numera desde cero). Al
     * perder el enlace, cambiar de sesión, ir a segundo plano o pasar a puntero.
     */
    fun reset(): RumbleCommand? {
        val wasActive = active
        lastSeq = null
        active = false
        level = 0f
        expiresAtMs = null
        return if (wasActive) RumbleCommand.Stop else null
    }

    companion object {
        /** Sin `ttl` en el paquete (0) se usa lo que manda el receptor entre refrescos. */
        const val DEFAULT_TTL_MS = 400L
    }
}

/**
 * Ajuste «Vibración en los juegos»: cuánto de lo que pide el juego llega al
 * motor. Se guarda como cadena (la misma en iOS y en Linux móvil); un valor
 * desconocido se lee como [NORMAL], que es el de serie.
 */
enum class RumblePref(val key: String, val factor: Float) {
    HIGH("high", 1f),
    NORMAL("normal", 0.65f),
    LOW("low", 0.35f),
    OFF("off", 0f);

    companion object {
        /** En el orden de los chips de Ajustes. */
        val ALL = listOf(HIGH, NORMAL, LOW, OFF)

        private fun parse(raw: String?): RumblePref =
            ALL.firstOrNull { it.key == raw } ?: NORMAL

        /** Escala que multiplica el nivel crudo del receptor (null o desconocido → normal). */
        fun scale(raw: String?): Float = parse(raw).factor

        /** El valor guardado, saneado (null o desconocido → "normal"). */
        fun normalize(raw: String?): String = parse(raw).key
    }
}
