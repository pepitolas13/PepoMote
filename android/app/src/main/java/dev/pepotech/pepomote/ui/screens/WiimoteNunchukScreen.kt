package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.displayCutoutPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.sensor.SenderKind
import dev.pepotech.pepomote.service.LandscapeSide
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.NunchukSide
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.AnalogStick
import dev.pepotech.pepomote.ui.components.CollapsibleHeader
import dev.pepotech.pepomote.ui.components.HeaderCollapse
import dev.pepotech.pepomote.ui.components.LocalPressRegistry
import dev.pepotech.pepomote.ui.components.NearPill
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.components.ShoulderButton
import dev.pepotech.pepomote.ui.components.UiScale
import dev.pepotech.pepomote.ui.components.rememberPressRegistry
import dev.pepotech.pepomote.ui.components.rememberScreenReader
import dev.pepotech.pepomote.ui.components.slideCanvas
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlin.math.roundToInt

/**
 * Medidas (dp) del mando de Wii con Nunchuk en un solo móvil, apaisado. `s`
 * es la escala de tablet ([UiScale.landscape]; 1 en cualquier móvil). El
 * stick es lo más grande que dejan la altura bajo las pastillas Z/C, su
 * columna (40 % del ancho) y el hueco que necesita la columna central entre
 * él y la cruceta + A de la derecha. Sin Compose: se prueba en la JVM.
 */
class WiiNunchukMetrics(w: Float, h: Float, val s: Float) {
    /** Cabecera compacta con su margen. */
    val headerH = 44f
    val margin = 8f * s
    val gap = 8f * s
    val pillH = 40f * s
    val zW = 120f * s
    val cW = 80f * s
    val bW = 150f * s
    val small = 44f * s
    val recenter = 54f * s
    val rowGap = 12f * s
    val bodyH = h - headerH
    val cross = minOf(0.45f * bodyH, 150f * s)
    val a = minOf(0.45f * bodyH, 148f * s)
    /** Lo que necesita la columna central (tres botones y sus huecos). */
    val centerMin = small * 3 + rowGap * 2 + gap * 2
    val stick = maxOf(
        minOf(
            bodyH - pillH - gap - margin * 2,
            0.40f * w - margin * 2,
            w - margin * 2 - cross - gap - a - centerMin,
            280f * s,
        ),
        120f,
    )

    /** Tamaño de texto (sp): crece con la tablet. */
    fun text(base: Float): Int = (base * s).roundToInt()
}

/**
 * Mando de Wii + Nunchuk en un solo móvil (modo Dolphin, apaisado, a dos
 * manos): a la izquierda Z y C arriba (donde el GamePad tiene L/ZL) y el
 * stick del Nunchuk; en el centro −, diana, +, 1, Home, 2; a la derecha B
 * arriba (el gatillo, bajo el índice), la cruceta y la A grande. El puntero
 * lo calcula Dolphin con los sensores, remapeados al marco apaisado como en
 * el GamePad: se apunta con el móvil de lado, y agitar es agitar. Solo se
 * enseña cuando el receptor ha confirmado el Nunchuk propio (`ownNunchuk`);
 * en vertical vuelve el mando de siempre.
 */
