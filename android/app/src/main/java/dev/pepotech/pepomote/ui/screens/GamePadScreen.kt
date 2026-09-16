package dev.pepotech.pepomote.ui.screens

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Paint
import android.graphics.RectF
import android.hardware.display.DisplayManager
import android.os.Handler
import android.os.Looper
import android.view.HapticFeedbackConstants
import android.view.Surface
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.displayCutoutPadding
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.BiasAlignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.net.ScreenClient
import dev.pepotech.pepomote.sensor.SenderKind
import dev.pepotech.pepomote.service.GamePadSide
import dev.pepotech.pepomote.service.LandscapeSide
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.Route
import dev.pepotech.pepomote.service.ScreenLink
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.AnalogStick
import dev.pepotech.pepomote.ui.components.CollapsibleHeader
import dev.pepotech.pepomote.ui.components.FitRect
import dev.pepotech.pepomote.ui.components.FullScreenMetrics
import dev.pepotech.pepomote.ui.components.GamePadHeaderText
import dev.pepotech.pepomote.ui.components.HeaderCollapse
import dev.pepotech.pepomote.ui.components.KeyboardButton
import dev.pepotech.pepomote.ui.components.KeyboardDialog
import dev.pepotech.pepomote.ui.components.LocalPressRegistry
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.PadSelector
import dev.pepotech.pepomote.ui.components.ReconnectingLabel
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.components.ShoulderButton
import dev.pepotech.pepomote.ui.theme.PepoColors
import dev.pepotech.pepomote.ui.components.padMetrics
import dev.pepotech.pepomote.ui.components.rememberPressRegistry
import dev.pepotech.pepomote.ui.components.rememberScreenReader
import dev.pepotech.pepomote.ui.components.slideCanvas
import dev.pepotech.pepomote.control.AppPrefs
import kotlin.math.roundToInt
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

/**
 * GamePad de Wii U (modo Cemu), siempre apaisado. Como el mando real: L/ZL
 * arriba a la izquierda y R/ZR a la derecha; stick izquierdo y cruceta a la
 * izquierda; stick derecho y rombo A/B/X/Y a la derecha; la pantalla táctil
 * en el centro con −, Home, +, TV/Pad y Soplar debajo. Todo escalado al
 * tamaño real de la pantalla. Como Pro Controller (jugadores 2-4) no hay
 * táctil, ni Soplar, ni TV/Pad. Con el ajuste «GamePad sin pantalla» tampoco
 * hay táctil (ni doble pantalla) y los botones crecen.
 *
 * Se enseña de forma optimista: conectando («Conectando…») o esperando el
 * eco de `mode cemu` («Activando Wii U…») los controles van atenuados e
 * inertes y el motor sigue emitiendo 72 bytes. Con el modo confirmado, el
 * motor emite como GamePad (80 bytes, sensores remapeados al marco apaisado);
 * al salir se suelta todo y el motor vuelve a lo de antes.
 *
 * Doble pantalla: siendo el GamePad (Jugador 1, no Pro) y con el modo
 * confirmado se abre el canal de pantalla ([ScreenLink]) y la zona táctil
 * pinta la pantalla del GamePad de Cemu que manda el receptor. Se cierra al
 * salir, al pasar a Pro/Mando de Wii, al perder el enlace o al irse la app a
 * segundo plano.
 */
