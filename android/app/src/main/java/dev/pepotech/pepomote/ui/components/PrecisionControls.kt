package dev.pepotech.pepomote.ui.components

import android.view.HapticFeedbackConstants
import android.view.View
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.control.UiSounds
import dev.pepotech.pepomote.ui.theme.PepoColors
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

/**
 * Precisión (PROTOCOL.md §4.2, bit 29): mientras se mantiene, el puntero del
 * PC se mueve al 40 %. Tira del borde izquierdo en vertical (espejo de la de
 * scroll) y píldora en apaisado; mantener sigue valiendo aunque el dedo se
 * salga. Solo tiene efecto en modo puntero.
 */
@Composable
fun PrecisionStrip(modifier: Modifier) {
    val view = LocalView.current
    var active by remember { mutableStateOf(false) }
    Box(
        modifier = modifier
            .padding(start = 6.dp)
            .background(
                if (active) PepoColors.Glow else PepoColors.CardBorder,
                RoundedCornerShape(15.dp)
            )
            .precisionHold(view) { active = it },
        contentAlignment = Alignment.Center
    ) {
        CrosshairGlyph(PepoColors.TextDim, 16.dp)
    }
}

/** Píldora «Precisión» del mando apaisado (mismo gesto: mantener). */
@Composable
fun PrecisionPill(modifier: Modifier) {
    val view = LocalView.current
    var active by remember { mutableStateOf(false) }
    Row(
        modifier = modifier
            .height(34.dp)
            .background(
                if (active) PepoColors.Glow else PepoColors.CardBorder,
                RoundedCornerShape(17.dp)
            )
            .precisionHold(view) { active = it }
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        CrosshairGlyph(PepoColors.TextDim, 14.dp)
        Spacer(Modifier.width(8.dp))
        Text(stringResource(R.string.precision), style = MaterialTheme.typography.bodyMedium)
    }
}

/**
 * Mantener = bit de precisión; háptico y tic al activar. El gesto sigue al
 * dedo hasta que lo levanta, aunque se salga de la tira (como la de scroll);
 * el `finally` suelta el bit también si el gesto se cancela (ACTION_CANCEL,
 * cambio de pantalla): sin él el puntero se quedaría al 40 % para siempre.
 */
private fun Modifier.precisionHold(view: View, onActive: (Boolean) -> Unit): Modifier =
    pointerInput(Unit) {
        awaitEachGesture {
            val down = awaitFirstDown()
            down.consume()
            try {
                onActive(true)
                ButtonState.set(ButtonState.PRECISION, true)
                view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                UiSounds.tick()
                // hasta levantar el dedo: sin umbral de arrastre ni límites
                drag(down.id) { it.consume() }
            } finally {
                onActive(false)
                ButtonState.set(ButtonState.PRECISION, false)
            }
        }
    }

/** Mirilla de francotirador: aro, cuatro marcas que lo cruzan y punto central. */
@Composable
private fun CrosshairGlyph(color: Color, size: Dp) {
    Canvas(Modifier.size(size)) {
        val px = size.toPx()
        val c = Offset(px / 2f, px / 2f)
        val stroke = px * 0.11f
        drawCircle(color, px * 0.30f, c, style = Stroke(width = stroke))
        // marcas N/S/E/O: nacen dentro del aro y lo cruzan hacia fuera
        val inner = px * 0.19f
        val outer = px * 0.47f
        for ((dx, dy) in listOf(0f to -1f, 0f to 1f, -1f to 0f, 1f to 0f)) {
            drawLine(
                color,
                Offset(c.x + dx * inner, c.y + dy * inner),
                Offset(c.x + dx * outer, c.y + dy * outer),
                strokeWidth = stroke,
                cap = StrokeCap.Round
            )
        }
        drawCircle(color, px * 0.06f, c)
    }
}
