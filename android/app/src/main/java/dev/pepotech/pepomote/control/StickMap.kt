package dev.pepotech.pepomote.control

import kotlin.math.hypot
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * Del desplazamiento del pulgar sobre el stick virtual (px, y hacia ABAJO como
 * en pantalla) al stick del Nunchuk (−127..127, +y ARRIBA). Zona muerta pequeña
 * en el centro, reescalada para que el borde de la zona muerta sea el 0 y el
 * radio el tope: sin escalón al salir del centro. Puro, sin Android: testeable.
 */
object StickMap {
    /** Fracción del radio que se ignora en el centro. */
    const val DEAD_ZONE = 0.08f

    /** @return (x, y) del stick; (0, 0) dentro de la zona muerta. */
    fun map(dx: Float, dy: Float, radius: Float): Pair<Int, Int> {
        val len = hypot(dx, dy)
        if (radius <= 0f || len <= 0f) return 0 to 0
        val m = len / radius
        if (m < DEAD_ZONE) return 0 to 0
        val scaled = (min(m, 1f) - DEAD_ZONE) / (1f - DEAD_ZONE) // 0..1
        val x = (dx / len * scaled * 127f).roundToInt().coerceIn(-127, 127)
        val y = (-dy / len * scaled * 127f).roundToInt().coerceIn(-127, 127)
        return x to y
    }
}
