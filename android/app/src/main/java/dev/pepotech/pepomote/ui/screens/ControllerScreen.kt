package dev.pepotech.pepomote.ui.screens

import android.view.HapticFeedbackConstants
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Mando h1: estado + botón A gigante + recentrar + desconectar.
 * El movimiento lo ponen los sensores; esta pantalla solo botones.
 * h2/h4 construyen el layout Wiimote completo.
 */
@Composable
fun ControllerScreen(link: UiLink, onDisconnect: () -> Unit) {
    val view = LocalView.current

    DisposableEffect(Unit) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .navigationBarsPadding(),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Spacer(Modifier.height(20.dp))
        when (link) {
            is UiLink.Connected -> {
                Text(link.pcName, style = MaterialTheme.typography.titleLarge)
                Text(
                    buildString {
                        append(if (link.mode == "dolphin") "Modo Dolphin" else "Modo puntero")
                        link.rttMs?.let { append("  ·  ${"%.0f".format(it)} ms") }
                    },
                    style = MaterialTheme.typography.bodyMedium
                )
            }

            is UiLink.Connecting -> Text("Conectando…", style = MaterialTheme.typography.titleLarge)
            else -> Text("Sin conexión", style = MaterialTheme.typography.titleLarge)
        }

        Spacer(Modifier.weight(1f))

        // Botón A — el corazón del mando
        var aDown by remember { mutableStateOf(false) }
        Box(
            modifier = Modifier
                .size(180.dp)
                .shadow(if (aDown) 2.dp else 10.dp, CircleShape)
                .background(if (aDown) PepoColors.BlueHover else PepoColors.Blue, CircleShape)
                .pointerInput(Unit) {
                    detectTapGestures(
                        onPress = {
                            aDown = true
                            ButtonState.set(ButtonState.A, true)
                            view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                            tryAwaitRelease()
                            aDown = false
                            ButtonState.set(ButtonState.A, false)
                        }
                    )
                },
            contentAlignment = Alignment.Center
        ) {
            Text(
                "A",
                style = MaterialTheme.typography.displayLarge.copy(color = PepoColors.Card)
            )
        }

        Spacer(Modifier.height(30.dp))

        // Recentrar (diana)
        Row(
            horizontalArrangement = Arrangement.spacedBy(18.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Box(
                modifier = Modifier
                    .size(72.dp)
                    .background(PepoColors.Card, CircleShape)
                    .pointerInput(Unit) {
                        detectTapGestures(onTap = {
                            ButtonState.bumpRecenter()
                            view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                        })
                    },
                contentAlignment = Alignment.Center
            ) {
                Box(
                    Modifier
                        .size(28.dp)
                        .background(PepoColors.Background, CircleShape),
                    contentAlignment = Alignment.Center
                ) {
                    Box(
                        Modifier
                            .size(10.dp)
                            .background(PepoColors.Blue, CircleShape)
                    )
                }
            }
            Text("Recentrar", style = MaterialTheme.typography.bodyMedium)
        }

        Spacer(Modifier.weight(1f))

        TextButton(onClick = onDisconnect, modifier = Modifier.fillMaxWidth()) {
            Text("Desconectar", color = PepoColors.Error)
        }
        Spacer(Modifier.height(12.dp))
    }
}
