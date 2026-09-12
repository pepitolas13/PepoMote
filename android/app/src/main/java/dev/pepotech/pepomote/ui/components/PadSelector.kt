package dev.pepotech.pepomote.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlinx.coroutines.delay
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

/** Sin eco de `pad` en este tiempo (PC antiguo), se vuelve a marcar el mando actual. */
private const val PAD_ECHO_TIMEOUT_MS = 2000L

/**
 * «En Cemu soy: [ GamePad ] [ Mando de Wii ]»: qué mando es este móvil
 * dentro del modo Wii U (nada que ver con el modo, que va en sus chips).
 * Segmento activo relleno en azul; el pedido y aún sin eco, a medio tono
 * ("pendiente"). Jugadores 2-4: «Pro Controller» en vez de «GamePad».
 * Tocar envía `pad wiimote` / `pad gamepad`; la pantalla cambia con el eco.
 *
 * Se adapta al ancho: los dos segmentos se reparten el sitio a partes
 * iguales y su texto se encoge hasta caber (nunca se corta); en pantallas
 * estrechas «En Cemu soy:» pasa a una línea encima.
 */
@Composable
fun PadSelector(link: UiLink.Connected, compact: Boolean = false, help: String? = null) {
    val wiimote = link.pad == LinkState.PAD_WIIMOTE
    var pending by remember { mutableStateOf<String?>(null) }

    // El eco de `pad` (cambie o no el mando) cierra la espera…
    LaunchedEffect(link.pad) { pending = null }
    // …y si no llega, se deja de esperar
    LaunchedEffect(pending) {
        if (pending != null) {
            delay(PAD_ECHO_TIMEOUT_MS)
            pending = null
        }
    }

    BoxWithConstraints(modifier = Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        val inlinePrefix = maxWidth >= 430.dp
        val prefixW = if (inlinePrefix) 110.dp else 0.dp
        val segmentsW = (maxWidth - prefixW - 24.dp).coerceIn(160.dp, if (compact) 330.dp else 360.dp)
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            if (!inlinePrefix) {
                Text(
                    stringResource(R.string.in_cemu),
                    style = MaterialTheme.typography.bodyMedium.copy(color = PepoColors.TextDim, fontSize = 11.sp),
                    maxLines = 1
                )
            }
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 10.dp)
            ) {
                if (inlinePrefix) {
                    Text(
                        stringResource(R.string.in_cemu),
                        style = MaterialTheme.typography.bodyMedium.copy(color = PepoColors.Text),
                        maxLines = 1
                    )
                }
                val shape = RoundedCornerShape(20.dp)
                Row(
                    modifier = Modifier
                        .width(segmentsW)
                        .background(PepoColors.Card, shape)
                        .border(1.5.dp, PepoColors.CardBorder, shape)
                        .padding(3.dp),
                    horizontalArrangement = Arrangement.spacedBy(3.dp)
                ) {
                    Segment(
                        label = if (link.player == 1) stringResource(R.string.gamepad) else stringResource(R.string.pro_controller),
                        selected = !wiimote,
                        pending = pending == LinkState.PAD_GAMEPAD,
                        compact = compact
                    ) {
                        if (wiimote) {
                            pending = LinkState.PAD_GAMEPAD
                            LinkState.sendPad?.invoke(LinkState.PAD_GAMEPAD)
                        }
                    }
                    Segment(
                        label = stringResource(R.string.wii_remote),
                        selected = wiimote,
                        pending = pending == LinkState.PAD_WIIMOTE,
                        compact = compact
                    ) {
                        if (!wiimote) {
                            pending = LinkState.PAD_WIIMOTE
                            LinkState.sendPad?.invoke(LinkState.PAD_WIIMOTE)
                        }
                    }
                }
            }
            if (help != null) {
                Text(
                    help,
                    style = MaterialTheme.typography.bodyMedium,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(top = 4.dp, start = 12.dp, end = 12.dp)
                )
            }
        }
    }
}

@Composable
private fun RowScope.Segment(label: String, selected: Boolean, pending: Boolean, compact: Boolean, onClick: () -> Unit) {
    val bg = when {
        selected -> PepoColors.Blue
        pending -> PepoColors.Glow
        else -> PepoColors.Card
    }
    val fg = when {
        selected -> PepoColors.OnAccent
        pending -> PepoColors.Text
        else -> PepoColors.TextDim
    }
    Box(
        modifier = Modifier
            .weight(1f)
            .background(bg, RoundedCornerShape(17.dp))
            .pointerInput(label) {
                detectTapGestures(onTap = { onClick() })
            }
            .padding(
                horizontal = if (compact) 8.dp else 12.dp,
                vertical = if (compact) 6.dp else 8.dp
            ),
        contentAlignment = Alignment.Center
    ) {
        FitText(
            label,
            style = MaterialTheme.typography.labelLarge.copy(color = fg),
            maxSize = MaterialTheme.typography.labelLarge.fontSize,
            minSize = 9.sp
        )
    }
}
