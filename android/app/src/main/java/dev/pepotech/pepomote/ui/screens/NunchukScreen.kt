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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.service.LinkForegroundService
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.AnalogStick
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.TriggerZone
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Nunchuk: el móvil de la otra mano, en vertical o de lado (mismo orden,
 * solo escalado). Stick centrado para el pulgar de cualquier mano; C es una
 * banda ancha justo encima del stick y Z la banda inferior (como el gatillo B
 * del mando): las dos se aciertan sin mirar. Sin selector de modo ni puntero:
 * eso lo decide el móvil-Wiimote; el modo real (le llega por difusión) y el
 * jugador van en la cabecera. En Wii U, si el mando de su jugador no es Mando
 * de Wii, el Nunchuk no tiene a quién acompañar y lo avisa.
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
        val cHeight = if (compact) 48.dp else 64.dp
        val zHeight = if (compact) 64.dp else 96.dp

        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Spacer(Modifier.height(10.dp))
            Header(link, onDisconnect)

            // Aviso permanente: en Wii U el Nunchuk solo acompaña a un Mando de Wii
            if (link is UiLink.Connected && link.mode == LinkState.MODE_CEMU &&
                link.pad != LinkState.PAD_WIIMOTE
            ) {
                Spacer(Modifier.height(6.dp))
                Text(
                    "En Wii U el Nunchuk solo funciona si el mando elige Mando de Wii",
                    style = MaterialTheme.typography.bodyMedium.copy(color = PepoColors.Warn),
                    textAlign = TextAlign.Center
                )
            }

            Spacer(Modifier.weight(1f))
            // C: banda ancha encima del stick, se acierta con el pulgar sin mirar
            TriggerZone(
                bit = ButtonState.C, label = "C", height = cHeight,
                background = PepoColors.CardBorder,
                pressedColor = PepoColors.Glow,
                textColor = PepoColors.Text
            )
            Spacer(Modifier.height(if (compact) 10.dp else 18.dp))
            AnalogStick(stickSize) { x, y -> ButtonState.setStick(x, y) }
            Spacer(Modifier.weight(1f))

            // Z: banda inferior, como el gatillo B del mando
            TriggerZone(bit = ButtonState.Z, label = "Z", height = zHeight)
            Spacer(Modifier.height(12.dp))
        }

        NoticeBanner(
            Modifier
                .align(Alignment.TopCenter)
                .padding(top = 64.dp, start = 24.dp, end = 24.dp)
        )
    }
}

/** Cabecera: PC · "Nunchuk · Jugador N · modo" (+ RTT) / Conectando… / Sin conexión, y Salir. */
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
                            append("Nunchuk · Jugador ${link.player} · ${modeLabel(link.mode)}")
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