@Composable
fun WiimoteNunchukScreen(link: UiLink, showChips: Boolean, onDisconnect: () -> Unit) {
    val view = LocalView.current
    val engine = LinkState.motion
    val rotation = rememberDisplayRotation()
    DisposableEffect(Unit) {
        view.keepScreenOn = true
        onDispose {
            view.keepScreenOn = false
            ButtonState.reset()
        }
    }
    // Trama con stick + C/Z y los sensores remapeados al marco apaisado; al
    // salir (vertical, Salir) vuelve el Wiimote de siempre y nada queda pulsado
    DisposableEffect(engine) {
        if (engine != null) {
            val prevKind = engine.kind
            val prevRotation = engine.rotation
            engine.rotation = rotation
            engine.kind = SenderKind.WII_NUNCHUK
            onDispose {
                engine.kind = prevKind
                engine.rotation = prevRotation
                ButtonState.reset()
            }
        } else {
            onDispose { }
        }
    }
    // El móvil puede girar 180° entre los dos apaisados: el remapeo lo sigue
    LaunchedEffect(engine, rotation) {
        engine?.rotation = rotation
    }
    // Primera vez: el lado del apaisado lo ha elegido el sensor; se pregunta
    // si es el bueno y se guarda para siempre (Ajustes lo cambia). Al salir
    // sin confirmar, la prueba («Darle la vuelta») se olvida
    val context = LocalContext.current
    val sideSaved by NunchukSide.saved.collectAsState()
    val sideProvisional by NunchukSide.provisional.collectAsState()
    DisposableEffect(Unit) {
        onDispose { NunchukSide.setProvisional(null) }
    }

    // Capa de pulsación de la pantalla: cada botón dice dónde está y, con
    // «Pulsar deslizando», el dedo pasa de uno a otro sin levantarlo
    val press = rememberPressRegistry()
    DisposableEffect(press) { onDispose { press.releaseAll() } }

    // La cabecera plegable avisa de si está desplegada (la pregunta del lado
    // ocupa el mismo sitio) y de cuánto mide, para poner la pregunta justo
    // debajo cuando la cabecera no se va a plegar sola
    var headerExpanded by remember { mutableStateOf(true) }
    var headerBox by remember { mutableStateOf(0.dp) }
    val density = LocalDensity.current
    // Con lector de pantalla la cabecera se queda abierta hasta que la cierren:
    // ahí la pregunta no puede esperar a que se pliegue o no saldría nunca
    val autoCollapse = HeaderCollapse.autoCollapses(link is UiLink.Connected, rememberScreenReader())

    CompositionLocalProvider(LocalPressRegistry provides press) {
        BoxWithConstraints(
            modifier = Modifier
                .fillMaxSize()
                .background(PepoColors.Background)
                .statusBarsPadding()
                .navigationBarsPadding()
                .displayCutoutPadding()
                .slideCanvas()
        ) {
            val s = UiScale.landscape(maxWidth.value, maxHeight.value)
            val m = WiiNunchukMetrics(maxWidth.value, maxHeight.value, s)

            // Izquierda: Z y C arriba (bajo el índice), el stick del Nunchuk abajo (pulgar)
            Row(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .padding(start = m.margin.dp, top = m.headerH.dp),
                horizontalArrangement = Arrangement.spacedBy(m.gap.dp)
            ) {
                ShoulderButton("Z", ButtonState.Z, m.zW.dp, m.pillH.dp, textSize = m.text(16f))
                ShoulderButton("C", ButtonState.C, m.cW.dp, m.pillH.dp, textSize = m.text(16f))
            }
            Box(
                modifier = Modifier
                    .align(Alignment.BottomStart)
                    .padding(start = m.margin.dp, bottom = m.margin.dp)
            ) {
                AnalogStick(m.stick.dp) { x, y -> ButtonState.setStick(x, y) }
            }

            // Centro: − diana + y 1 Home 2
            Column(
                modifier = Modifier.align(Alignment.Center),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(m.rowGap.dp)
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(m.rowGap.dp), verticalAlignment = Alignment.CenterVertically) {
                    RoundButton("−", m.small.dp, ButtonState.MINUS, textSize = m.text(19f))
                    RecenterButton(size = m.recenter.dp)
                    RoundButton("+", m.small.dp, ButtonState.PLUS, textSize = m.text(19f))
                }
                Row(horizontalArrangement = Arrangement.spacedBy(m.rowGap.dp), verticalAlignment = Alignment.CenterVertically) {
                    RoundButton("1", m.small.dp, ButtonState.ONE, textSize = m.text(18f))
                    RoundButton(stringResource(R.string.home_btn), m.small.dp, ButtonState.HOME, textSize = m.text(12f))
                    RoundButton("2", m.small.dp, ButtonState.TWO, textSize = m.text(18f))
                }
                // Acercar el mando a la pantalla (mantener): juegos que lo piden
                NearPill(modifier = Modifier)
            }

            // Derecha: B arriba (el gatillo, bajo el índice); cruceta y A grande abajo (pulgar)
            Box(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(end = m.margin.dp, top = m.headerH.dp)
            ) {
                ShoulderButton("B", ButtonState.B, m.bW.dp, m.pillH.dp, textSize = m.text(16f))
            }
            Row(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(end = m.margin.dp, bottom = m.margin.dp),
                horizontalArrangement = Arrangement.spacedBy(m.gap.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                PadCross(sizeDp = m.cross.dp)
                RoundButton(
                    "A", m.a.dp, ButtonState.A,
                    background = PepoColors.Blue,
                    pressedColor = PepoColors.BlueHover,
                    textColor = PepoColors.OnAccent,
                    textSize = m.text(44f),
                    pop = true
                )
            }

            // La pregunta del lado va donde la cabecera desplegada: se espera a
            // que se pliegue (si no, una encima de la otra) y, cuando no se
            // pliega sola, se pone justo bajo la tarjeta
            if (sideSaved == LandscapeSide.Unset && (!headerExpanded || !autoCollapse)) {
                val shown = LandscapeSide.effective(LandscapeSide.current(rotation), sideProvisional)
                SideAskCard(
                    modifier = Modifier
                        .align(Alignment.TopCenter)
                        .padding(top = if (headerExpanded) headerBox + 4.dp else (m.headerH + 4f).dp),
                    title = stringResource(R.string.side_ask),
                    onFlip = { NunchukSide.setProvisional(shown.flipped()) },
                    onKeep = { NunchukSide.save(context, shown) }
                )
            }

            NoticeBanner(
                Modifier
                    .align(Alignment.TopCenter)
                    .padding(top = 84.dp)
            )

            // Cabecera plegable, la última del Box: desplegada tapa a B, Z y C
            // (y no al revés), y a los cuatro segundos se quita de en medio
            CollapsibleHeader(
                link = link,
                showChips = showChips,
                modifier = Modifier
                    .align(Alignment.TopCenter)
                    .onSizeChanged { headerBox = with(density) { it.height.toDp() } },
                alwaysNunchukChip = true,
                onExpandedChange = { headerExpanded = it },
                onDisconnect = onDisconnect
            )
        }
    }
}
