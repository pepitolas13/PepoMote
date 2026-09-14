package dev.pepotech.pepomote.ui.components

/**
 * Medidas del GamePad de Wii U para un tamaño de pantalla (dp, todo escalado
 * al tamaño real; misma fórmula letra por letra que `PadMetrics` en iOS). En
 * una tablet los topes (gatillos, botones) crecen con [k]; en cualquier móvil
 * `k` = 1. Stick y cruceta (o rombo) llenan la columna lateral y lo que
 * queda de altura bajo los gatillos.
 *
 * Sin pantalla táctil (Pro Controller, o el ajuste «GamePad sin pantalla») el
 * centro solo necesita la fila − · Home · +: se prueban dos trazados y se
 * queda el que dé pads más grandes. Apilado (columnas laterales anchas:
 * tablets) o [row], con stick y cruceta uno al lado del otro, L3/R3 junto a
 * los gatillos y el centro como columna vertical (móviles, justos de altura
 * y sobrados de ancho). Con pantalla, en un móvil todo mide lo de siempre.
 */
data class PadMetrics(
    val w: Float,
    val h: Float,
    val k: Float,
    /** Sin pantalla táctil: Pro Controller o el ajuste «GamePad sin pantalla». */
    val noScreen: Boolean,
    val pro: Boolean,
    /** Stick y cruceta (o rombo) uno al lado del otro y el centro en columna. */
    val row: Boolean,
    val gap: Float,
    val headerH: Float,
    val selectorH: Float,
    val bodyH: Float,
    val sideW: Float,
    val shoulderH: Float,
    val shoulderW: Float,
    val padSize: Float,
    val clickSize: Float,
    val faceBtn: Float,
    val centerW: Float,
    val bottomRowH: Float,
    /** Fila (o columna) del centro: −, Home, + y las pastillas TV/Pad y Soplar. */
    val rowGap: Float,
    val roundBtn: Float,
    val pillW: Float,
    val pillH: Float,
    val touchW: Float,
    val touchH: Float,
) {
    /** Tamaño de texto de los botones (sp): crece con la tablet. */
    fun text(base: Float): Float = base * k
}

fun padMetrics(w: Float, h: Float, noScreen: Boolean = false, pro: Boolean = false): PadMetrics {
    val k = UiScale.gamePad(w, h)
    val none = noScreen || pro
    val gap = 6f
    val bodyH = h - 38f - 36f - gap * 3
    val shoulderH = minOf(maxOf(bodyH * 0.09f, 26f), 40f * k)
    val bottomRowH = minOf(maxOf(bodyH * 0.16f, 44f), 60f * k)
    val clickMax = 44f * k
    // Dos pads apilados bajo los gatillos
    val padH = (bodyH - shoulderH * 2 - gap * 3) / 2
    val row: Boolean
    val sideW: Float
    val pad: Float
    if (!none) {
        row = false
        sideW = w * 0.29f
        pad = minOf(padH, sideW - clickMax - gap)
    } else {
        val rb = bottomRowH * 0.85f
        val need = if (pro) rb * 3 + 10f * 2 + 20f else rb * 3 + 10f * 4 + 66f * k * 2 + 20f
        val sideS = (w - need - gap * 2) / 2
        val padStack = minOf(padH, sideS - clickMax - gap)
        val cw = if (pro) rb else 66f * k
        val padRow = minOf(bodyH - shoulderH * 2 - gap * 2, (w - cw - gap * 4) / 4)
        row = padRow > padStack
        pad = if (row) padRow else padStack
        sideW = if (row) maxOf(pad, 40f) * 2 + gap else sideS
    }
    val padSize = maxOf(pad, 40f)
    val clickSize = minOf(maxOf(padSize * 0.30f, 30f), 44f * k)
    val shoulderW = minOf(maxOf(sideW * 0.6f, 90f), 150f * k)
    val faceBtn = padSize / 2.6f
    // En fila el centro es lo que dejan los pads: justo su columna, o más si los limitó la altura
    val centerW = if (row) w - sideW * 2 - gap * 2 else maxOf(w - sideW * 2 - gap * 2, 60f)
    val rowGap: Float
    val roundBtn: Float
    val pillW: Float
    if (row) {
        rowGap = gap
        roundBtn = bottomRowH * 0.85f
        pillW = if (pro) 0f else 66f * k
    } else {
        // La fila entera tiene que caber en el centro: pastillas y círculos se encogen juntos
        rowGap = if (centerW < 300f) 6f else 10f
        pillW = if (pro) 0f else minOf(maxOf(centerW * 0.22f, 44f), 66f * k)
        val pills = if (pro) 0 else 2
        roundBtn = minOf(maxOf((centerW - rowGap * (2 + pills) - pillW * pills) / 3, 28f), bottomRowH * 0.85f)
    }
    val touchW = maxOf(minOf(centerW, (bodyH - bottomRowH - gap * 2) * (16f / 9f)), 64f)
    return PadMetrics(
        w = w, h = h, k = k, noScreen = none, pro = pro, row = row,
        gap = gap, headerH = 38f, selectorH = 36f, bodyH = bodyH,
        sideW = sideW, shoulderH = shoulderH, shoulderW = shoulderW,
        padSize = padSize, clickSize = clickSize, faceBtn = faceBtn,
        centerW = centerW, bottomRowH = bottomRowH,
        rowGap = rowGap, roundBtn = roundBtn, pillW = pillW, pillH = 30f * k,
        touchW = touchW, touchH = touchW * (9f / 16f),
    )
}