@Composable
fun GamePadScreen(link: UiLink, onDisconnect: () -> Unit) {
    val view = LocalView.current
    val connected = link as? UiLink.Connected
    val intent by LinkState.intent.collectAsState()
    val wantedMode = Route.displayMode(link, intent)
    val switchPad = wantedMode == LinkState.MODE_SWITCH
    val operative = Route.isGamePad(link) && connected?.mode == wantedMode
    val pro = switchPad || connected?.pad == LinkState.PAD_PRO
    val engine = LinkState.motion
    val rotation = rememberDisplayRotation()
    val screen by ScreenLink.client.collectAsState()
    // Primera vez: el lado del apaisado lo ha elegido el sensor; se pregunta
    // si es el bueno y se guarda para siempre (Ajustes lo cambia; aparte del
    // lado del mando + Nunchuk). Al salir sin confirmar, la prueba se olvida
    val sideSaved by GamePadSide.saved.collectAsState()
    val sideProvisional by GamePadSide.provisional.collectAsState()
    DisposableEffect(Unit) {
        onDispose { GamePadSide.setProvisional(null) }
    }

    // «Teclado»: texto para el teclado en pantalla de Cemu (GamePad y Pro)
    var keyboardOpen by remember { mutableStateOf(false) }

    DisposableEffect(Unit) {
        view.keepScreenOn = true
        onDispose {
            view.keepScreenOn = false
            ButtonState.reset() // nada queda pulsado, inclinado ni tocado
        }
    }

    // Solo con el modo confirmado el motor emite como GamePad; si el modo se
    // va (eco de otro modo, Mando de Wii) o se sale, vuelve lo de antes.
    DisposableEffect(engine, operative, wantedMode, connected?.pad) {
        ButtonState.reset()
        if (engine != null && operative) {
            val prevKind = engine.kind
            val prevRotation = engine.rotation
            engine.rotation = rotation
            engine.kind = if (switchPad) SenderKind.SWITCH else SenderKind.GAMEPAD
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
    LaunchedEffect(engine, rotation, operative) {
        if (operative) engine?.rotation = rotation
    }

    val context = LocalContext.current
    // Ajustes «GamePad sin pantalla táctil» y «pantalla completa»: se leen al
    // entrar (Ajustes es otra pantalla)
    val noScreenPref = remember { AppPrefs.gamePadNoScreen(context) }
    val fullScreenPref = remember { AppPrefs.gamePadFullScreen(context) }
    val fullScreenKb = remember { AppPrefs.gamePadFullScreenKeyboard(context) }
    // Doble pantalla: solo el GamePad (no un Pro Controller, que no tiene),
    // con el modo confirmado y sin el ajuste «GamePad sin pantalla»
    val wantScreen = operative && !switchPad && connected?.pad == LinkState.PAD_GAMEPAD && !noScreenPref
    // Pantalla completa: solo la pantalla de Cemu y el táctil (mando real en
    // el PC). Nunca sin el modo confirmado: el estado «Activando Wii U…»
    // sigue con cabecera y «Salir»
    val fullScreen = wantScreen && fullScreenPref

    // Receptor anterior a 1.6: no confirma «solo pantalla» ni en el ok ni
    // con el eco; se avisa una vez (a los 2 s, por si el eco llega tarde)
    var warnedOld by remember { mutableStateOf(false) }
    LaunchedEffect(fullScreen, connected?.screenOnly) {
        if (fullScreen && connected?.screenOnly == null && !warnedOld) {
            kotlinx.coroutines.delay(2000)
            warnedOld = true
            LinkState.publishNotice(context.getString(R.string.fullscreen_old_receiver))
        }
    }

    // Capa de pulsación de la pantalla: cada botón dice dónde está y, con
    // «Pulsar deslizando», el dedo pasa de uno a otro sin levantarlo
    val press = rememberPressRegistry()
    DisposableEffect(press) { onDispose { press.releaseAll() } }

    // Sin el modo confirmado (controles inertes) o a pantalla completa (no
    // hay botones) la capa no pulsa nada
    SideEffect { press.enabled = operative && !fullScreen }

    // La cabecera plegable avisa de si está desplegada (la pregunta del lado
    // ocupa el mismo sitio) y de cuánto mide, para poner la pregunta justo
    // debajo cuando la cabecera no se va a plegar sola
    var headerExpanded by remember { mutableStateOf(true) }
    var headerBox by remember { mutableStateOf(0.dp) }
    // Con lector de pantalla la cabecera se queda abierta hasta que la cierren:
    // ahí la pregunta no puede esperar a que se pliegue o no saldría nunca
    // Sin el eco del modo la tarjeta no se pliega sola (ver GamePadCollapsibleHeader)
    val autoCollapse = operative && HeaderCollapse.autoCollapses(link is UiLink.Connected, rememberScreenReader())

    CompositionLocalProvider(LocalPressRegistry provides press) {
        BoxWithConstraints(
            modifier = Modifier
                .fillMaxSize()
                .background(if (fullScreen) Color.Black else PepoColors.Background)
                .statusBarsPadding()
                .navigationBarsPadding()
                .displayCutoutPadding()
                .slideCanvas()
        ) {
            // Medidas (PadMetrics, las mismas que en iOS): en una tablet los topes
            // crecen con k y los pads llenan la columna; en cualquier móvil con
            // pantalla, lo de siempre; sin pantalla, en fila o apilado según cuál
            // dé pads más grandes
            val m = padMetrics(maxWidth.value, maxHeight.value, noScreenPref, pro, switchPad)
            val gap = m.gap.dp
            val k = m.k
            val headerH = m.headerH.dp
            val selectorH = m.selectorH.dp
            val bodyH = m.bodyH.dp
            val sideW = m.sideW.dp
            val shoulderH = m.shoulderH.dp
            val shoulderW = m.shoulderW.dp
            val padSize = m.padSize.dp
            val clickSize = m.clickSize.dp
            val faceBtn = m.faceBtn.dp
            val touchW = m.touchW.dp
            val touchH = m.touchH.dp

            // Tamaño que se pide al PC: como máximo la resolución nativa (854×480)
            // o, si la zona táctil es más estrecha, su ancho real en píxeles
            // físicos con el alto 16:9; en pantalla completa, el área entera
            // (estable: no depende de cada fotograma). Al salir (o cambiar) se
            // retira.
            val density = LocalDensity.current
            val touchPx = with(density) { touchW.roundToPx() }
            val (reqW, reqH) = if (fullScreen) {
                FullScreenMetrics.streamRequest(with(density) { maxWidth.roundToPx() }, with(density) { maxHeight.roundToPx() })
            } else {
                val w = minOf(ScreenClient.NATIVE_WIDTH, touchPx)
                w to w * 9 / 16
            }
            DisposableEffect(wantScreen, reqW, reqH) {
                if (wantScreen) {
                    ScreenLink.request(reqW, reqH)
                    onDispose { ScreenLink.release() }
                } else {
                    onDispose { }
                }
            }

            if (fullScreen) {
                FullScreenGamePad(screen, showKeyboard = fullScreenKb, onKeyboard = { keyboardOpen = true })
            } else Column(Modifier.fillMaxSize()) {
                // El hueco de la cabecera: la de verdad va al final del Box,
                // por encima de todo, y aquí solo se le guarda el sitio
                Spacer(Modifier.height(headerH))
                Spacer(Modifier.height(gap))
                // Qué mando soy en Cemu: debajo de la cabecera, centrado
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(selectorH),
                    contentAlignment = Alignment.Center
                ) {
                    if (connected != null && connected.mode == wantedMode) PadSelector(connected, compact = true)
                }
                Spacer(Modifier.height(gap))

                Box(
                    Modifier
                        .fillMaxWidth()
                        .height(bodyH)
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxSize()
                            .alpha(if (operative) 1f else 0.4f),
                        horizontalArrangement = Arrangement.spacedBy(gap)
                    ) {
                        // Izquierda: L y ZL en la esquina, stick (+ L3), cruceta. En
                        // fila: L3 junto a los gatillos y stick y cruceta uno al lado
                        // del otro, abajo
                        Column(
                            modifier = Modifier
                                .width(sideW)
                                .fillMaxHeight(),
                            horizontalAlignment = Alignment.Start,
                            verticalArrangement = Arrangement.Top
                        ) {
                            val shoulders: @Composable () -> Unit = {
                                Column(verticalArrangement = Arrangement.spacedBy(gap)) {
                                    ShoulderButton("L", ButtonState.L, shoulderW, shoulderH, textSize = (16 * k).roundToInt())
                                    ShoulderButton("ZL", ButtonState.ZL, shoulderW, shoulderH, textSize = (16 * k).roundToInt())
                                }
                            }
                            val click: @Composable () -> Unit = {
                                RoundButton("L3", clickSize, ButtonState.STICK_L, textSize = (12 * k).roundToInt())
                            }
                            val stick: @Composable () -> Unit = {
                                AnalogStick(padSize) { x, y -> ButtonState.setStick(x, y) }
                            }
                            if (m.row) {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(gap)
                                ) {
                                    shoulders()
                                    click()
                                }
                                Spacer(Modifier.weight(1f))
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(gap)
                                ) {
                                    stick()
                                    PadCross(sizeDp = padSize)
                                }
                            } else {
                                shoulders()
                                Spacer(Modifier.weight(1f))
                                Row(
                                    verticalAlignment = Alignment.Bottom,
                                    horizontalArrangement = Arrangement.spacedBy(gap)
                                ) {
                                    stick()
                                    click()
                                }
                                // Tablet: stick y cruceta juntos, abajo (donde llega el
                                // pulgar), en vez de repartidos por toda la altura
                                if (k > 1f) Spacer(Modifier.height(gap * 2)) else Spacer(Modifier.weight(1f))
                                PadCross(sizeDp = padSize)
                            }
                        }

                        // Centro: pantalla táctil (si la hay) y la fila − · Home · +
                        // (con TV/Pad y Soplar); en fila, una columna vertical con lo mismo
                        Column(
                            modifier = Modifier
                                .weight(1f)
                                .fillMaxHeight(),
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.Top
                        ) {
                            val rowGap = m.rowGap.dp
                            val pillW = m.pillW.dp
                            val pillH = m.pillH.dp
                            val roundBtn = m.roundBtn.dp
                            val minus: @Composable () -> Unit = { RoundButton("−", roundBtn, ButtonState.MINUS, textSize = (19 * k).roundToInt()) }
                            val home: @Composable () -> Unit = { RoundButton(stringResource(R.string.home_btn), roundBtn, ButtonState.HOME, textSize = (12 * k).roundToInt()) }
                            val plus: @Composable () -> Unit = { RoundButton("+", roundBtn, ButtonState.PLUS, textSize = (19 * k).roundToInt()) }
                            val tv: @Composable () -> Unit = { ShoulderButton(stringResource(R.string.tv_pad), ButtonState.SCREEN, pillW, pillH, textSize = (12 * k).roundToInt()) }
                            val blow: @Composable () -> Unit = { ShoulderButton(stringResource(R.string.blow), ButtonState.MIC, pillW, pillH, textSize = (12 * k).roundToInt()) }
                            val capture: @Composable () -> Unit = { ShoulderButton(stringResource(R.string.capture), ButtonState.SCREEN, pillW, pillH, textSize = (12 * k).roundToInt()) }
                            Spacer(Modifier.weight(1f))
                            if (m.row) {
                                Column(
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                    verticalArrangement = Arrangement.spacedBy(gap)
                                ) {
                                    minus()
                                    home()
                                    plus()
                                    if (switchPad) capture()
                                    if (!pro) {
                                        tv()
                                        blow()
                                    }
                                }
                            } else {
                                if (pro) {
                                    Text(
                                        stringResource(R.string.pro_controller),
                                        style = MaterialTheme.typography.bodyMedium,
                                        textAlign = TextAlign.Center
                                    )
                                    // Tablet: la fila pegada al texto (un bloque centrado)
                                    if (k > 1f) Spacer(Modifier.height(gap * 3)) else Spacer(Modifier.weight(1f))
                                } else if (!m.noScreen) {
                                    TouchScreen(touchW, touchH, screen)
                                    // Tablet: la fila pegada a la pantalla (un bloque centrado)
                                    if (k > 1f) Spacer(Modifier.height(gap * 3)) else Spacer(Modifier.weight(1f))
                                }
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(rowGap)
                                ) {
                                    if (switchPad) capture() else if (!pro) tv()
                                    minus()
                                    home()
                                    plus()
                                    if (!pro) blow()
                                }
                            }
                            Spacer(Modifier.weight(1f))
                        }

                        // Derecha: R y ZR en la esquina, (R3 +) stick, rombo A/B/X/Y.
                        // En fila: R3 junto a los gatillos y rombo y stick uno al lado
                        // del otro, abajo
                        Column(
                            modifier = Modifier
                                .width(sideW)
                                .fillMaxHeight(),
                            horizontalAlignment = Alignment.End,
                            verticalArrangement = Arrangement.Top
                        ) {
                            val shoulders: @Composable () -> Unit = {
                                Column(
                                    horizontalAlignment = Alignment.End,
                                    verticalArrangement = Arrangement.spacedBy(gap)
                                ) {
                                    ShoulderButton("R", ButtonState.R, shoulderW, shoulderH, textSize = (16 * k).roundToInt())
                                    ShoulderButton("ZR", ButtonState.ZR, shoulderW, shoulderH, textSize = (16 * k).roundToInt())
                                }
                            }
                            val click: @Composable () -> Unit = {
                                RoundButton("R3", clickSize, ButtonState.STICK_R, textSize = (12 * k).roundToInt())
                            }
                            val stick: @Composable () -> Unit = {
                                AnalogStick(padSize) { x, y -> ButtonState.setStick2(x, y) }
                            }
                            if (m.row) {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(gap)
                                ) {
                                    click()
                                    shoulders()
                                }
                                Spacer(Modifier.weight(1f))
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(gap)
                                ) {
                                    FaceButtons(padSize, faceBtn)
                                    stick()
                                }
                            } else {
                                shoulders()
                                Spacer(Modifier.weight(1f))
                                Row(
                                    verticalAlignment = Alignment.Bottom,
                                    horizontalArrangement = Arrangement.spacedBy(gap)
                                ) {
                                    click()
                                    stick()
                                }
                                if (k > 1f) Spacer(Modifier.height(gap * 2)) else Spacer(Modifier.weight(1f))
                                FaceButtons(padSize, faceBtn)
                            }
                        }
                    }

                    if (!operative) {
                        // Inerte hasta que el receptor confirme el modo: nada llega a los controles
                        Box(
                            Modifier
                                .matchParentSize()
                                .pointerInput(Unit) {
                                    awaitPointerEventScope {
                                        while (true) awaitPointerEvent()
                                    }
                                }
                        )
                    }
                }
            }

            // La pregunta del lado va donde la cabecera desplegada: se espera a
            // que se pliegue (si no, una encima de la otra) y, cuando no se
            // pliega sola, se pone justo bajo la tarjeta. A pantalla completa
            // no hay cabecera y la pregunta sale sin más
            if (sideSaved == LandscapeSide.Unset && (fullScreen || !headerExpanded || !autoCollapse)) {
                val shown = LandscapeSide.effective(LandscapeSide.current(rotation), sideProvisional)
                SideAskCard(
                    modifier = Modifier
                        .align(Alignment.TopCenter)
                        .padding(
                            top = when {
                                fullScreen -> 8.dp
                                headerExpanded -> headerBox + 4.dp
                                else -> headerH + gap
                            }
                        ),
                    title = stringResource(if (switchPad) R.string.side_ask_switch else R.string.side_ask_gamepad),
                    onFlip = { GamePadSide.setProvisional(shown.flipped()) },
                    onKeep = { GamePadSide.save(context, shown) }
                )
            }

            NoticeBanner(
                Modifier
                    .align(Alignment.TopCenter)
                    .padding(top = if (fullScreen) 8.dp else headerH + selectorH + gap * 2)
            )

            if (keyboardOpen && (link as? UiLink.Connected)?.textInput != false) {
                KeyboardDialog(
                    onSend = { LinkState.sendText?.invoke(it) },
                    onClose = { keyboardOpen = false },
                    switchPad = switchPad
                )
            }

            // Cabecera plegable, la última del Box: desplegada tapa al
            // selector, a los controles y a los avisos (y no al revés). A
            // pantalla completa no hay cabecera: manda el mando del PC
            if (!fullScreen) {
                GamePadCollapsibleHeader(
                    link = link,
                    operative = operative,
                    wantedMode = wantedMode,
                    screen = screen,
                    onKeyboard = if (operative && (link as? UiLink.Connected)?.textInput != false) {
                        { keyboardOpen = true }
                    } else null,
                    onExpandedChange = { headerExpanded = it },
                    onDisconnect = onDisconnect,
                    modifier = Modifier
                        .align(Alignment.TopCenter)
                        .onSizeChanged { headerBox = with(density) { it.height.toDp() } }
                )
            }
        }
    }
}

