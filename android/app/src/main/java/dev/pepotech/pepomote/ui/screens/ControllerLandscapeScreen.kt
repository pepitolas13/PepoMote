package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.displayCutoutPadding
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import dev.pepotech.pepomote.service.Route
import dev.pepotech.pepomote.sensor.Frame
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.CollapsibleHeader
import dev.pepotech.pepomote.ui.components.KeyboardDialog
import dev.pepotech.pepomote.ui.components.LocalPressRegistry
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.NearPill
import dev.pepotech.pepomote.ui.components.PrecisionPill
import dev.pepotech.pepomote.ui.components.RotateSuggestion
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.components.UiScale
import dev.pepotech.pepomote.ui.components.rememberPressRegistry
import dev.pepotech.pepomote.ui.components.slideCanvas
import dev.pepotech.pepomote.ui.theme.PepoColors
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R
import kotlin.math.roundToInt

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
    val context = androidx.compose.ui.platform.LocalContext.current
    var keyboardOpen by remember { mutableStateOf(false) }
    DisposableEffect(Unit) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }
    // Como en vertical: el teclado y la pose congelada van juntos y se sueltan
    // solos (al cerrar, al cambiar de modo, al reconectar o al salir)
    androidx.compose.runtime.LaunchedEffect(keyboardOpen, link) {
        if (keyboardOpen && !dev.pepotech.pepomote.service.Route.showsKeyboard(link)) keyboardOpen = false
        LinkState.motion?.pointerHold =
            keyboardOpen && dev.pepotech.pepomote.service.Route.holdsPointerForKeyboard(link)
    }
    DisposableEffect(Unit) { onDispose { LinkState.motion?.pointerHold = false } }
    // Móvil de lado: los sensores giran con el mando (Route.sidewaysRotation:
    // mando girado con el IR a la izquierda en un juego, o apuntando con el
    // borde largo en modo puntero); al salir, el mando vertical de siempre
    val engine = LinkState.motion
    val rotation = rememberDisplayRotation()
    LaunchedEffect(engine, link, rotation) {
        engine?.rotation = Route.sidewaysRotation(link, rotation)
    }
    DisposableEffect(engine) {
        onDispose {
            engine?.rotation = Frame.ROTATION_0
            ButtonState.reset()
        }
    }

    // Capa de pulsación de la pantalla: cada botón dice dónde está y, con
    // «Pulsar deslizando», el dedo pasa de uno a otro sin levantarlo
    val press = rememberPressRegistry()
    DisposableEffect(press) { onDispose { press.releaseAll() } }

    CompositionLocalProvider(LocalPressRegistry provides press) {
        BoxWithConstraints(
            modifier = Modifier
                .fillMaxSize()
                .background(PepoColors.Background)
                .statusBarsPadding()
                .displayCutoutPadding()
                .navigationBarsPadding()
                .slideCanvas()
        ) {
            // En una tablet todo crece a la vez (UiScale; en cualquier móvil, 1)
            val s = UiScale.landscape(maxWidth.value, maxHeight.value)

            // Cruceta izquierda
            Box(
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    .padding(start = 34.dp * s)
            ) {
                // En un juego, la cruceta de un mando girado (IR a la izquierda)
                PadCross(sizeDp = 190.dp * s, sideways = Route.sidewaysDpad(link))
            }

            // RetroArch como mando de NES: 1 y 2 son B y A del RetroPad, la A
            // grande es X y Home abre el menú de RetroArch (el receptor los mapea)
            val retro = Route.isRetroArch(link)
            // − / ◎ / + y A en el centro (un 20 % más grandes que en la primera
            // versión: de lado se pulsan con el pulgar y quedaban pequeños). La
            // diana, como en el mando vertical y en el mando + Nunchuk: mantener
            // recentra el cursor (puntero) o el puntero IR (Dolphin, Mando de Wii
            // en Cemu), que de lado también se apunta. Home, pequeño, a la derecha
            // de A y fuera del modo puntero (el PC no le da uso): como en el Mando
            // de Wii de lado, donde −/Home/+ quedan entre A y 1/2; Dolphin abre con
            // él el menú HOME y Mario Party 10 lo pide para emparejar cada mando
            Column(
                modifier = Modifier.align(Alignment.Center),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp * s)
            ) {
                Spacer(Modifier.height(20.dp * s))
                Row(
                    horizontalArrangement = Arrangement.spacedBy(16.dp * s),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    RoundButton("−", 53.dp * s, ButtonState.MINUS, textSize = (19 * s).roundToInt())
                    RecenterButton(size = 54.dp * s)
                    RoundButton("+", 53.dp * s, ButtonState.PLUS, textSize = (19 * s).roundToInt())
                }
                if (showHomeButton(link)) {
                    // un hueco igual a la izquierda deja A centrada bajo la diana
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(14.dp * s),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Spacer(Modifier.size(40.dp * s))
                        RoundButton(if (Route.retroNes(link)) "X" else "A", 62.dp * s, ButtonState.A, textSize = (22 * s).roundToInt())
                        RoundButton(stringResource(if (retro) R.string.retro_menu else R.string.home_btn), 40.dp * s, ButtonState.HOME, textSize = ((if (retro) 10 else 11) * s).roundToInt())
                    }
                } else {
                    RoundButton("A", 62.dp * s, ButtonState.A, textSize = (22 * s).roundToInt())
                }
            }

            // 1 y 2 grandes a la derecha (los botones de acción del modo NES)
            Row(
                modifier = Modifier
                    .align(Alignment.CenterEnd)
                    .padding(end = 30.dp * s),
                horizontalArrangement = Arrangement.spacedBy(18.dp * s)
            ) {
                RoundButton(
                    if (retro) "B" else "1", 92.dp * s, ButtonState.ONE,
                    background = PepoColors.Blue,
                    pressedColor = PepoColors.BlueHover,
                    textColor = PepoColors.OnAccent,
                    textSize = (28 * s).roundToInt()
                )
                RoundButton(
                    if (retro) "A" else "2", 92.dp * s, ButtonState.TWO,
                    background = PepoColors.Blue,
                    pressedColor = PepoColors.BlueHover,
                    textColor = PepoColors.OnAccent,
                    textSize = (28 * s).roundToInt(),
                    pop = true
                )
            }

            if (retro) {
                Box(Modifier.align(Alignment.BottomCenter).padding(bottom = 8.dp).width(112.dp * s)) {
                    dev.pepotech.pepomote.ui.components.TriggerZone(
                        label = if (Route.retroNes(link)) "Y" else "B", height = 44.dp * s)
                }
            } else Text(
                stringResource(R.string.rotate_hint),
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 8.dp)
            )

            // En Dolphin la precisión no hace nada: la píldora es «Acercar»
            val pill = Modifier
                .align(Alignment.BottomStart)
                .padding(start = 16.dp, bottom = 6.dp)
            if (isDolphin(link)) NearPill(modifier = pill) else PrecisionPill(modifier = pill)

            // Aviso de «sin giroscopio real» (una vez, en modo puntero: de lado
            // también se apunta) y, debajo, los avisos transitorios del receptor
            Column(
                Modifier
                    .align(Alignment.TopCenter)
                    .padding(top = 84.dp),
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
                    onClose = { keyboardOpen = false },
                    retroPad = Route.isRetroArch(link),
                    pointer = (link as? UiLink.Connected)?.mode == LinkState.MODE_POINTER
                )
            }

            // Cabecera plegable, la última del Box: desplegada tapa a los
            // botones (y no al revés) y a los cuatro segundos se quita de en
            // medio. Dentro lleva lo que antes iba en una línea: el PC, los
            // chips, «Teclado», «En Cemu soy» y «Salir»
            CollapsibleHeader(
                link = link,
                showChips = showChips,
                modifier = Modifier.align(Alignment.TopCenter),
                onKeyboard = {
                    dev.pepotech.pepomote.ui.components.openKeyboard(context, link, press)
                    keyboardOpen = true
                },
                onDisconnect = onDisconnect
            )
        }
    }
}
