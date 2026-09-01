package dev.pepotech.pepomote.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.ui.theme.PepoColors

// Glifos propios, dibujados a mano — nada de iconografía ajena.
enum class ChannelGlyph { Qr, Pad, Pointer, Gear }

@Composable
fun ChannelCard(
    title: String,
    subtitle: String,
    glyph: ChannelGlyph,
    accent: Color = PepoColors.Blue,
    onClick: () -> Unit = {}
) {
    Card(
        onClick = onClick,
        modifier = Modifier
            .fillMaxWidth()
            .aspectRatio(1f),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
        border = BorderStroke(1.5.dp, PepoColors.CardBorder),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(18.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center
        ) {
            Canvas(modifier = Modifier.size(52.dp).padding(bottom = 4.dp)) {
                when (glyph) {
                    ChannelGlyph.Qr -> drawQrGlyph(accent)
                    ChannelGlyph.Pad -> drawPadGlyph(accent)
                    ChannelGlyph.Pointer -> drawPointerGlyph(accent)
                    ChannelGlyph.Gear -> drawGearGlyph(accent)
                }
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
