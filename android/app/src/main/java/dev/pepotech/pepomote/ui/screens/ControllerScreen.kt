package dev.pepotech.pepomote.ui.screens

import android.view.HapticFeedbackConstants
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.components.TriggerZone
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

/** Mando vertical estilo Wiimote: cruceta, −/diana/+, A, 1/2, multimedia, B. */
@Composable
fun ControllerScreen(link: UiLink, onDisconnect: () -> Unit) {
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
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Spacer(Modifier.height(10.dp))

            // Cabecera: estado + desconectar
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
                                    append(if (link.mode == "dolphin") "Dolphin" else "Puntero")
                                    link.rttMs?.let { append(" · ${"%.0f".format(it)} ms") }
                                },
                                style = MaterialTheme.typography.bodyMedium
                            )
                        }

                        is UiLink.Connecting -> Text("Conectando…", style = MaterialTheme.typography.titleMedium)
                        else -> Text("Sin conexión", style = MaterialTheme.typography.titleMedium)
                    }
                }
                TextButton(onClick = onDisconnect) {
                    Text("Salir", color = PepoColors.Error)
                }
            }

            Spacer(Modifier.height(12.dp))
            PadCross(sizeDp = 168.dp)

            Spacer(Modifier.height(16.dp))
            Row(
                horizontalArrangement = Arrangement.spacedBy(20.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                RoundButton("−", 54.dp, ButtonState.MINUS)
                RecenterButton()
                RoundButton("+", 54.dp, ButtonState.PLUS)
            }

            Spacer(Modifier.height(16.dp))
            RoundButton(
                "A", 148.dp, ButtonState.A,
                background = PepoColors.Blue,
                pressedColor = PepoColors.BlueHover,
                textColor = PepoColors.Card,
                textSize = 44
            )

            Spacer(Modifier.height(14.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(20.dp)) {
                RoundButton("1", 52.dp, ButtonState.ONE, textSize = 18)
                RoundButton("2", 52.dp, ButtonState.TWO, textSize = 18)
            }

            Spacer(Modifier.height(10.dp))
            MediaRow()

            Spacer(Modifier.weight(1f))
            TriggerZone()
            Spacer(Modifier.height(12.dp))
        }

        ScrollStrip(
            modifier = Modifier
                .align(Alignment.CenterEnd)
                .fillMaxHeight(0.45f)
                .width(30.dp)
        )
    }
}

/** Diana de recentrado: mantener 150 ms → vibra y recentra. */
@Composable
private fun RecenterButton() {
    val view = LocalView.current
    var down by remember { mutableStateOf(false) }

    Box(
        modifier = Modifier
            .size(64.dp)
            .background(if (down) PepoColors.Glow else PepoColors.Card, CircleShape)
            .pointerInput(Unit) {
                detectTapGestures(onPress = {
                    down = true
                    coroutineScope {
                        val job = launch {
                            delay(150)
                            ButtonState.bumpRecenter()
                            view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                        }
                        tryAwaitRelease()
                        job.cancel()
                    }
                    down = false
                })
            },
        contentAlignment = Alignment.Center
    ) {
        Box(
            Modifier
                .size(26.dp)
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
}

/** Fila multimedia plegable. */
@Composable
private fun MediaRow() {
    var expanded by remember { mutableStateOf(false) }

    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        TextButton(onClick = { expanded = !expanded }) {
            Text(
                if (expanded) "Multimedia ▲" else "Multimedia ▼",
                color = PepoColors.TextDim,
                style = MaterialTheme.typography.bodyMedium
            )
        }
        if (expanded) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                RoundButton("⏮", 46.dp, ButtonState.MEDIA_PREV, textSize = 16)
                RoundButton("🔉", 46.dp, ButtonState.MEDIA_VOL_DOWN, textSize = 16)
                RoundButton("⏯", 46.dp, ButtonState.MEDIA_PLAY_PAUSE, textSize = 16)
                RoundButton("🔇", 46.dp, ButtonState.MEDIA_MUTE, textSize = 16)
                RoundButton("🔊", 46.dp, ButtonState.MEDIA_VOL_UP, textSize = 16)
                RoundButton("⏭", 46.dp, ButtonState.MEDIA_NEXT, textSize = 16)
            }
        }
    }
}

/** Tira de scroll del borde derecho: arrastra para hacer scroll en el PC. */
@Composable
private fun ScrollStrip(modifier: Modifier) {
    var active by remember { mutableStateOf(false) }

    Box(
        modifier = modifier
            .padding(end = 6.dp)
            .background(
                if (active) PepoColors.Glow else PepoColors.CardBorder,
                RoundedCornerShape(15.dp)
            )
            .pointerInput(Unit) {
                detectVerticalDragGestures(
                    onDragStart = { active = true },
                    onDragEnd = { active = false },
                    onDragCancel = { active = false }
                ) { _, dragAmount ->
                    // dedo hacia arriba (dragAmount negativo) = scroll up = positivo
                    ButtonState.addScroll((-dragAmount).roundToInt())
                }
            },
        contentAlignment = Alignment.Center
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            repeat(3) {
                Box(
                    Modifier
                        .size(6.dp)
                        .background(PepoColors.TextDim, CircleShape)
                )
            }
        }
    }
}
