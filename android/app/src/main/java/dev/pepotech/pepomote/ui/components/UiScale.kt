package dev.pepotech.pepomote.ui.components

/**
 * Escala de la interfaz en pantallas grandes (tablets). El factor SOLO
 * agranda: cualquier móvil queda exactamente como hasta ahora (factor 1) y
 * una tablet crece con la pantalla hasta [MAX]. Cada pantalla pasa su base:
 * la anchura del móvil más grande (440 dp) o la anchura natural de su
 * trazado, y la altura de lo que escala descontando lo fijo (cabecera,
 * chips, selector), que no crece. Misma regla y mismos números que en iOS
 * (UiScale.swift). Sin Compose: se prueba con JUnit.
 */
object UiScale {
    const val MAX = 2f

    /** GamePad apaisado: el móvil más grande de lado. */
    const val PHONE_LANDSCAPE_W = 956f
    const val PHONE_LANDSCAPE_H = 440f

    /**
     * Mando vertical y Nunchuk: 632 dp escalan (huecos mínimos 10+16+16+14+10,
     * cruceta 168, fila 64, A 148, 1/2 52, multimedia 46, B 88) y 176 dp son
     * fijos (10 + cabecera 44 + chips 42 + selector «En Cemu soy» 64 + 4 + 12).
     */
    const val REMOTE_BASE_W = 440f
    const val REMOTE_BASE_H = 632f
    const val REMOTE_FIXED = 176f

    /**
     * Mando apaisado (NES): anchura natural del trazado a escala 1
     * (34+190 | 122 | 232+30 = 578) más dos huecos.
     */
    const val LANDSCAPE_BASE_W = 700f
    const val LANDSCAPE_BASE_H = 440f

    /** max(1, min(ancho/baseAncho, (alto − fixed)/baseAlto, max)). */
    fun factor(w: Float, h: Float, baseW: Float, baseH: Float, fixed: Float = 0f, max: Float = MAX): Float {
        if (w <= 0f || h <= 0f || baseW <= 0f || baseH <= 0f) return 1f
        val f = minOf(w / baseW, (h - fixed) / baseH, max)
        return maxOf(1f, f)
    }

    /** Mando vertical y Nunchuk. */
    fun remote(w: Float, h: Float): Float = factor(w, h, REMOTE_BASE_W, REMOTE_BASE_H, REMOTE_FIXED)

    /** Mando apaisado (NES). */
    fun landscape(w: Float, h: Float): Float = factor(w, h, LANDSCAPE_BASE_W, LANDSCAPE_BASE_H)

    /** GamePad de Wii U (topes de gatillos y botones). */
    fun gamePad(w: Float, h: Float): Float = factor(w, h, PHONE_LANDSCAPE_W, PHONE_LANDSCAPE_H)
}
