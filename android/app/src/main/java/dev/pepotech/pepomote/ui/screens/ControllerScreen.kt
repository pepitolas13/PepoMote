package dev.pepotech.pepomote.ui.screens

import android.view.HapticFeedbackConstants
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.KeyboardButton
import dev.pepotech.pepomote.ui.components.KeyboardDialog
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.PadSelector
import dev.pepotech.pepomote.ui.components.PrecisionStrip
import dev.pepotech.pepomote.ui.components.ReconnectingLabel
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.components.TriggerZone
import dev.pepotech.pepomote.ui.components.UiScale
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.roundToInt
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

/**
 * Mando vertical estilo Wiimote: cruceta, −/diana/+, A, 1/2, multimedia, B.
 * `showChips`: mostrar el selector Puntero/Dolphin/Wii U (entrada por Conectar
 * con el ajuste activo). Entrando por la tarjeta Dolphin no hay selector: esa
 * pantalla es solo-Dolphin. Dentro de Wii U como Mando de Wii, la cabecera lo
 * dice, los chips se ven siempre (Jugador 1) y debajo va el selector
 * «En Cemu soy» con su ayuda; el puntero se recentra como en Dolphin. En
 * modo Wii U la cabecera lleva además «Teclado» (texto para el teclado en
 * pantalla de Cemu).
 */
@Composable
fun ControllerScreen(link: UiLink, showChips: Boolean, onDisconnect: () -> Unit) {
    val view = LocalView.current
    var keyboardOpen by remember { mutableStateOf(false) }

    DisposableEffect(Unit) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }

    // Recentrar cuando el mando está EN PANTALLA y conectado: es el momento
    // en que el usuario de verdad empuña el móvil (la primera conexión llega
    // moviéndolo tras escanear el QR y un recentrado temprano se pierde).
    val connected = link is UiLink.Connected
    androidx.compose.runtime.LaunchedEffect(connected) {
        if (connected) {
            kotlinx.coroutines.delay(400)
            ButtonState.bumpRecenter()
        }
    }

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .navigationBarsPadding()
    ) {
        // En una tablet todo crece a la vez (UiScale; en cualquier móvil, 1),
        // la columna del mando se limita a 520·grow y las tiras la abrazan (en
        // vez de irse a los bordes de la pantalla); los huecos entre grupos se
        // vuelven flexibles para repartir la holgura vertical
        val grow = UiScale.remote(maxWidth.value, maxHeight.value)
        val colW = minOf(maxWidth, 520.dp * grow)
        val gutter = (maxWidth - colW) / 2
        val flexible = grow > 1f
        Column(
            modifier = Modifier
                .align(Alignment.Center)
                .fillMaxHeight()
                .width(colW)
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
                            Text(
                                if (link.slot > 0) stringResource(R.string.pc_player, link.pcName, link.slot + 1)
                                else link.pcName,
                                style = MaterialTheme.typography.titleMedium
                            )
                            val modeText = when {
                                isWiiUAsWiimote(link) -> stringResource(R.string.wiiu_as_wiimote)
                                link.mode == LinkState.MODE_CEMU -> stringResource(R.string.mode_wiiu)
                                link.mode == LinkState.MODE_DOLPHIN && link.ownNunchuk -> stringResource(R.string.mode_dolphin_nunchuk)
                                link.mode == LinkState.MODE_DOLPHIN -> stringResource(R.string.mode_dolphin)
                                link.slot > 0 -> stringResource(R.string.pointer_player1_points)
                                else -> stringResource(R.string.mode_pointer)
                            }
                            Text(
                                buildString {
                                    append(modeText)
                                    link.rttMs?.let { append(" · ${"%.0f".format(it)} ms") }
                                },
                                style = MaterialTheme.typography.bodyMedium
                            )
                        }

                        is UiLink.Connecting -> Text(stringResource(R.string.status_connecting), style = MaterialTheme.typography.titleMedium)
                        is UiLink.Reconnecting -> ReconnectingLabel(link)
                        else -> {
                            Text(stringResource(R.string.status_disconnected), style = MaterialTheme.typography.titleMedium)
                            val ctx = androidx.compose.ui.platform.LocalContext.current
                            if (dev.pepotech.pepomote.net.PairStore.load(ctx) != null) {
                                TextButton(onClick = {
                                    dev.pepotech.pepomote.service.LinkForegroundService.start(ctx)
                                }) {
                                    Text(stringResource(R.string.reconnect), color = PepoColors.Blue)
                                }
                            }
                        }
                    }
                }
                // Modo Wii U: texto para el teclado en pantalla de Cemu
                if (link is UiLink.Connected && link.mode == LinkState.MODE_CEMU) {
                    KeyboardButton(compact = true) { keyboardOpen = true }
                    Spacer(Modifier.width(4.dp))
                }
                TextButton(onClick = onDisconnect) {
                    Text(stringResource(R.string.exit), color = PepoColors.Error)
                }
            }

            if (link is UiLink.Connected) {
                // Chips de modo: Jugador 1 con el ajuste activo, y siempre dentro de Wii U
                if (showModeChips(link, showChips)) {
                    Spacer(Modifier.height(6.dp))
                    ModeChips(current = link.mode, supportsCemu = link.supportsCemu)
                }
                // Dolphin: el Nunchuk en el mismo móvil (gira el móvil para usarlo)
                if (showNunchukChip(link)) {
                    Spacer(Modifier.height(6.dp))
                    NunchukChip(link)
                }
                // Wii U como Mando de Wii: qué mando soy en Cemu, con su ayuda
                if (isWiiUAsWiimote(link)) {
                    Spacer(Modifier.height(8.dp))
                    PadSelector(link, help = stringResource(R.string.wii_pad_help))
                }
            }

            Gap(10.dp * grow, flexible)
            PadCross(sizeDp = 168.dp * grow, glyphSp = (14 * grow).roundToInt())

            Gap(16.dp * grow, flexible)
            Row(
                horizontalArrangement = Arrangement.spacedBy(20.dp * grow),
                verticalAlignment = Alignment.CenterVertically
            ) {
                RoundButton("−", 54.dp * grow, ButtonState.MINUS, textSize = (20 * grow).roundToInt())
                RecenterButton(size = 64.dp * grow)
                RoundButton("+", 54.dp * grow, ButtonState.PLUS, textSize = (20 * grow).roundToInt())
            }

            Gap(16.dp * grow, flexible)
            RoundButton(
                "A", 148.dp * grow, ButtonState.A,
                background = PepoColors.Blue,
                pressedColor = PepoColors.BlueHover,
                textColor = PepoColors.OnAccent,
                textSize = (44 * grow).roundToInt(),
                pop = true
            )

            Gap(14.dp * grow, flexible)
            Row(horizontalArrangement = Arrangement.spacedBy(20.dp * grow)) {
                RoundButton("1", 52.dp * grow, ButtonState.ONE, textSize = (18 * grow).roundToInt())
                RoundButton("2", 52.dp * grow, ButtonState.TWO, textSize = (18 * grow).roundToInt())
            }

            Gap(10.dp * grow, flexible)
            MediaRow(buttonSize = 46.dp * grow, textSize = (16 * grow).roundToInt())

            Spacer(Modifier.weight(1f))
            TriggerZone(height = 88.dp * grow)
            Spacer(Modifier.height(12.dp))
        }

        ScrollStrip(
            modifier = Modifier
                .align(Alignment.CenterEnd)
                .padding(end = gutter)
                .fillMaxHeight(0.45f)
                .width(30.dp * grow)
        )

        // Espejo de la de scroll, algo más ancha: mantener = puntero al 40 %
        // (sigue aunque el dedo se salga de la tira)
        PrecisionStrip(
            modifier = Modifier
                .align(Alignment.CenterStart)
                .padding(start = gutter)
                .fillMaxHeight(0.45f)
                .width(40.dp * grow),
            glyph = 16.dp * grow
        )

        NoticeBanner(
            Modifier
                .align(Alignment.TopCenter)
                .padding(top = 64.dp, start = 24.dp, end = 24.dp)
        )

        if (keyboardOpen) {
            KeyboardDialog(
                onSend = { LinkState.sendText?.invoke(it) },
                onClose = { keyboardOpen = false }
            )
        }
    }
}

