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
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
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
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.Route
import dev.pepotech.pepomote.service.ScreenLink
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.AnalogStick
import dev.pepotech.pepomote.ui.components.KeyboardButton
import dev.pepotech.pepomote.ui.components.KeyboardDialog
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.PadSelector
import dev.pepotech.pepomote.ui.components.ReconnectingLabel
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.components.ShoulderButton
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlin.math.roundToInt

/**
 * GamePad de Wii U (modo Cemu), siempre apaisado. Como el mando real: L/ZL
 * arriba a la izquierda y R/ZR a la derecha; stick izquierdo y cruceta a la
 * izquierda; stick derecho y rombo A/B/X/Y a la derecha; la pantalla táctil
 * en el centro con −, Home, +, TV/Pad y Soplar debajo. Todo escalado al
 * tamaño real de la pantalla. Como Pro Controller (jugadores 2-4) no hay
 * táctil, ni Soplar, ni TV/Pad.
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

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .navigationBarsPadding()
            .displayCutoutPadding()
    ) {
        val gap = 6.dp
        val screenW = maxWidth
        val headerH = 38.dp
        val selectorH = 36.dp
        val bodyH = maxHeight - headerH - selectorH - gap * 3
        val sideW = maxWidth * 0.29f
        val shoulderH = (bodyH * 0.09f).coerceIn(26.dp, 40.dp)
        val shoulderW = (sideW * 0.6f).coerceIn(90.dp, 150.dp)
        // Stick y cruceta (o rombo) se reparten lo que queda bajo los gatillos
        val padSize = ((bodyH - shoulderH * 2 - gap * 3) / 2f)
            .coerceAtMost(sideW * 0.62f)
            .coerceAtMost(200.dp)
        val clickSize = (padSize * 0.30f).coerceIn(30.dp, 44.dp)
        val faceBtn = padSize / 2.6f
        val centerW = maxWidth - sideW * 2 - gap * 2
        val bottomRowH = (bodyH * 0.16f).coerceIn(44.dp, 60.dp)
        val touchW = minOf(centerW, (bodyH - bottomRowH - gap * 2) * (16f / 9f))
        val touchH = touchW * (9f / 16f)

        // Doble pantalla: solo el GamePad (no un Pro Controller, que no tiene)
        // y con el modo confirmado. Se pide como máximo la resolución nativa
        // (854×480) o, si la zona táctil es más estrecha, su ancho real en
        // píxeles físicos con el alto 16:9. Al salir (o cambiar) se retira.
        val wantScreen = operative && connected?.pad == LinkState.PAD_GAMEPAD
        val touchPx = with(LocalDensity.current) { touchW.roundToPx() }
        // En pantallas estrechas el contador de fps de la cabecera no cabe con todo lo demás
        val showFps = maxWidth >= 760.dp
        DisposableEffect(wantScreen, touchPx) {
            if (wantScreen) {
                val w = minOf(ScreenClient.NATIVE_WIDTH, touchPx)
                ScreenLink.request(w, w * 9 / 16)
                onDispose { ScreenLink.release() }
            } else {
                onDispose { }
            }
        }

        Column(Modifier.fillMaxSize()) {
            Header(
                link, operative, headerH, screen,
                width = screenW,
                showFps = showFps,
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
                    // Izquierda: L y ZL en la esquina, stick (+ L3), cruceta
                    Column(
                        modifier = Modifier
                            .width(sideW)
                            .fillMaxHeight(),
                        horizontalAlignment = Alignment.Start,
                        verticalArrangement = Arrangement.SpaceBetween
                    ) {
                        Column(verticalArrangement = Arrangement.spacedBy(gap)) {
                            ShoulderButton("L", ButtonState.L, shoulderW, shoulderH)
                            ShoulderButton("ZL", ButtonState.ZL, shoulderW, shoulderH)
                        }
                        Row(
                            verticalAlignment = Alignment.Bottom,
                            horizontalArrangement = Arrangement.spacedBy(gap)
                        ) {
                            AnalogStick(padSize) { x, y -> ButtonState.setStick(x, y) }
                            RoundButton("L3", clickSize, ButtonState.STICK_L, textSize = 12)
                        }
                        PadCross(sizeDp = padSize)
                    }

                    // Centro: pantalla táctil y la fila − · Home · + (con TV/Pad y Soplar)
                    Column(
                        modifier = Modifier
                            .weight(1f)
                            .fillMaxHeight(),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.SpaceEvenly
                    ) {
                        if (pro) {
                            Text(
                                "Pro Controller",
                                style = MaterialTheme.typography.bodyMedium,
                                textAlign = TextAlign.Center
                            )
                        } else {
                            TouchScreen(touchW, touchH, screen)
                        }
                        // La fila entera tiene que caber en el centro (móviles
                        // estrechos): pastillas y círculos se encogen juntos
                        val rowGap = if (centerW < 300.dp) 6.dp else 10.dp
                        val pillW = if (pro) 0.dp else (centerW * 0.22f).coerceIn(44.dp, 66.dp)
                        val pills = if (pro) 0 else 2
                        val roundBtn = ((centerW - rowGap * (2 + pills) - pillW * pills) / 3f)
                            .coerceAtMost(bottomRowH * 0.85f)
                            .coerceAtLeast(28.dp)
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(rowGap)
                        ) {
                            if (!pro) ShoulderButton("TV/Pad", ButtonState.SCREEN, pillW, 30.dp, textSize = 12)
                            RoundButton("−", roundBtn, ButtonState.MINUS, textSize = 19)
                            RoundButton("Home", roundBtn, ButtonState.HOME, textSize = 12)
                            RoundButton("+", roundBtn, ButtonState.PLUS, textSize = 19)
                            if (!pro) ShoulderButton("Soplar", ButtonState.MIC, pillW, 30.dp, textSize = 12)
                        }
                    }

                    // Derecha: R y ZR en la esquina, (R3 +) stick, rombo A/B/X/Y
                    Column(
                        modifier = Modifier
                            .width(sideW)
                            .fillMaxHeight(),
                        horizontalAlignment = Alignment.End,
                        verticalArrangement = Arrangement.SpaceBetween
                    ) {
                        Column(
                            horizontalAlignment = Alignment.End,
                            verticalArrangement = Arrangement.spacedBy(gap)
                        ) {
                            ShoulderButton("R", ButtonState.R, shoulderW, shoulderH)
                            ShoulderButton("ZR", ButtonState.ZR, shoulderW, shoulderH)
                        }
                        Row(
                            verticalAlignment = Alignment.Bottom,
                            horizontalArrangement = Arrangement.spacedBy(gap)
                        ) {
                            RoundButton("R3", clickSize, ButtonState.STICK_R, textSize = 12)
                            AnalogStick(padSize) { x, y -> ButtonState.setStick2(x, y) }
                        }
                        FaceButtons(padSize, faceBtn)
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

        NoticeBanner(
            Modifier
                .align(Alignment.TopCenter)
                .padding(top = headerH + selectorH + gap * 2)
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
 * Cabecera compacta: PC · «J1 · GamePad» / «J2 · Pro Controller» · ritmo de
 * la doble pantalla («pantalla · 30 fps», media de 1 s, si [showFps]) ·
 * chips de modo (Jugador 1) · «Teclado» (con el modo confirmado) · Salir.
 */
