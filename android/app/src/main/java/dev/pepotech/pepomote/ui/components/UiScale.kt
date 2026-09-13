package dev.pepotech.pepomote.ui.components

/**
 * Escala de la interfaz en pantallas grandes (tablets). El factor SOLO
 * agranda: la base es un móvil grande (440×956 dp), así cualquier móvil
 * queda exactamente como hasta ahora (factor 1) y una tablet crece con
 * tope. Misma regla y mismos números que en iOS (UiScale.swift). Sin
 * Compose: se prueba en la JVM.
 */
object UiScale {
    const val PHONE_PORTRAIT_W = 440f
    const val PHONE_PORTRAIT_H = 956f
    const val PHONE_LANDSCAPE_W = 956f
    const val PHONE_LANDSCAPE_H = 440f

    /** max(1, min(ancho/baseAncho, alto/baseAlto, max)). */
    fun factor(w: Float, h: Float, baseW: Float, baseH: Float, max: Float): Float {
        if (w <= 0f || h <= 0f || baseW <= 0f || baseH <= 0f) return 1f
        val f = minOf(w / baseW, h / baseH, max)
        return maxOf(1f, f)
    }

    /** Solo por ancho (GamePad apaisado): clamp(ancho/base, 1, max). */
    fun factorWidth(w: Float, base: Float, max: Float): Float {
        if (w <= 0f || base <= 0f) return 1f
        return maxOf(1f, minOf(w / base, max))
    }
}
