package dev.pepotech.pepomote.ui.components

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.theme.PepoColors
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

/** Punto que late (algo está en marcha: conectando, reconectando…). */
@Composable
fun PulsingDot(color: Color, size: Dp = 10.dp) {
    val transition = rememberInfiniteTransition(label = "pulse")
    val alpha by transition.animateFloat(
        initialValue = 0.35f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(700, easing = FastOutSlowInEasing), RepeatMode.Reverse),
        label = "alpha"
    )
    Box(
        Modifier
            .size(size)
            .alpha(alpha)
            .background(color, CircleShape)
    )
}

/** «Reconectando con X…» con su punto latiendo, para las cabeceras del mando. */
@Composable
fun ReconnectingLabel(link: UiLink.Reconnecting, style: TextStyle = MaterialTheme.typography.titleMedium) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        PulsingDot(PepoColors.Warn)
        Spacer(Modifier.width(8.dp))
        Text(
            stringResource(R.string.status_reconnecting, link.pcName),
            style = style,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
    }
}
