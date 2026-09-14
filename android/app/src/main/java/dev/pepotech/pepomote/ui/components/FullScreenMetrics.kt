package dev.pepotech.pepomote.ui.components

import kotlin.math.roundToInt

/** Rectángulo (x, y, ancho, alto) en las unidades del contenedor. */
data class FitRect(val x: Float, val y: Float, val w: Float, val h: Float)

/**
 * Geometría de la «pantalla del GamePad a pantalla completa» (idéntica en
 * iOS, `FullScreenMetrics.swift`, con los mismos vectores de test): la
 * imagen ajustada a su proporción real y centrada sobre fondo negro, el
 * táctil solo sobre ella, y el tamaño que se pide al PC.
 */
object FullScreenMetrics {
    const val NATIVE_W = 854
    const val NATIVE_H = 480

    /**
     * Imagen `imageW×imageH` (sin fotograma aún: 16:9) ajustada y centrada en
     * un contenedor `containerW×containerH`; vacío si el contenedor no tiene
     * tamaño.
     */
    fun fitRect(containerW: Float, containerH: Float, imageW: Float, imageH: Float): FitRect {
        if (containerW <= 0f || containerH <= 0f) return FitRect(0f, 0f, 0f, 0f)
        val known = imageW > 0f && imageH > 0f
        val iw = if (known) imageW else 16f
        val ih = if (known) imageH else 9f
        val scale = minOf(containerW / iw, containerH / ih)
        val w = iw * scale
        val h = ih * scale
        return FitRect((containerW - w) / 2f, (containerH - h) / 2f, w, h)
    }

    /** ¿El punto cae sobre la imagen? Fuera (bandas negras) no es un toque. */
    fun contains(r: FitRect, x: Float, y: Float): Boolean =
        r.w > 0f && r.h > 0f && x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h

    /** Fracción 0..65535 del toque dentro de `r`, recortada a sus bordes. */
    fun fraction(r: FitRect, x: Float, y: Float): Pair<Int, Int> {
        if (r.w <= 0f || r.h <= 0f) return 0 to 0
        val fx = ((x - r.x) / r.w).coerceIn(0f, 1f)
        val fy = ((y - r.y) / r.h).coerceIn(0f, 1f)
        return (fx * 65535f).roundToInt() to (fy * 65535f).roundToInt()
    }

    /**
     * Tamaño pedido al PC en pantalla completa: el del área en píxeles, como
     * mucho el nativo del GamePad (854×480); el PC conserva la proporción.
     */
    fun streamRequest(containerPxW: Int, containerPxH: Int): Pair<Int, Int> =
        if (containerPxW < 1 || containerPxH < 1) NATIVE_W to NATIVE_H
        else minOf(NATIVE_W, containerPxW) to minOf(NATIVE_H, containerPxH)
}