@Composable
private fun Header(
    link: UiLink,
    operative: Boolean,
    height: Dp,
    screen: ScreenClient<Bitmap>?,
    width: Dp,
    showFps: Boolean,
    onKeyboard: (() -> Unit)?,
    onDisconnect: () -> Unit
) {
    // En móviles estrechos la cabecera no cabe entera: primero cae el nombre
    // del PC, luego el nombre del mando y el RTT (los chips, el teclado y
    // Salir se quedan siempre)
    val showPc = width >= 640.dp
    val showPad = width >= 560.dp
    val showRtt = width >= 700.dp
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(height)
            .padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        when (link) {
            is UiLink.Connected -> {
                if (showPc) {
                    Text(
                        link.pcName,
                        style = MaterialTheme.typography.titleMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.widthIn(max = 160.dp)
                    )
                }
                Text(
                    if (operative) buildString {
                        append("J${link.player}")
                        if (showPad) {
                            append(" · ")
                            append(if (link.pad == LinkState.PAD_PRO) "Pro Controller" else "GamePad")
                        }
                        if (showRtt) link.rttMs?.let { append(" · ${"%.0f".format(it)} ms") }
                    } else "Activando Wii U…",
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1
                )
                // Barato: el cliente lo publica una vez por segundo
                val fps = screen?.fps?.collectAsState()?.value ?: 0
                if (showFps && fps > 0) {
                    Text(
                        "pantalla · $fps fps",
                        style = MaterialTheme.typography.bodyMedium.copy(fontSize = 12.sp),
                        maxLines = 1
                    )
                }
                Spacer(Modifier.weight(1f))
                if (link.slot == 0) {
                    // Mientras se espera el eco, Wii U ya va marcado (es lo pedido)
                    ModeChips(
                        current = if (operative) link.mode else LinkState.MODE_CEMU,
                        supportsCemu = link.supportsCemu,
                        compact = true
                    )
                }
                // Texto para el teclado en pantalla de Cemu (GamePad y Pro)
                if (onKeyboard != null) KeyboardButton(compact = true, onClick = onKeyboard)
            }

            is UiLink.Connecting -> {
                Text("Conectando…", style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.weight(1f))
            }

            is UiLink.Reconnecting -> {
                ReconnectingLabel(link)
                Spacer(Modifier.weight(1f))
            }

            else -> {
                Text("Sin conexión", style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.weight(1f))
            }
        }
        TextButton(onClick = onDisconnect) {
            Text("Salir", color = PepoColors.Error, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

/** Rombo: X arriba, Y izquierda, A derecha (azul, destacado), B abajo. */
@Composable
private fun FaceButtons(size: Dp, btn: Dp) {
    Box(Modifier.size(size)) {
        Box(Modifier.align(BiasAlignment(0f, -1f))) {
            RoundButton("X", btn, ButtonState.X, textSize = 18)
        }
        Box(Modifier.align(BiasAlignment(-1f, 0f))) {
            RoundButton("Y", btn, ButtonState.Y, textSize = 18)
        }
        Box(Modifier.align(BiasAlignment(1f, 0f))) {
            RoundButton(
                "A", btn, ButtonState.A,
                background = PepoColors.Blue,
                pressedColor = PepoColors.BlueHover,
                textColor = PepoColors.OnAccent,
                textSize = 20,
                pop = true
            )
        }
        Box(Modifier.align(BiasAlignment(0f, 1f))) {
            RoundButton("B", btn, ButtonState.B, textSize = 18)
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
                Text("Pantalla táctil", style = MaterialTheme.typography.bodyMedium)
                if (screen != null) {
                    Text(
                        status?.value ?: "Conectando la pantalla…",
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
private fun rememberDisplayRotation(): Int {
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
