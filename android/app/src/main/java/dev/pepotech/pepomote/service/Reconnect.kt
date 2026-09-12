package dev.pepotech.pepomote.service

/**
 * Reconexión automática tras una caída con la sesión viva (Wi-Fi que se va,
 * receptor reiniciado…): función pura, sin Android, testeada en
 * ReconnectTest. Espera creciente entre intentos y rendición a los dos
 * minutos; entre intentos el servicio busca el PC por si cambió de IP.
 */
object Reconnect {
    private val DELAYS_MS = longArrayOf(1_000, 2_000, 4_000, 8_000, 15_000)

    /** Sin conseguir volver en este tiempo, la conexión se da por perdida. */
    const val GIVE_UP_MS = 120_000L

    /** Espera antes del intento `attempt` (1 = el primero): 1, 2, 4, 8, 15, 15… s. */
    fun delayMs(attempt: Int): Long = DELAYS_MS[(attempt - 1).coerceIn(0, DELAYS_MS.size - 1)]

    /** Ya toca rendirse: la caída fue en `droppedAtMs` y ahora es `nowMs` (mismo reloj). */
    fun giveUp(droppedAtMs: Long, nowMs: Long): Boolean = nowMs - droppedAtMs >= GIVE_UP_MS
}
