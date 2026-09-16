package dev.pepotech.pepomote.ui.screens

import android.view.HapticFeedbackConstants
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.displayCutoutPadding
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.KeyboardButton
import dev.pepotech.pepomote.ui.components.KeyboardDialog
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.RotateSuggestion
import dev.pepotech.pepomote.ui.components.PadSelector
import dev.pepotech.pepomote.ui.components.NearStrip
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
 * Mando vertical estilo Wiimote: cruceta, −/diana/+, A, 1/Home/2 (Home fuera
 * del modo puntero), multimedia, B.
 * `showChips`: mostrar el selector Puntero/Dolphin/Wii U (entrada por Conectar
 * con el ajuste activo). Entrando por la tarjeta Dolphin no hay selector: esa
 * pantalla es solo-Dolphin. Dentro de Wii U como Mando de Wii, la cabecera lo
 * dice, los chips se ven siempre (Jugador 1) y debajo va el selector
 * «En Cemu soy» con su ayuda; el puntero se recentra como en Dolphin. En
 * modo Wii U la cabecera lleva además «Teclado» (texto para el teclado en
 * pantalla de Cemu).
 */
@OptIn(ExperimentalLayoutApi::class)
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
            .displayCutoutPadding()
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
                // Chips de modo (Jugador 1 con el ajuste activo, y siempre dentro de
                // Wii U) y, en Dolphin, el interruptor «Nunchuk» a la derecha de Wii U
                // con otro tono: es una opción, no un modo. Todo en UNA fila, lejos
                // de la cruceta (antes el Nunchuk iba en una fila propia justo
                // encima y se pulsaba sin querer): en un móvil los chips van densos
                // para que quepan los cuatro; si aun así no caben (letra grande),
                // el último pasa a la fila siguiente
                val modeChips = showModeChips(link, showChips)
                val nunchuk = showNunchukChip(link)
                if (modeChips || nunchuk) {
                    Spacer(Modifier.height(6.dp))
                    val dense = colW < 420.dp
                    FlowRow(
                        horizontalArrangement = Arrangement.spacedBy(if (dense) 8.dp else 10.dp, Alignment.CenterHorizontally),
                        verticalArrangement = Arrangement.spacedBy(6.dp)
                    ) {
                        if (modeChips) ModeChips(current = link.mode, supportsCemu = link.supportsCemu, supportsSwitch = link.supportsSwitch, androidReceiver = link.platform == "android", dense = dense)
                        if (nunchuk) NunchukChip(link, dense = dense, modifier = Modifier.padding(start = 4.dp))
                    }
                }
                // Wii U como Mando de Wii: qué mando soy en Cemu, con su ayuda
                if (isWiiUAsWiimote(link)) {
                    Spacer(Modifier.height(8.dp))
                    PadSelector(link, help = stringResource(R.string.wii_pad_help))
                }
            }

            // Hueco de sobra entre los chips y la cruceta: que ir a por ↑ no toque un chip
            Gap(18.dp * grow, flexible)
            PadCross(sizeDp = 168.dp * grow)

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
            // 1 · Home · 2, como en el mando + Nunchuk. Home solo fuera del modo
            // puntero (el PC no le da uso): en Dolphin es el menú HOME de la Wii
            // y en Cemu, además, Mario Party 10 lo pide para dar por emparejado
            // cada Mando de Wii emulado
            Row(
                horizontalArrangement = Arrangement.spacedBy(20.dp * grow),
                verticalAlignment = Alignment.CenterVertically
            ) {
                RoundButton("1", 52.dp * grow, ButtonState.ONE, textSize = (18 * grow).roundToInt())
                if (showHomeButton(link)) {
                    RoundButton(stringResource(R.string.home_btn), 52.dp * grow, ButtonState.HOME, textSize = (12 * grow).roundToInt())
                }
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
        // (sigue aunque el dedo se salga de la tira). En Dolphin la precisión
        // no hace nada: ahí la tira es «Acercar» (el mando emulado se acerca
        // a la pantalla, para los juegos que lo piden)
        val sideStrip = Modifier
            .align(Alignment.CenterStart)
            .padding(start = gutter)
            .fillMaxHeight(0.45f)
            .width(40.dp * grow)
        if (isDolphin(link)) {
            NearStrip(modifier = sideStrip, glyph = 16.dp * grow)
        } else {
            PrecisionStrip(modifier = sideStrip, glyph = 16.dp * grow)
        }

        // Aviso de «sin giroscopio real» (una vez, en modo puntero) y, debajo,
        // los avisos transitorios del receptor
        Column(
            Modifier
                .align(Alignment.TopCenter)
                .padding(top = 64.dp, start = 24.dp, end = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            RotateSuggestion()
            GyroWarnCard(link)
            NoticeBanner()
        }

        if (keyboardOpen) {
            KeyboardDialog(
                onSend = { LinkState.sendText?.invoke(it) },
                onClose = { keyboardOpen = false }
            )
        }
    }
}

