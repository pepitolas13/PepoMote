package dev.pepotech.pepomote.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.ui.theme.PepoColors

// Glifos propios, dibujados a mano — nada de iconografía ajena.
enum class ChannelGlyph { Qr, Pad, Pointer, Stick, GamePad, Switch, Gear, Server, Retro }

/** `wide`: tarjeta apaisada a todo el ancho (glifo a la izquierda, textos al lado). */
@Composable
fun ChannelCard(
    title: String,
    subtitle: String,
    glyph: ChannelGlyph,
    accent: Color = PepoColors.Blue,
    wide: Boolean = false,
    onClick: () -> Unit = {}
) {
    Card(
        onClick = onClick,
        modifier = if (wide) Modifier.fillMaxWidth() else Modifier.fillMaxWidth().aspectRatio(1f),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
        border = BorderStroke(1.5.dp, PepoColors.CardBorder),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
    ) {
        if (wide) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 18.dp, vertical = 16.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Canvas(modifier = Modifier.size(40.dp)) { drawGlyph(glyph, accent) }
                Spacer(Modifier.width(16.dp))
                Column {
                    Text(title, style = MaterialTheme.typography.titleMedium)
                    Text(subtitle, style = MaterialTheme.typography.bodyMedium, maxLines = 1)
                }
            }
        } else {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(18.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center
            ) {
                Canvas(modifier = Modifier.size(52.dp).padding(bottom = 4.dp)) {
                    drawGlyph(glyph, accent)
                }
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(
                    subtitle,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 2
                )
            }
        }
    }
}

private fun DrawScope.drawGlyph(glyph: ChannelGlyph, accent: Color) {
    when (glyph) {
        ChannelGlyph.Qr -> drawQrGlyph(accent)
        ChannelGlyph.Pad -> drawPadGlyph(accent)
        ChannelGlyph.Pointer -> drawPointerGlyph(accent)
        ChannelGlyph.Stick -> drawStickGlyph(accent)
        ChannelGlyph.GamePad -> drawGamePadGlyph(accent)
        ChannelGlyph.Switch -> drawSwitchGlyph(accent)
        ChannelGlyph.Gear -> drawGearGlyph(accent)
        ChannelGlyph.Server -> drawServerGlyph(accent)
        ChannelGlyph.Retro -> drawRetroGlyph(accent)
    }
}

/** Mando clásico apaisado: cuerpo, cruceta a la izquierda y dos botones a la derecha. */
private fun DrawScope.drawRetroGlyph(accent: Color) {
    val w = size.width
    val h = size.height
    drawRoundRect(accent, androidx.compose.ui.geometry.Offset(w * .04f, h * .28f),
        androidx.compose.ui.geometry.Size(w * .92f, h * .44f),
        androidx.compose.ui.geometry.CornerRadius(w * .08f), style = Stroke(w * .045f))
    // cruceta
    drawLine(accent, androidx.compose.ui.geometry.Offset(w * .27f, h * .38f),
        androidx.compose.ui.geometry.Offset(w * .27f, h * .62f), w * .07f, StrokeCap.Round)
    drawLine(accent, androidx.compose.ui.geometry.Offset(w * .15f, h * .50f),
        androidx.compose.ui.geometry.Offset(w * .39f, h * .50f), w * .07f, StrokeCap.Round)
    // B y A
    drawCircle(accent, w * .05f, androidx.compose.ui.geometry.Offset(w * .66f, h * .55f))
    drawCircle(accent, w * .05f, androidx.compose.ui.geometry.Offset(w * .82f, h * .45f))
}

private fun DrawScope.drawServerGlyph(accent: Color) {
    val w = size.width
    val h = size.height
    drawRoundRect(accent, androidx.compose.ui.geometry.Offset(w * .25f, h * .04f),
        androidx.compose.ui.geometry.Size(w * .5f, h * .92f),
        androidx.compose.ui.geometry.CornerRadius(w * .08f), style = Stroke(w * .045f))
    for (y in listOf(.30f, .46f, .62f)) {
        drawLine(accent, androidx.compose.ui.geometry.Offset(w * .38f, h * y),
            androidx.compose.ui.geometry.Offset(w * .62f, h * y), w * .045f, StrokeCap.Round)
    }
    drawCircle(accent, w * .03f, androidx.compose.ui.geometry.Offset(w * .5f, h * .84f))
}

private fun DrawScope.drawSwitchGlyph(accent: Color) {
    val w = size.width
    val h = size.height
    for (x in listOf(0.16f, 0.56f)) {
        drawRoundRect(accent, androidx.compose.ui.geometry.Offset(w * x, h * 0.08f),
            androidx.compose.ui.geometry.Size(w * 0.28f, h * 0.84f),
            androidx.compose.ui.geometry.CornerRadius(w * 0.1f), style = Stroke(w * 0.04f))
    }
    drawCircle(accent, w * 0.065f, androidx.compose.ui.geometry.Offset(w * 0.30f, h * 0.32f))
    drawCircle(accent, w * 0.065f, androidx.compose.ui.geometry.Offset(w * 0.70f, h * 0.68f))
}

