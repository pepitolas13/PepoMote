package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Mando apaisado estilo "de lado" (NES): cruceta a la izquierda, 1 y 2
 * grandes a la derecha. Para juegos 2D en Dolphin con el Wiimote de lado.
 */
@Composable
fun ControllerLandscapeScreen(link: UiLink, onDisconnect: () -> Unit) {
    val view = LocalView.current
    DisposableEffect(Unit) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .navigationBarsPadding()
    ) {
        // Cabecera compacta
        Row(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp)
        ) {
            Text(
                when (link) {
                    is UiLink.Connected -> link.pcName
                    is UiLink.Connecting -> "Conectando…"
                    else -> "Sin conexión"
                },
                style = MaterialTheme.typography.bodyMedium
            )
            TextButton(onClick = onDisconnect) {
                Text("Salir", color = PepoColors.Error, style = MaterialTheme.typography.bodyMedium)
            }
        }

        // Cruceta izquierda
        Box(
            modifier = Modifier
                .align(Alignment.CenterStart)
                .padding(start = 34.dp)
        ) {
            PadCross(sizeDp = 190.dp)
        }

        // − / + centro
        Column(
            modifier = Modifier.align(Alignment.Center),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Spacer(Modifier.height(20.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                RoundButton("−", 44.dp, ButtonState.MINUS, textSize = 16)
                RoundButton("+", 44.dp, ButtonState.PLUS, textSize = 16)
            }
            RoundButton("A", 52.dp, ButtonState.A, textSize = 18)
        }

        // 1 y 2 grandes a la derecha (los botones de acción del modo NES)
        Row(
            modifier = Modifier
                .align(Alignment.CenterEnd)
                .padding(end = 30.dp),
            horizontalArrangement = Arrangement.spacedBy(18.dp)
        ) {
            RoundButton(
                "1", 92.dp, ButtonState.ONE,
                background = PepoColors.Blue,
                pressedColor = PepoColors.BlueHover,
                textColor = PepoColors.Card,
                textSize = 28
            )
            RoundButton(
                "2", 92.dp, ButtonState.TWO,
                background = PepoColors.Blue,
                pressedColor = PepoColors.BlueHover,
                textColor = PepoColors.Card,
                textSize = 28,
                pop = true
            )
        }

        Text(
            "Gira el móvil para volver al mando vertical",
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = 8.dp)
        )
    }
}