/** Modo Wii U actuando como Mando de Wii (layouts Wii de siempre, 72 bytes). */
internal fun isWiiUAsWiimote(link: UiLink.Connected): Boolean =
    link.mode == LinkState.MODE_CEMU && link.pad == LinkState.PAD_WIIMOTE

/** Chips de modo: solo el Jugador 1; con el ajuste activo o, siempre, dentro de Wii U. */
internal fun showModeChips(link: UiLink.Connected, showChips: Boolean): Boolean =
    link.slot == 0 && (showChips || link.mode == LinkState.MODE_CEMU)

/** Chip «Nunchuk»: en Dolphin, cualquier jugador que sea mando (no un Nunchuk). */
internal fun showNunchukChip(link: UiLink.Connected): Boolean =
    link.mode == LinkState.MODE_DOLPHIN && link.role == LinkState.ROLE_WIIMOTE

/**
 * Chip «Nunchuk» (modo Dolphin): el mando lleva su propio Nunchuk (con el
 * móvil de lado: stick, C y Z). Cambia el ajuste y se lo pide al receptor;
 * marcado solo cuando el receptor lo ha confirmado. Cambiarlo exige reabrir
 * Dolphin (el receptor lo avisa).
 */
@Composable
internal fun NunchukChip(link: UiLink.Connected, compact: Boolean = false) {
    val context = androidx.compose.ui.platform.LocalContext.current
    ModeChip(stringResource(R.string.nunchuk_chip), selected = link.ownNunchuk, compact = compact) {
        val want = !link.ownNunchuk
        dev.pepotech.pepomote.control.AppPrefs.setOwnNunchuk(context, want)
        LinkState.sendNunchuk?.invoke(want)
    }
}