/**
 * «Pantalla del GamePad a pantalla completa»: solo la pantalla de Cemu,
 * ajustada a su proporción real sobre fondo negro, con el táctil sobre la
 * imagen (un gesto que empieza en las bandas negras se ignora) y, si el
 * ajuste lo pide, el botón de teclado arriba a la derecha. Sin sticks ni
 * botones: el mando real va en el PC. Atrás vuelve a Inicio.
 */
@Composable
private fun FullScreenGamePad(screen: ScreenClient<Bitmap>?, showKeyboard: Boolean, onKeyboard: () -> Unit) {
    Box(Modifier.fillMaxSize()) {
        FullScreenTouch(screen, Modifier.fillMaxSize())
        if (showKeyboard) {
            KeyboardButton(
                compact = true,
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(8.dp)
                    .alpha(0.7f),
                onClick = onKeyboard
            )
        }
    }
}

/**
 * La pantalla del GamePad a pantalla completa: mismo dibujado y mismo ACK de
 * fotograma que [TouchScreen], pero el rectángulo de la imagen (16:9 mientras
 * no hay fotograma) lo da [FullScreenMetrics] y el táctil se mapea sobre él.
 */
@Composable
private fun FullScreenTouch(screen: ScreenClient<Bitmap>?, modifier: Modifier) {
    val view = LocalView.current
    var finger by remember { mutableStateOf<Offset?>(null) }
    val dot = 14.dp
    val image = screen?.image?.collectAsState()
    val status = screen?.status?.collectAsState()
    val hasImage by remember(image) { derivedStateOf { image?.value != null } }
    val paint = remember { Paint().apply { isFilterBitmap = true } }
    val dst = remember { RectF() }
    fun rectFor(w: Float, h: Float): FitRect {
        val bmp = image?.value?.bitmap
        return FullScreenMetrics.fitRect(w, h, bmp?.width?.toFloat() ?: 0f, bmp?.height?.toFloat() ?: 0f)
    }

    Box(
        modifier = modifier
            .background(Color.Black)
            .drawBehind {
                val img = image?.value ?: return@drawBehind
                val r = rectFor(size.width, size.height)
                if (r.w <= 0f || r.h <= 0f) return@drawBehind
                dst.set(r.x, r.y, r.x + r.w, r.y + r.h)
                drawIntoCanvas { it.nativeCanvas.drawBitmap(img.bitmap, null, dst, paint) }
                screen?.shown(img.seq)
            }
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown()
                    val r = rectFor(size.width.toFloat(), size.height.toFloat())
                    if (!FullScreenMetrics.contains(r, down.position.x, down.position.y)) {
                        // en las bandas negras: no es un toque en la pantalla del GamePad
                        return@awaitEachGesture
                    }
                    fun report(pos: Offset, isDown: Boolean) {
                        val (fx, fy) = FullScreenMetrics.fraction(r, pos.x, pos.y)
                        ButtonState.setTouch(fx, fy, isDown)
                    }
                    down.consume()
                    view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                    var last = down.position
                    finger = last
                    report(last, true)
                    drag(down.id) { change ->
                        change.consume()
                        last = change.position
                        finger = last
                        report(last, true)
                    }
                    finger = null
                    report(last, false)
                }
            },
        contentAlignment = Alignment.Center
    ) {
        if (!hasImage) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text(
                    stringResource(R.string.touch_screen),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PepoColors.TextDim
                )
                if (screen != null) {
                    Text(
                        screenStatusText(status?.value),
                        style = MaterialTheme.typography.bodyMedium.copy(fontSize = 12.sp),
                        color = PepoColors.TextDim,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.padding(top = 2.dp, start = 12.dp, end = 12.dp)
                    )
                }
            }
        }
        finger?.let { p ->
            val half = with(LocalDensity.current) { (dot / 2).toPx() }
            Box(
                Modifier
                    .align(Alignment.TopStart)
                    .offset { IntOffset((p.x - half).roundToInt(), (p.y - half).roundToInt()) }
                    .size(dot)
                    .background(PepoColors.Blue, CircleShape)
            )
        }
    }
}