/** Modo Dolphin: la tira / píldora lateral es «Acercar» (la precisión ahí no hace nada). */
internal fun isDolphin(link: UiLink): Boolean =
    link is UiLink.Connected && link.mode == LinkState.MODE_DOLPHIN

/** Modo Wii U actuando como Mando de Wii (layouts Wii de siempre, 72 bytes). */
internal fun isWiiUAsWiimote(link: UiLink.Connected): Boolean =
    link.mode == LinkState.MODE_CEMU && link.pad == LinkState.PAD_WIIMOTE

/** Chips de modo: solo el Jugador 1; con el ajuste activo o, siempre, dentro de Wii U. */
internal fun showModeChips(link: UiLink.Connected, showChips: Boolean): Boolean =
    link.slot == 0 && (showChips || link.mode == LinkState.MODE_CEMU || link.platform == "android")

/** Chip «Nunchuk»: en Dolphin, cualquier jugador que sea mando (no un Nunchuk). */
internal fun showNunchukChip(link: UiLink.Connected): Boolean =
    link.mode == LinkState.MODE_DOLPHIN && link.role == LinkState.ROLE_WIIMOTE

/**
 * Botón Home del Mando de Wii (vertical y apaisado): conectado y fuera del
 * modo puntero, donde el PC no le da uso. En Dolphin es el menú HOME de la
 * Wii; en Cemu, además, Mario Party 10 lo pide para dar por emparejado cada
 * Mando de Wii emulado. El mando + Nunchuk y el GamePad lo tienen siempre.
 */
internal fun showHomeButton(link: UiLink): Boolean =
    link is UiLink.Connected && link.mode != LinkState.MODE_POINTER

/**
 * Chip «Nunchuk» (modo Dolphin): el mando lleva su propio Nunchuk (con el
 * móvil de lado: stick, C y Z). Cambia el ajuste y se lo pide al receptor;
 * marcado solo cuando el receptor lo ha confirmado. Cambiarlo exige reabrir
 * Dolphin (el receptor lo avisa).
 */
