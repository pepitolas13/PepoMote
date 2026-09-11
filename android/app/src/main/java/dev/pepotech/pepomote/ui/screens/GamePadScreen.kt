package dev.pepotech.pepomote.ui.screens

import android.content.Context
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.BiasAlignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Offset
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
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.sensor.SenderKind
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.Route
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.AnalogStick
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.PadSelector
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
 */
@Composable
fun GamePadScreen(link: UiLink, onDisconnect: () -> Unit) {
    val view = LocalView.current
    val connected = link as? UiLink.Connected
    val operative = Route.isGamePad(link)
    val pro = connected?.pad == LinkState.PAD_PRO
    val engine = LinkState.motion
    val rotation = rememberDisplayRotation()

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

        Column(Modifier.fillMaxSize()) {
            Header(link, operative, headerH, onDisconnect)
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
                            TouchScreen(touchW, touchH)
                        }
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            if (!pro) ShoulderButton("TV/Pad", ButtonState.SCREEN, 66.dp, 30.dp, textSize = 12)
                            RoundButton("−", bottomRowH * 0.85f, ButtonState.MINUS, textSize = 19)
                            RoundButton("Home", bottomRowH * 0.85f, ButtonState.HOME, textSize = 12)
                            RoundButton("+", bottomRowH * 0.85f, ButtonState.PLUS, textSize = 19)
                            if (!pro) ShoulderButton("Soplar", ButtonState.MIC, 66.dp, 30.dp, textSize = 12)
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
    }
}

/** Cabecera compacta: PC · «J1 · GamePad» / «J2 · Pro Controller» · chips de modo (Jugador 1) · Salir. */
@Composable
private fun Header(link: UiLink, operative: Boolean, height: Dp, onDisconnect: () -> Unit) {
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
                Text(
                    link.pcName,
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
                Text(
                    if (operative) buildString {
                        append("J${link.player} · ")
                        append(if (link.pad == LinkState.PAD_PRO) "Pro Controller" else "GamePad")
                        link.rttMs?.let { append(" · ${"%.0f".format(it)} ms") }
                    } else "Activando Wii U…",
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1
                )
                Spacer(Modifier.weight(1f))
                if (link.slot == 0) {
                    // Mientras se espera el eco, Wii U ya va marcado (es lo pedido)
                    ModeChips(
                        current = if (operative) link.mode else LinkState.MODE_CEMU,
                        supportsCemu = link.supportsCemu,
                        compact = true
                    )
                }
            }

            is UiLink.Connecting -> {
                Text("Conectando…", style = MaterialTheme.typography.titleMedium)
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
                textColor = PepoColors.Card,
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
 */
@Composable
private fun TouchScreen(width: Dp, height: Dp) {
    val view = LocalView.current
    var finger by remember { mutableStateOf<Offset?>(null) }
    val shape = RoundedCornerShape(10.dp)
    val dot = 14.dp

    Box(
        modifier = Modifier
            .size(width, height)
            .background(PepoColors.Card, shape)
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
        Text("Pantalla táctil", style = MaterialTheme.typography.bodyMedium)
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