/**
 * Cabecera plegable del GamePad (Wii U, Pro Controller y Switch): en la banda
 * de cabecera, centrada, la pastilla con el modo («Wii U», «Switch») y, a su
 * lado, «Teclado», que se ve siempre y está a un toque mientras se juega.
 *
 * Al tocar la pastilla se despliega la tarjeta con lo que antes iba apretado
 * en una línea: el PC y «Salir», el estado («J1 · GamePad · 23 ms», o
 * «Activando Wii U…») con el ritmo de la doble pantalla, y los chips de modo
 * del Jugador 1. El teclado no entra en la tarjeta: se queda fuera.
 */
@Composable
internal fun GamePadCollapsibleHeader(
    link: UiLink,
    operative: Boolean,
    wantedMode: String,
    screen: ScreenClient<Bitmap>?,
    onKeyboard: (() -> Unit)?,
    onExpandedChange: (Boolean) -> Unit,
    onDisconnect: () -> Unit,
    modifier: Modifier = Modifier
) {
    CollapsibleHeader(
        link = link,
        label = GamePadHeaderText.handle(
            connected = link is UiLink.Connected,
            connecting = link is UiLink.Connecting,
            // el modo que se quiere: Wii U ya se lee aunque falte el eco
            mode = modeLabel(wantedMode),
            connectingText = stringResource(R.string.status_connecting),
            disconnectedText = stringResource(R.string.status_disconnected)
        ),
        modifier = modifier,
        // los 44 dp de la caja táctil de la pastilla son justo la banda de
        // cabecera (38) y su hueco (6): nada de debajo se mueve
        topPadding = 0.dp,
        beside = {
            // Texto para el teclado en pantalla de Cemu (GamePad y Pro): al
            // lado de la pastilla, nunca dentro de la tarjeta. Con la letra
            // grande no puede estirar la fila más allá de la banda (44 dp)
            if (onKeyboard != null) {
                KeyboardButton(compact = true, modifier = Modifier.heightIn(max = 44.dp), onClick = onKeyboard)
            }
        },
        onExpandedChange = onExpandedChange,
        // Esperando el eco del modo el mando está inerte: la tarjeta se queda
        // (es lo que explica el «Activando…» y donde están los chips y Salir)
        autoCollapse = operative
    ) {
        GamePadHeaderCard(link, operative, wantedMode, screen, onDisconnect)
    }
}

