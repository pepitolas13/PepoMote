package dev.pepotech.pepomote.ui.screens

import dev.pepotech.pepomote.ui.components.UiScale
import kotlin.math.roundToInt

/**
 * Medidas (dp) del cuerpo del mando vertical (cruceta, −/diana/+, A, 1/Home/2,
 * multimedia y B) para el alto que queda bajo la cabecera, [bodyH] (margen,
 * cabecera, chips y selector ya descontados: los mide Compose). En un móvil
 * bajo todo lo que escala se encoge a la vez (`s` < 1) hasta que quepa, como
 * en iOS (RemoteMetrics) y en Linux móvil; en cualquier otro móvil las medidas
 * de siempre (`s` = 1, exacto); en una tablet todo crece a la vez ([UiScale],
 * `grow` > 1) y nunca encoge, aunque la cabecera sea alta. Sin Compose: se
 * prueba en la JVM. [mediaRow]: la fila multimedia se enseña (solo en modo
 * puntero, o en todos con el ajuste); sin ella, ni su botón ni su hueco
 * cuentan y [mediaOpen] da igual.
 */
class WiiRemoteMetrics(val grow: Float, val bodyH: Float, mediaOpen: Boolean = false, val mediaRow: Boolean = true) {
    val mediaOpen = mediaRow && mediaOpen
    /** Tablet: los huecos entre grupos se vuelven flexibles (reparten la holgura). */
    val flexible = grow > 1f

    /** Lo que escala, a 1: huecos 18+16+16+14 (+10 con la fila multimedia), cruceta 168, fila 64, A 148, 1/2 52, B 88 (+ fila multimedia abierta). */
    private val scalable = SCALABLE - (if (mediaRow) 0f else MEDIA_GAP) + (if (this.mediaOpen) MEDIA else 0f)
    private val fit = (bodyH - (if (mediaRow) MEDIA_BUTTON else 0f) - BOTTOM) / scalable

    /**
     * Escala del cuerpo: en tablet `grow` (crece, nunca encoge); en móvil 1 si
     * cabe y, si no, lo justo para que quepa, hasta [FLOOR].
     */
    val s = if (flexible) grow else fit.coerceIn(FLOOR, 1f)

    val cross = 168f * s
    val small = 54f * s
    val recenter = 64f * s
    val big = 148f * s
    val one = 52f * s
    val media = 46f * s
    val trigger = 88f * s
    val spacing = 20f * s

    /** Hueco entre grupos, escalado como los botones. */
    fun gap(base: Float): Float = base * s

    /** Tamaño de texto (sp): crece con la tablet; en un móvil bajo NO se encoge (como iOS). */
    fun text(base: Int): Int = (base * grow).roundToInt()

    /** Alto mínimo del cuerpo con estas medidas: para las pruebas. */
    val bodyMin = gap(18f) + cross + gap(16f) + recenter + gap(16f) + big + gap(14f) + one +
        (if (mediaRow) gap(MEDIA_GAP) + MEDIA_BUTTON + (if (this.mediaOpen) media else 0f) else 0f) + trigger + BOTTOM

    /** Lo que se sale por abajo (0 si cabe): solo por debajo de [FLOOR]; ahí se recorta el cuerpo, nunca la B. */
    val overflow = (bodyMin - bodyH).coerceAtLeast(0f)

    companion object {
        /** Margen superior de la columna, encima de la cabecera. */
        const val TOP = 10f
        /** Botón «Multimedia ▼» (TextButton de Material 3): no escala. */
        const val MEDIA_BUTTON = 40f
        /** Margen bajo la B. */
        const val BOTTOM = 12f
        /** Huecos 18+16+16+14+10 + cruceta 168 + fila 64 + A 148 + 1/2 52 + B 88. */
        const val SCALABLE = 594f
        /** Fila multimedia desplegada (botones de 46). */
        const val MEDIA = 46f
        /** Hueco entre 1/2 y la fila multimedia (dentro de [SCALABLE]). */
        const val MEDIA_GAP = 10f
        /**
         * Por debajo no se encoge más (1 y 2 quedan en 31 dp): con menos alto
         * (pantalla dividida, «Tamaño de pantalla» enorme) el cuerpo se recorta
         * por abajo y la B sigue entera.
         */
        const val FLOOR = 0.6f

        /** Medidas para una pantalla de [w]×[h] dp con una cabecera de [headerH] dp. */
        fun forScreen(w: Float, h: Float, headerH: Float, mediaOpen: Boolean = false, mediaRow: Boolean = true): WiiRemoteMetrics =
            WiiRemoteMetrics(UiScale.remote(w, h), h - TOP - headerH, mediaOpen, mediaRow)
    }
}
