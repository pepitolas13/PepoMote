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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
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
import androidx.compose.ui.layout.layoutId
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
import dev.pepotech.pepomote.ui.components.FitRect
import dev.pepotech.pepomote.ui.components.FullScreenMetrics
import dev.pepotech.pepomote.ui.components.HeaderSlot
import dev.pepotech.pepomote.ui.components.KeyboardButton
import dev.pepotech.pepomote.ui.components.KeyboardDialog
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.PadSelector
import dev.pepotech.pepomote.ui.components.PriorityRow
import dev.pepotech.pepomote.ui.components.ReconnectingLabel
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.components.ShoulderButton
import dev.pepotech.pepomote.ui.theme.PepoColors
import dev.pepotech.pepomote.ui.components.padMetrics
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
    val operative = Route.isGamePad(link)
    val pro = connected?.pad == LinkState.PAD_PRO
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
    DisposableEffect(engine, operative) {
        if (engine != null && operative) {
            val prevKind = engine.kind
            val prevRotation = engine.rotation
            engine.rotation = rotation
            engine.kind = SenderKind.GAMEPAD
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
    val wantScreen = operative && connected?.pad == LinkState.PAD_GAMEPAD && !noScreenPref
    // Pantalla completa: solo la pantalla de Cemu y el táctil (mando real en
    // el PC). Nunca sin el modo confirmado: el estado «Activando Wii U…»
    // sigue con cabecera y «Salir»
    val fullScreen = wantScreen && fullScreenPref

    // Receptor anterior a 1.5.53: no confirma «solo pantalla» ni en el ok ni
    // con el eco; se avisa una vez (a los 2 s, por si el eco llega tarde)
    var warnedOld by remember { mutableStateOf(false) }
    LaunchedEffect(fullScreen, connected?.screenOnly) {
        if (fullScreen && connected?.screenOnly == null && !warnedOld) {
            kotlinx.coroutines.delay(2000)
            warnedOld = true
            LinkState.publishNotice(context.getString(R.string.fullscreen_old_receiver))
        }
    }

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(if (fullScreen) Color.Black else PepoColors.Background)
            .statusBarsPadding()
            .navigationBarsPadding()
            .displayCutoutPadding()
    ) {
        // Medidas (PadMetrics, las mismas que en iOS): en una tablet los topes
        // crecen con k y los pads llenan la columna; en cualquier móvil con
        // pantalla, lo de siempre; sin pantalla, en fila o apilado según cuál
        // dé pads más grandes
        val m = padMetrics(maxWidth.value, maxHeight.value, noScreenPref, pro)
        val gap = m.gap.dp
        val screenW = maxWidth
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
            Header(
                link, operative, headerH, screen,
                width = screenW,
                onKeyboard = if (operative) {
                    { keyboardOpen = true }
                } else null,
                onDisconnect = onDisconnect
            )
            Spacer(Modifier.height(gap))
            // Qué mando soy en Cemu: debajo de la cabecera, centrado
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(selectorH),
                contentAlignment = Alignment.Center
            ) {
                if (connected != null) PadSelector(connected, compact = true)
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
                        Spacer(Modifier.weight(1f))
                        if (m.row) {
                            Column(
                                horizontalAlignment = Alignment.CenterHorizontally,
                                verticalArrangement = Arrangement.spacedBy(gap)
                            ) {
                                minus()
                                home()
                                plus()
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
                                if (!pro) tv()
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

        if (sideSaved == LandscapeSide.Unset) {
            val shown = LandscapeSide.effective(LandscapeSide.current(rotation), sideProvisional)
            SideAskCard(
                modifier = Modifier
                    .align(Alignment.TopCenter)
                    .padding(top = if (fullScreen) 8.dp else headerH + gap),
                title = stringResource(R.string.side_ask_gamepad),
                onFlip = { GamePadSide.setProvisional(shown.flipped()) },
                onKeep = { GamePadSide.save(context, shown) }
            )
        }

        NoticeBanner(
            Modifier
                .align(Alignment.TopCenter)
                .padding(top = if (fullScreen) 8.dp else headerH + selectorH + gap * 2)
        )

        if (keyboardOpen) {
            KeyboardDialog(
                onSend = { LinkState.sendText?.invoke(it) },
                onClose = { keyboardOpen = false }
            )
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
 * Cabecera compacta: PC · «J1 · GamePad» / «J2 · Pro Controller» · ritmo de
 * la doble pantalla («pantalla · 30 fps», media de 1 s) · chips de modo
 * (Jugador 1) · «Teclado» (con el modo confirmado) · Salir.
 *
 * Lo que no cabe lo decide [PriorityRow]: primero cae el ritmo de la pantalla,
 * luego el nombre del PC, luego el estado (que antes se encoge con puntos
 * suspensivos) y, en último extremo, los chips y «Teclado»; «Salir» nunca.
 */
@Composable
private fun Header(
    link: UiLink,
    operative: Boolean,
    height: Dp,
    screen: ScreenClient<Bitmap>?,
    width: Dp,
    onKeyboard: (() -> Unit)?,
    onDisconnect: () -> Unit
) {
    // El estado se acorta antes de recortarse: en móviles estrechos fuera el
    // nombre del mando y el RTT («J1 · GamePad · 23 ms» → «J1»)
    val showPad = width >= 560.dp
    val showRtt = width >= 700.dp
    PriorityRow(
        modifier = Modifier
            .fillMaxWidth()
            .height(height)
            .padding(horizontal = 4.dp),
        spacing = 8.dp
    ) {
        when (link) {
            is UiLink.Connected -> {
                Text(
                    link.pcName,
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier
                        .layoutId(HeaderSlot.Pc)
                        .widthIn(max = 160.dp)
                )
                val padName = if (link.pad == LinkState.PAD_PRO) stringResource(R.string.pro_controller) else stringResource(R.string.gamepad)
                val activating = stringResource(R.string.activating_wiiu)
                Text(
                    if (operative) buildString {
                        append("J${link.player}")
                        if (showPad) {
                            append(" · ")
                            append(padName)
                        }
                        if (showRtt) link.rttMs?.let { append(" · ${"%.0f".format(it)} ms") }
                    } else activating,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.layoutId(HeaderSlot.Status)
                )
                // Barato: el cliente lo publica una vez por segundo
                val fps = screen?.fps?.collectAsState()?.value ?: 0
                if (fps > 0) {
                    Text(
                        stringResource(R.string.screen_fps, fps),
                        style = MaterialTheme.typography.bodyMedium.copy(fontSize = 12.sp),
                        maxLines = 1,
                        modifier = Modifier.layoutId(HeaderSlot.Fps)
                    )
                }
                if (link.slot == 0) {
                    // Mientras se espera el eco, Wii U ya va marcado (es lo pedido)
                    ModeChips(
                        current = if (operative) link.mode else LinkState.MODE_CEMU,
                        supportsCemu = link.supportsCemu,
                        compact = true,
                        modifier = Modifier.layoutId(HeaderSlot.Chips)
                    )
                }
                // Texto para el teclado en pantalla de Cemu (GamePad y Pro)
                if (onKeyboard != null) {
                    KeyboardButton(compact = true, modifier = Modifier.layoutId(HeaderSlot.Keyboard), onClick = onKeyboard)
                }
            }

            is UiLink.Connecting -> Text(
                stringResource(R.string.status_connecting),
                style = MaterialTheme.typography.titleMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.layoutId(HeaderSlot.Status)
            )

            is UiLink.Reconnecting -> ReconnectingLabel(link, modifier = Modifier.layoutId(HeaderSlot.Status))

            else -> Text(
                stringResource(R.string.status_disconnected),
                style = MaterialTheme.typography.titleMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.layoutId(HeaderSlot.Status)
            )
        }
        TextButton(onClick = onDisconnect, modifier = Modifier.layoutId(HeaderSlot.Exit)) {
            Text(stringResource(R.string.exit), color = PepoColors.Error, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

/**
 * Rombo: X arriba, Y izquierda, A derecha (azul, destacado), B abajo. El
 * texto crece con el botón (18 y 20 sp en los 44 dp de un móvil).
 */
@Composable
private fun FaceButtons(size: Dp, btn: Dp) {
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