/** Las filas de la tarjeta del GamePad, una debajo de otra. */
@Composable
private fun GamePadHeaderCard(
    link: UiLink,
    operative: Boolean,
    wantedMode: String,
    screen: ScreenClient<Bitmap>?,
    onDisconnect: () -> Unit
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(20.dp)
    ) {
        if (link is UiLink.Reconnecting) {
            ReconnectingLabel(link, MaterialTheme.typography.bodyMedium)
        } else {
            Text(
                when (link) {
                    is UiLink.Connected -> link.pcName
                    is UiLink.Connecting -> stringResource(R.string.status_connecting)
                    else -> stringResource(R.string.status_disconnected)
                },
                style = MaterialTheme.typography.bodyMedium,
                // un nombre de PC largo no echa a «Salir» fuera de la tarjeta
                modifier = Modifier.widthIn(max = 160.dp),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
        TextButton(onClick = onDisconnect) {
            Text(stringResource(R.string.exit), color = PepoColors.Error, style = MaterialTheme.typography.bodyMedium)
        }
    }
    if (link is UiLink.Connected) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            val padName = if (wantedMode == LinkState.MODE_SWITCH || link.pad == LinkState.PAD_PRO)
                stringResource(R.string.pro_controller) else stringResource(R.string.gamepad)
            Text(
                GamePadHeaderText.status(
                    operative = operative,
                    player = link.player,
                    padName = padName,
                    rttMs = link.rttMs,
                    activating = stringResource(
                        if (wantedMode == LinkState.MODE_SWITCH) R.string.activating_switch else R.string.activating_wiiu
                    )
                ),
                style = MaterialTheme.typography.bodyMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            // Barato: el cliente lo publica una vez por segundo
            val fps = screen?.fps?.collectAsState()?.value ?: 0
            if (fps > 0) {
                Text(
                    stringResource(R.string.screen_fps, fps),
                    style = MaterialTheme.typography.bodyMedium.copy(fontSize = 12.sp),
                    maxLines = 1
                )
            }
        }
        if (link.slot == 0) {
            // Mientras se espera el eco, Wii U ya va marcado (es lo pedido)
            ModeChips(
                current = if (operative) link.mode else wantedMode,
                supportsCemu = link.supportsCemu,
                supportsSwitch = link.supportsSwitch,
                androidReceiver = link.platform == "android",
                compact = true
            )
        }
    }
}