@Composable
internal fun NunchukChip(link: UiLink.Connected, compact: Boolean = false, dense: Boolean = false, modifier: Modifier = Modifier) {
    val context = androidx.compose.ui.platform.LocalContext.current
    ModeChip(stringResource(R.string.nunchuk_chip), selected = link.ownNunchuk, compact = compact, dense = dense, toggle = true, modifier = modifier) {
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
    LinkState.MODE_SWITCH -> stringResource(R.string.mode_switch)
    else -> stringResource(R.string.mode_pointer)
}

/**
 * Selector de modo: Puntero (controla el PC) / Dolphin (Wiimote virtual) /
 * Wii U (GamePad para Cemu, solo si el receptor lo soporta). Selección por
 * igualdad exacta del modo. Wii U pasa al GamePad al instante (optimista) y
 * el eco lo confirma. Lo comparten el mando vertical, el apaisado y el GamePad.
 */
@Composable
@OptIn(ExperimentalLayoutApi::class)
internal fun ModeChips(current: String, supportsCemu: Boolean, supportsSwitch: Boolean = false, compact: Boolean = false, dense: Boolean = false, modifier: Modifier = Modifier, androidReceiver: Boolean = false) {
    val chips: @Composable () -> Unit = {
        if (!androidReceiver) ModeChip(stringResource(R.string.mode_pointer), selected = current == LinkState.MODE_POINTER, compact = compact, dense = dense) {
            LinkState.requestMode(LinkState.MODE_POINTER)
        }
        ModeChip(stringResource(R.string.mode_dolphin), selected = current == LinkState.MODE_DOLPHIN, compact = compact, dense = dense) {
            LinkState.requestMode(LinkState.MODE_DOLPHIN)
        }
        if (supportsCemu && !androidReceiver) {
            ModeChip(stringResource(R.string.mode_wiiu), selected = current == LinkState.MODE_CEMU, compact = compact, dense = dense) {
                LinkState.requestMode(LinkState.MODE_CEMU)
            }
        }
        if (supportsSwitch) {
            ModeChip(stringResource(if (androidReceiver) R.string.mode_eden else R.string.mode_switch), selected = current == LinkState.MODE_SWITCH, compact = compact, dense = dense) {
                LinkState.requestMode(LinkState.MODE_SWITCH)
            }
        }
    }
    val gap = if (compact) 6.dp else if (dense) 8.dp else 10.dp
    if (compact) Row(modifier, horizontalArrangement = Arrangement.spacedBy(gap)) { chips() }
    else FlowRow(modifier, horizontalArrangement = Arrangement.spacedBy(gap), verticalArrangement = Arrangement.spacedBy(6.dp)) { chips() }
}

/**
 * Chip de modo (azul cuando es el activo). [compact]: bajo y estrecho, para
 * las cabeceras apaisadas; [dense]: solo estrecho, para que quepan cuatro en
 * la fila de un móvil; [toggle]: no es un modo sino un interruptor (el
 * Nunchuk): verde encendido y con contorno verde apagado, para que se vea
 * que es una opción activable.
 */
@Composable
internal fun ModeChip(
    label: String,
    selected: Boolean,
    compact: Boolean = false,
    dense: Boolean = false,
    toggle: Boolean = false,
    enabled: Boolean = true,
    modifier: Modifier = Modifier,
    onClick: () -> Unit
) {
    // pointerInput(label) arranca una vez: sin esto el gesto se quedaba con el
    // onClick de la primera composición (el chip Nunchuk, que calcula «lo
    // contrario de ahora», solo funcionaba la primera vez)
    val current by rememberUpdatedState(onClick)
    val shape = RoundedCornerShape(18.dp)
    val fill = if (!selected) PepoColors.Card else if (toggle) PepoColors.Ok else PepoColors.Blue
    val text = if (!selected) PepoColors.TextDim else if (toggle) PepoColors.OnAccent else PepoColors.Card
    Box(
        modifier = modifier
            .alpha(if (enabled) 1f else 0.5f)
            .background(fill, shape)
            .then(if (toggle && !selected) Modifier.border(1.5.dp, PepoColors.Ok, shape) else Modifier)
            .semantics { this.selected = selected }
            .clickable(enabled = enabled, role = if (toggle) Role.Switch else Role.RadioButton) { current() }
            .padding(
                horizontal = if (compact || dense) 12.dp else 18.dp,
                vertical = if (compact) 6.dp else 8.dp
            )
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium.copy(color = text),
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

/**
 * Diana de recentrado: mantener 150 ms → vibra y recentra (el cursor en modo
 * puntero, el puntero IR en Dolphin y como Mando de Wii en Cemu). La misma en
 * el mando vertical, en el apaisado y en el mando + Nunchuk.
 */
@Composable
internal fun RecenterButton(size: Dp = 64.dp) {
    val view = LocalView.current
    var down by remember { mutableStateOf(false) }
    val label = stringResource(R.string.recenter)

    Box(
        modifier = Modifier
            .size(size)
            .semantics { contentDescription = label }
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
