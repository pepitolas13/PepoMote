package dev.pepotech.pepomote.control

/**
 * Sitio que ocupa un botón en la pantalla, en píxeles de la raíz (los que da
 * `boundsInRoot`). [circular]: los botones redondos ([bit] A, 1, 2, Home…)
 * se aciertan por su círculo, no por su cuadrado, así deslizar entre dos
 * redondos pegados no pulsa el de la esquina. Sin Compose: se prueba en la JVM.
 */
class PressZone(
    val bit: Int,
    val left: Float,
    val top: Float,
    val right: Float,
    val bottom: Float,
    val circular: Boolean = false
) {
    val centerX: Float get() = (left + right) / 2f
    val centerY: Float get() = (top + bottom) / 2f

    /** ¿El punto cae dentro? El círculo es el mayor que cabe en la caja. */
    fun contains(x: Float, y: Float): Boolean {
        if (x < left || x > right || y < top || y > bottom) return false
        if (!circular) return true
        val r = minOf(right - left, bottom - top) / 2f
        val dx = x - centerX
        val dy = y - centerY
        return dx * dx + dy * dy <= r * r
    }

    /** Distancia al cuadrado hasta el centro, para deshacer solapes. */
    fun distance2(x: Float, y: Float): Float {
        val dx = x - centerX
        val dy = y - centerY
        return dx * dx + dy * dy
    }
}

/** Qué botón hay bajo un dedo. */
object PressHit {
    /**
     * El botón que ocupa ([x], [y]), o null si ahí no hay ninguno. Si dos se
     * solapan (el rombo A/B/X/Y, la cruceta y A), gana aquel a cuyo centro
     * está más cerca el dedo.
     */
    fun resolve(zones: Collection<PressZone>, x: Float, y: Float): Int? {
        var best: PressZone? = null
        var bestD = Float.MAX_VALUE
        for (zone in zones) {
            if (!zone.contains(x, y)) continue
            val d = zone.distance2(x, y)
            if (best == null || d < bestD) {
                best = zone
                bestD = d
            }
        }
        return best?.bit
    }
}