/**
 * Rombo: X arriba, Y izquierda, A derecha (azul, destacado), B abajo. El
 * texto crece con el botón (18 y 20 sp en los 44 dp de un móvil).
 */
@Composable
internal fun FaceButtons(size: Dp, btn: Dp) {
    val text = (btn.value * 18f / 44f).roundToInt()
    Box(Modifier.size(size)) {
        Box(Modifier.align(BiasAlignment(0f, -1f))) {
            RoundButton("X", btn, ButtonState.X, textSize = text)
        }
        Box(Modifier.align(BiasAlignment(-1f, 0f))) {
            RoundButton("Y", btn, ButtonState.Y, textSize = text)
        }
        Box(Modifier.align(BiasAlignment(1f, 0f))) {
            RoundButton(
                "A", btn, ButtonState.A,
                background = PepoColors.Blue,
                pressedColor = PepoColors.BlueHover,
                textColor = PepoColors.OnAccent,
                textSize = (btn.value * 20f / 44f).roundToInt(),
                pop = true
            )
        }
        Box(Modifier.align(BiasAlignment(0f, 1f))) {
            RoundButton("B", btn, ButtonState.B, textSize = text)
        }
    }
}

/**
 * Pantalla táctil del GamePad: rectángulo 16:9 con marco fino. Un solo
 * dedo; mientras esté, su posición dentro del rectángulo (recortada a 0..1)
 * va como fracción 0..65535 con FLAG_TOUCH; al soltar, sin dedo.
 *
 * Con el canal de pantalla abierto pinta la imagen del GamePad de Cemu
 * ocupando la zona entera (la fuente es 16:9; si no lo fuera, letterbox
 * centrado), con filtro lineal y solo en la fase de dibujo: cada imagen
 * invalida el dibujo, no la composición. Al pintarla avisa al cliente
 * ([ScreenClient.shown]) y entonces sale la confirmación al receptor. Sin
 * imagen: la etiqueta de siempre y la última línea de estado del receptor
 * (o «Conectando la pantalla…»). El mapeo táctil es el mismo con o sin imagen.
 */
