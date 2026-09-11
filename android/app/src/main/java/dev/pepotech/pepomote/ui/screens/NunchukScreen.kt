package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.service.LinkForegroundService
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.AnalogStick
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.components.TriggerZone
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Nunchuk: el móvil de la otra mano. Stick analógico, C encima y el gatillo Z
 * abajo (como el B del mando). Sin selector de modo ni puntero: eso lo decide
 * el móvil-Wiimote. Mismo layout vertical y de lado, solo escalado.
 */
@Composable
fun NunchukScreen(link: UiLink, onDisconnect: () -> Unit) {
    val view = LocalView.current

    DisposableEffect(Unit) {
        view.keepScreenOn = true
        onDispose {
            view.keepScreenOn = false
            ButtonState.setStick(0, 0) // nada queda inclinado al salir
        }
    }

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .navigationBarsPadding()
    ) {
        // De lado hay poca altura: todo más pequeño, mismo orden
        val compact = maxHeight < 520.dp
        val stickSize = if (compact) maxHeight * 0.40f else minOf(maxWidth * 0.66f, 264.dp)
        val cSize = if (compact) 56.dp else 76.dp
        val zHeight = if (compact) 64.dp else 96.dp

        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Spacer(Modifier.height(10.dp))
            Header(link, onDisconnect)

            Spacer(Modifier.weight(1f))
            RoundButton("C", cSize, ButtonState.C, textSize = if (compact) 20 else 26)
            Spacer(Modifier.height(if (compact) 10.dp else 22.dp))
            AnalogStick(stickSize) { x, y -> ButtonState.setStick(x, y) }
            Spacer(Modifier.weight(1f))

            TriggerZone(bit = ButtonState.Z, label = "Z", height = zHeight)
            Spacer(Modifier.height(12.dp))
        }
    }
}

/** Cabecera: PC · "Nunchuk · Jugador N" (+ RTT) / Conectando… / Sin conexión, y Salir. */
@Composable
private fun Header(link: UiLink, onDisconnect: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(Modifier.weight(1f)) {
            when (link) {
                is UiLink.Connected -> {
                    Text(link.pcName, style = MaterialTheme.typography.titleMedium)
                    Text(
                        buildString {
                            append("Nunchuk · Jugador ${link.player}")
                            link.rttMs?.let { append(" · ${"%.0f".format(it)} ms") }
                        },
                        style = MaterialTheme.typography.bodyMedium
                    )
                }

                is UiLink.Connecting -> Text("Conectando…", style = MaterialTheme.typography.titleMedium)
                else -> {
                    Text("Sin conexión", style = MaterialTheme.typography.titleMedium)
                    val ctx = LocalContext.current
                    if (PairStore.load(ctx) != null) {
                        TextButton(onClick = {
                            LinkForegroundService.start(ctx, LinkState.ROLE_NUNCHUK)
                        }) {
                            Text("Reconectar", color = PepoColors.Blue)
                        }
                    }
                }
            }
        }
        TextButton(onClick = onDisconnect) {
            Text("Salir", color = PepoColors.Error)
        }
    }
}