private fun DrawScope.drawQrGlyph(accent: Color) {
    val cell = size.width / 5f
    val positions = listOf(
        0 to 0, 1 to 0, 3 to 0, 4 to 0,
        0 to 1, 4 to 1,
        2 to 2,
        0 to 3, 3 to 3,
        0 to 4, 1 to 4, 4 to 4
    )
    positions.forEach { (x, y) ->
        drawRoundRect(
            color = accent,
            topLeft = androidx.compose.ui.geometry.Offset(x * cell + cell * 0.1f, y * cell + cell * 0.1f),
            size = androidx.compose.ui.geometry.Size(cell * 0.8f, cell * 0.8f),
            cornerRadius = androidx.compose.ui.geometry.CornerRadius(cell * 0.2f)
        )
    }
}

private fun DrawScope.drawPadGlyph(accent: Color) {
    val w = size.width
    val arm = w * 0.3f
    val thick = w * 0.28f
    val r = androidx.compose.ui.geometry.CornerRadius(thick * 0.35f)
    // Cruceta: barra vertical + barra horizontal
    drawRoundRect(
        color = accent,
        topLeft = androidx.compose.ui.geometry.Offset((w - thick) / 2f, (w - (arm * 2 + thick)) / 2f),
        size = androidx.compose.ui.geometry.Size(thick, arm * 2 + thick),
        cornerRadius = r
    )
    drawRoundRect(
        color = accent,
        topLeft = androidx.compose.ui.geometry.Offset((w - (arm * 2 + thick)) / 2f, (w - thick) / 2f),
        size = androidx.compose.ui.geometry.Size(arm * 2 + thick, thick),
        cornerRadius = r
    )
}

private fun DrawScope.drawPointerGlyph(accent: Color) {
    val c = androidx.compose.ui.geometry.Offset(size.width / 2f, size.height / 2f)
    drawCircle(color = accent, radius = size.width * 0.42f, center = c, style = Stroke(width = size.width * 0.10f))
    drawCircle(color = accent, radius = size.width * 0.12f, center = c)
}

/** Stick empujado arriba-derecha: aro, vástago y pomo. */
private fun DrawScope.drawStickGlyph(accent: Color) {
    val w = size.width
    val c = androidx.compose.ui.geometry.Offset(w / 2f, size.height / 2f)
    drawCircle(color = accent, radius = w * 0.42f, center = c, style = Stroke(width = w * 0.07f))
    val knob = androidx.compose.ui.geometry.Offset(c.x + w * 0.15f, c.y - w * 0.15f)
    drawLine(color = accent, start = c, end = knob, strokeWidth = w * 0.11f, cap = StrokeCap.Round)
    drawCircle(color = accent, radius = w * 0.17f, center = knob)
}

/** GamePad de Wii U: cuerpo apaisado con la pantalla en medio y un stick a cada lado. */
private fun DrawScope.drawGamePadGlyph(accent: Color) {
    val w = size.width
    val stroke = w * 0.08f
    val bodyH = w * 0.64f
    val top = (size.height - bodyH) / 2f
    drawRoundRect(
        color = accent,
        topLeft = androidx.compose.ui.geometry.Offset(stroke / 2f, top + stroke / 2f),
        size = androidx.compose.ui.geometry.Size(w - stroke, bodyH - stroke),
        cornerRadius = androidx.compose.ui.geometry.CornerRadius(w * 0.14f),
        style = Stroke(width = stroke)
    )
    // Pantalla táctil
    drawRoundRect(
        color = accent,
        topLeft = androidx.compose.ui.geometry.Offset(w * 0.31f, top + bodyH * 0.24f),
        size = androidx.compose.ui.geometry.Size(w * 0.38f, bodyH * 0.52f),
        cornerRadius = androidx.compose.ui.geometry.CornerRadius(w * 0.04f)
    )
    // Sticks
    drawCircle(color = accent, radius = w * 0.075f, center = androidx.compose.ui.geometry.Offset(w * 0.17f, top + bodyH * 0.40f))
    drawCircle(color = accent, radius = w * 0.075f, center = androidx.compose.ui.geometry.Offset(w * 0.83f, top + bodyH * 0.40f))
    // Cruceta y botones, insinuados
    drawCircle(color = accent, radius = w * 0.04f, center = androidx.compose.ui.geometry.Offset(w * 0.17f, top + bodyH * 0.72f))
    drawCircle(color = accent, radius = w * 0.04f, center = androidx.compose.ui.geometry.Offset(w * 0.83f, top + bodyH * 0.72f))
}

private fun DrawScope.drawGearGlyph(accent: Color) {
    val c = androidx.compose.ui.geometry.Offset(size.width / 2f, size.height / 2f)
    val rOut = size.width * 0.42f
    val rIn = size.width * 0.16f
    // Dientes: 8 rectángulos rotados
    for (i in 0 until 8) {
        rotate(degrees = i * 45f, pivot = c) {
            drawRoundRect(
                color = accent,
                topLeft = androidx.compose.ui.geometry.Offset(c.x - size.width * 0.06f, c.y - rOut),
                size = androidx.compose.ui.geometry.Size(size.width * 0.12f, size.width * 0.20f),
                cornerRadius = androidx.compose.ui.geometry.CornerRadius(size.width * 0.03f)
            )
        }
    }
    drawCircle(color = accent, radius = rOut * 0.72f, center = c, style = Stroke(width = size.width * 0.10f))
    drawCircle(color = accent, radius = rIn, center = c)
}

private fun DrawScope.rotate(degrees: Float, pivot: androidx.compose.ui.geometry.Offset, block: DrawScope.() -> Unit) {
    drawContext.transform.rotate(degrees, pivot)
    block()
    drawContext.transform.rotate(-degrees, pivot)
}