@Composable
private fun TouchScreen(width: Dp, height: Dp, screen: ScreenClient<Bitmap>?) {
    val view = LocalView.current
    var finger by remember { mutableStateOf<Offset?>(null) }
    val shape = RoundedCornerShape(10.dp)
    val dot = 14.dp
    val image = screen?.image?.collectAsState()
    val status = screen?.status?.collectAsState()
    val hasImage by remember(image) { derivedStateOf { image?.value != null } }
    val paint = remember { Paint().apply { isFilterBitmap = true } }
    val dst = remember { RectF() }

    Box(
        modifier = Modifier
            .size(width, height)
            .clip(shape)
            .background(PepoColors.Card, shape)
            .drawBehind {
                val img = image?.value ?: return@drawBehind
                val bmp = img.bitmap
                val bw = bmp.width.toFloat()
                val bh = bmp.height.toFloat()
                if (bw <= 0f || bh <= 0f) return@drawBehind
                // Escalada a la zona conservando la proporción; un 16:9 de
                // verdad la cubre entera (sin hilo de fondo por el redondeo)
                val scale = minOf(size.width / bw, size.height / bh)
                val dw = (bw * scale).let { if (it > size.width - 1f) size.width else it }
                val dh = (bh * scale).let { if (it > size.height - 1f) size.height else it }
                val left = (size.width - dw) / 2f
                val top = (size.height - dh) / 2f
                dst.set(left, top, left + dw, top + dh)
                drawIntoCanvas { it.nativeCanvas.drawBitmap(bmp, null, dst, paint) }
                screen?.shown(img.seq)
            }
            .border(1.5.dp, if (finger != null) PepoColors.Glow else PepoColors.CardBorder, shape)
            .pointerInput(Unit) {
                awaitEachGesture {
                    fun report(pos: Offset, down: Boolean) {
                        val fx = (pos.x / size.width).coerceIn(0f, 1f)
                        val fy = (pos.y / size.height).coerceIn(0f, 1f)
                        ButtonState.setTouch((fx * 65535f).roundToInt(), (fy * 65535f).roundToInt(), down)
                    }
                    val down = awaitFirstDown()
                    down.consume()
                    view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                    var last = down.position
                    finger = last
                    report(last, true)
                    drag(down.id) { change ->
                        change.consume()
                        last = change.position
                        finger = last
                        report(last, true)
                    }
                    finger = null
                    report(last, false)
                }
            },
        contentAlignment = Alignment.Center
    ) {
        if (!hasImage) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text(stringResource(R.string.touch_screen), style = MaterialTheme.typography.bodyMedium)
                if (screen != null) {
                    Text(
                        screenStatusText(status?.value),
                        style = MaterialTheme.typography.bodyMedium.copy(fontSize = 12.sp),
                        textAlign = TextAlign.Center,
                        modifier = Modifier.padding(top = 2.dp, start = 12.dp, end = 12.dp)
                    )
                }
            }
        }
        finger?.let { p ->
            val half = with(LocalDensity.current) { (dot / 2).toPx() }
            Box(
                Modifier
                    .align(Alignment.TopStart)
                    .offset { IntOffset((p.x - half).roundToInt(), (p.y - half).roundToInt()) }
                    .size(dot)
                    .background(PepoColors.Blue, CircleShape)
            )
        }
    }
}

