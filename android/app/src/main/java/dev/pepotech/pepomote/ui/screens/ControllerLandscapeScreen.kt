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
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.KeyboardButton
import dev.pepotech.pepomote.ui.components.KeyboardDialog
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.PadSelector
import dev.pepotech.pepomote.ui.components.PrecisionPill
import dev.pepotech.pepomote.ui.components.ReconnectingLabel
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Mando apaisado estilo "de lado" (NES): cruceta a la izquierda, 1 y 2
 * grandes a la derecha. Para juegos 2D en Dolphin con el Wiimote de lado.
 * `showChips`: selector Puntero/Dolphin/Wii U, mismas condiciones que en
 * vertical. Dentro de Wii U como Mando de Wii: cabecera «Wii U · Mando de
 * Wii» con «Teclado» (texto para el teclado en pantalla de Cemu) y el
 * selector «En Cemu soy» debajo.
 */
@Composable
fun ControllerLandscapeScreen(link: UiLink, showChips: Boolean, onDisconnect: () -> Unit) {
    val view = LocalView.current
    var keyboardOpen by remember { mutableStateOf(false) }
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
        // Cabecera compacta (+ selector de mando dentro de Wii U)
        Column(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = 6.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(2.dp)
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(14.dp)
            ) {
                if (link is UiLink.Reconnecting) {
                    ReconnectingLabel(link, MaterialTheme.typography.bodyMedium)
                } else {
                    Text(
                        when (link) {
                            is UiLink.Connected -> link.pcName
                            is UiLink.Connecting -> "Conectando…"
                            else -> "Sin conexión"
                        },
                        style = MaterialTheme.typography.bodyMedium,
                        // Un nombre de PC largo no echa a «Salir» fuera de la pantalla
                        modifier = Modifier.widthIn(max = 160.dp),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                }
                if (link is UiLink.Connected) {
                    if (isWiiUAsWiimote(link)) {
                        Text("Wii U · Mando de Wii", style = MaterialTheme.typography.bodyMedium)
                    }
                    // Selector Puntero/Dolphin/Wii U también de lado (solo el Jugador 1)
                    if (showModeChips(link, showChips)) {
                        ModeChips(current = link.mode, supportsCemu = link.supportsCemu, compact = true)
                    }
                    // Modo Wii U: texto para el teclado en pantalla de Cemu
                    if (link.mode == LinkState.MODE_CEMU) {
                        KeyboardButton(compact = true) { keyboardOpen = true }
                    }
                }
                TextButton(onClick = onDisconnect) {
                    Text("Salir", color = PepoColors.Error, style = MaterialTheme.typography.bodyMedium)
                }
            }
            if (link is UiLink.Connected && isWiiUAsWiimote(link)) {
                PadSelector(link, compact = true)
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

        // − / + / A centro (un 20 % más grandes que en la primera versión:
        // de lado se pulsan con el pulgar y quedaban pequeños)
        Column(
            modifier = Modifier.align(Alignment.Center),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Spacer(Modifier.height(20.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                RoundButton("−", 53.dp, ButtonState.MINUS, textSize = 19)
                RoundButton("+", 53.dp, ButtonState.PLUS, textSize = 19)
            }
            RoundButton("A", 62.dp, ButtonState.A, textSize = 22)
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
                textColor = PepoColors.OnAccent,
                textSize = 28
            )
            RoundButton(
                "2", 92.dp, ButtonState.TWO,
                background = PepoColors.Blue,
                pressedColor = PepoColors.BlueHover,
                textColor = PepoColors.OnAccent,
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

        PrecisionPill(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(start = 16.dp, bottom = 6.dp)
        )

        NoticeBanner(
            Modifier
                .align(Alignment.TopCenter)
                .padding(top = 84.dp)
        )

        if (keyboardOpen) {
            KeyboardDialog(
                onSend = { LinkState.sendText?.invoke(it) },
                onClose = { keyboardOpen = false }
            )
        }
    }
}