/** Nombre del modo del receptor para las cabeceras. */
@Composable
internal fun modeLabel(mode: String): String = when (mode) {
    LinkState.MODE_DOLPHIN -> stringResource(R.string.mode_dolphin)
    LinkState.MODE_CEMU -> stringResource(R.string.mode_wiiu)
    else -> stringResource(R.string.mode_pointer)
}

/**
 * Selector de modo: Puntero (controla el PC) / Dolphin (Wiimote virtual) /
 * Wii U (GamePad para Cemu, solo si el receptor lo soporta). Selección por
 * igualdad exacta del modo. Wii U pasa al GamePad al instante (optimista) y
 * el eco lo confirma. Lo comparten el mando vertical, el apaisado y el GamePad.
 */
@Composable
internal fun ModeChips(current: String, supportsCemu: Boolean, compact: Boolean = false, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(if (compact) 6.dp else 10.dp)) {
        ModeChip(stringResource(R.string.mode_pointer), selected = current == LinkState.MODE_POINTER, compact = compact) {
            LinkState.requestMode(LinkState.MODE_POINTER)
        }
        ModeChip(stringResource(R.string.mode_dolphin), selected = current == LinkState.MODE_DOLPHIN, compact = compact) {
            LinkState.requestMode(LinkState.MODE_DOLPHIN)
        }
        if (supportsCemu) {
            ModeChip(stringResource(R.string.mode_wiiu), selected = current == LinkState.MODE_CEMU, compact = compact) {
                LinkState.requestMode(LinkState.MODE_CEMU)
            }
        }
    }
}

@Composable
internal fun ModeChip(label: String, selected: Boolean, compact: Boolean = false, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .background(
                if (selected) PepoColors.Blue else PepoColors.Card,
                RoundedCornerShape(18.dp)
            )
            .pointerInput(label) {
                detectTapGestures(onTap = { onClick() })
            }
            .padding(
                horizontal = if (compact) 12.dp else 18.dp,
                vertical = if (compact) 6.dp else 8.dp
            )
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium.copy(
                color = if (selected) PepoColors.Card else PepoColors.TextDim
            ),
            maxLines = 1
        )
    }
}

/**
 * Hueco entre grupos del mando: fijo en el móvil; flexible en una tablet
 * (reparte la holgura vertical con los demás huecos flexibles).
 */
@Composable
private fun ColumnScope.Gap(h: Dp, flexible: Boolean) {
    if (flexible) {
        Spacer(Modifier.weight(1f).heightIn(min = h))
    } else {
        Spacer(Modifier.height(h))
    }
}

/** Diana de recentrado: mantener 150 ms → vibra y recentra. */
@Composable
internal fun RecenterButton(size: Dp = 64.dp) {
    val view = LocalView.current
    var down by remember { mutableStateOf(false) }

    Box(
        modifier = Modifier
            .size(size)
            .background(if (down) PepoColors.Glow else PepoColors.Card, CircleShape)
            .pointerInput(Unit) {
                detectTapGestures(onPress = {
                    down = true
                    coroutineScope {
                        val job = launch {
                            delay(150)
                            ButtonState.bumpRecenter()
                            view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                            dev.pepotech.pepomote.control.UiSounds.tick()
                        }
                        tryAwaitRelease()
                        job.cancel()
                    }
                    down = false
                })
            },
        contentAlignment = Alignment.Center
    ) {
        // 26/64 y 10/64 del tamaño (exactos en un móvil)
        Box(
            Modifier
                .size(size * 0.40625f)
                .background(PepoColors.Background, CircleShape),
            contentAlignment = Alignment.Center
        ) {
            Box(
                Modifier
                    .size(size * 0.15625f)
                    .background(PepoColors.Blue, CircleShape)
            )
        }
    }
}

/** Fila multimedia plegable. */
@Composable
private fun MediaRow(buttonSize: Dp = 46.dp, textSize: Int = 16) {
    var expanded by remember { mutableStateOf(false) }

    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        TextButton(onClick = { expanded = !expanded }) {
            Text(
                if (expanded) stringResource(R.string.media_open) else stringResource(R.string.media_closed),
                color = PepoColors.TextDim,
                style = MaterialTheme.typography.bodyMedium
            )
        }
        if (expanded) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                RoundButton("⏮", buttonSize, ButtonState.MEDIA_PREV, textSize = textSize)
                RoundButton("🔉", buttonSize, ButtonState.MEDIA_VOL_DOWN, textSize = textSize)
                RoundButton("⏯", buttonSize, ButtonState.MEDIA_PLAY_PAUSE, textSize = textSize)
                RoundButton("🔇", buttonSize, ButtonState.MEDIA_MUTE, textSize = textSize)
                RoundButton("🔊", buttonSize, ButtonState.MEDIA_VOL_UP, textSize = textSize)
                RoundButton("⏭", buttonSize, ButtonState.MEDIA_NEXT, textSize = textSize)
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