/**
 * Rotación actual de la pantalla (`Surface.ROTATION_*`). Sigue tanto el
 * cambio de configuración (vertical → apaisado al entrar) como el giro de
 * 180° entre los dos apaisados, que no cambia la configuración pero sí la
 * pantalla (DisplayManager lo avisa).
 */
@Composable
internal fun rememberDisplayRotation(): Int {
    val view = LocalView.current
    val context = LocalContext.current
    val configuration = LocalConfiguration.current
    var rotation by remember { mutableIntStateOf(view.display?.rotation ?: Surface.ROTATION_0) }

    LaunchedEffect(configuration) {
        rotation = view.display?.rotation ?: rotation
    }
    DisposableEffect(view) {
        val dm = context.getSystemService(Context.DISPLAY_SERVICE) as DisplayManager
        val listener = object : DisplayManager.DisplayListener {
            override fun onDisplayAdded(displayId: Int) = Unit
            override fun onDisplayRemoved(displayId: Int) = Unit
            override fun onDisplayChanged(displayId: Int) {
                rotation = view.display?.rotation ?: rotation
            }
        }
        dm.registerDisplayListener(listener, Handler(Looper.getMainLooper()))
        onDispose { dm.unregisterDisplayListener(listener) }
    }
    return rotation
}

/** Estado del canal de pantalla, traducido (los centinelas de [ScreenClient]). */
@Composable
private fun screenStatusText(status: String?): String = when {
    status == null -> stringResource(R.string.screen_connecting)
    status == ScreenClient.STATUS_UNAVAILABLE -> stringResource(R.string.screen_unavailable)
    status == ScreenClient.STATUS_RECONNECTING -> stringResource(R.string.screen_reconnecting)
    status.startsWith(ScreenClient.STATUS_DETAIL) -> stringResource(R.string.screen_unavailable_detail, status.removePrefix(ScreenClient.STATUS_DETAIL))
    else -> status
}
