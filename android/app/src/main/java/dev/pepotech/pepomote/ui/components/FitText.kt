package dev.pepotech.pepomote.ui.components

import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp

/**
 * Texto de una sola línea que se encoge hasta caber en el espacio que le
 * dan (nunca se corta): etiquetas de botones y del selector en móviles
 * pequeños. Empieza en `maxSize` y baja de medio en medio punto hasta
 * `minSize`.
 */
@Composable
fun FitText(
    text: String,
    style: TextStyle,
    maxSize: TextUnit,
    modifier: Modifier = Modifier,
    minSize: TextUnit = 8.sp,
    textAlign: TextAlign = TextAlign.Center
) {
    BasicText(
        text = text,
        modifier = modifier,
        style = style.copy(textAlign = textAlign),
        maxLines = 1,
        softWrap = false,
        autoSize = TextAutoSize.StepBased(minFontSize = minSize, maxFontSize = maxSize, stepSize = 0.5.sp)
    )
}
