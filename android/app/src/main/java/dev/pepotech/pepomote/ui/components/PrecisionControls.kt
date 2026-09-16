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
fun PrecisionStrip(modifier: Modifier, glyph: Dp = 16.dp) {
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
        CrosshairGlyph(PepoColors.TextDim, glyph)
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
 * Acercar (PROTOCOL.md §4.2, bit 30): mientras se mantiene, el receptor
 * acerca el Mando de Wii emulado a la pantalla (juegos de Wii que piden
 * acercar el mando, como los microjuegos de WarioWare). Ocupa el sitio de la
 * precisión en modo Dolphin, donde esta no hace nada. Mismo gesto: mantener,
 * aunque el dedo se salga.
 */
@Composable
fun NearStrip(modifier: Modifier, glyph: Dp = 16.dp) {
    val view = LocalView.current
    var active by remember { mutableStateOf(false) }
    Box(
        modifier = modifier
            .padding(start = 6.dp)
            .background(
                if (active) PepoColors.Glow else PepoColors.CardBorder,
                RoundedCornerShape(15.dp)
            )
            .holdBit(view, ButtonState.NEAR) { active = it },
        contentAlignment = Alignment.Center
    ) {
        NearGlyph(PepoColors.TextDim, glyph)
    }
}

/** Píldora «Acercar» de los mandos apaisados en Dolphin (mismo gesto: mantener). */
@Composable
fun NearPill(modifier: Modifier) {
    val view = LocalView.current
    var active by remember { mutableStateOf(false) }
    Row(
        modifier = modifier
            .height(34.dp)
            .background(
                if (active) PepoColors.Glow else PepoColors.CardBorder,
                RoundedCornerShape(17.dp)
            )
            .holdBit(view, ButtonState.NEAR) { active = it }
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        NearGlyph(PepoColors.TextDim, 14.dp)
        Spacer(Modifier.width(8.dp))
        Text(stringResource(R.string.near), style = MaterialTheme.typography.bodyMedium)
    }
}

/** Mantener = bit de precisión (ver `holdBit`). */
private fun Modifier.precisionHold(view: View, onActive: (Boolean) -> Unit): Modifier =
    holdBit(view, ButtonState.PRECISION, onActive)

/**
 * Mantener = un bit sostenido (precisión, acercar); háptico y tic al
 * activar. El gesto sigue al dedo hasta que lo levanta, aunque se salga de
 * la tira (como la de scroll); el `finally` suelta el bit también si el
 * gesto se cancela (ACTION_CANCEL, cambio de pantalla): sin él el puntero se
 * quedaría al 40 % (o el mando pegado a la pantalla) para siempre. Clave
 * `bit`: si la tira cambia de bit al cambiar de modo, el gesto se rehace.
 */
private fun Modifier.holdBit(view: View, bit: Int, onActive: (Boolean) -> Unit): Modifier =
    pointerInput(bit) {
        awaitEachGesture {
            val down = awaitFirstDown()
            down.consume()
            try {
                onActive(true)
                ButtonState.set(bit, true)
                view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                UiSounds.tick()
                // hasta levantar el dedo: sin umbral de arrastre ni límites
                drag(down.id) { it.consume() }
            } finally {
                onActive(false)
                ButtonState.set(bit, false)
            }
        }
    }

/** Pantalla (barra arriba) y flecha que se le acerca: el glifo de «Acercar». */
@Composable
private fun NearGlyph(color: Color, size: Dp) {
    Canvas(Modifier.size(size)) {
        val px = size.toPx()
        val stroke = px * 0.11f
        val cx = px / 2f
        // la pantalla: barra horizontal arriba
        drawLine(color, Offset(px * 0.15f, px * 0.14f), Offset(px * 0.85f, px * 0.14f), strokeWidth = stroke, cap = StrokeCap.Round)
        // la flecha: asta y punta hacia la barra
        drawLine(color, Offset(cx, px * 0.90f), Offset(cx, px * 0.34f), strokeWidth = stroke, cap = StrokeCap.Round)
        drawLine(color, Offset(cx - px * 0.22f, px * 0.54f), Offset(cx, px * 0.32f), strokeWidth = stroke, cap = StrokeCap.Round)
        drawLine(color, Offset(cx + px * 0.22f, px * 0.54f), Offset(cx, px * 0.32f), strokeWidth = stroke, cap = StrokeCap.Round)
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
