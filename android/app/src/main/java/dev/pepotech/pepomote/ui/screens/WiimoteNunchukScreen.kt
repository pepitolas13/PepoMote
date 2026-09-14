package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layoutId
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.sensor.SenderKind
import dev.pepotech.pepomote.service.LandscapeSide
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.NunchukSide
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.AnalogStick
import dev.pepotech.pepomote.ui.components.HeaderSlot
import dev.pepotech.pepomote.ui.components.NoticeBanner
import dev.pepotech.pepomote.ui.components.PadCross
import dev.pepotech.pepomote.ui.components.PriorityRow
import dev.pepotech.pepomote.ui.components.ReconnectingLabel
import dev.pepotech.pepomote.ui.components.RoundButton
import dev.pepotech.pepomote.ui.components.ShoulderButton
import dev.pepotech.pepomote.ui.components.UiScale
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

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .navigationBarsPadding()
            .displayCutoutPadding()
    ) {
        val s = UiScale.landscape(maxWidth.value, maxHeight.value)
        val m = WiiNunchukMetrics(maxWidth.value, maxHeight.value, s)

        // Cabecera compacta: PC · «Dolphin · Nunchuk» · chips · Salir
        Column(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = 6.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            PriorityRow(spacing = 14.dp, fill = false) {
                if (link is UiLink.Reconnecting) {
                    ReconnectingLabel(link, MaterialTheme.typography.bodyMedium, modifier = Modifier.layoutId(HeaderSlot.Status))
                } else {
                    Text(
                        when (link) {
                            is UiLink.Connected -> link.pcName
                            is UiLink.Connecting -> stringResource(R.string.status_connecting)
                            else -> stringResource(R.string.status_disconnected)
                        },
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier
                            .layoutId(if (link is UiLink.Connected) HeaderSlot.Pc else HeaderSlot.Status)
                            .widthIn(max = 160.dp),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                }
                if (link is UiLink.Connected) {
                    Text(
                        stringResource(R.string.mode_dolphin_nunchuk),
                        style = MaterialTheme.typography.bodyMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.layoutId(HeaderSlot.Status)
                    )
                    Row(
                        modifier = Modifier.layoutId(HeaderSlot.Chips),
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        if (showModeChips(link, showChips)) {
                            ModeChips(current = link.mode, supportsCemu = link.supportsCemu, compact = true)
                        }
                        NunchukChip(link, compact = true)
                    }
                }
                TextButton(onClick = onDisconnect, modifier = Modifier.layoutId(HeaderSlot.Exit)) {
                    Text(stringResource(R.string.exit), color = PepoColors.Error, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }

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

        if (sideSaved == LandscapeSide.Unset) {
            val shown = LandscapeSide.effective(LandscapeSide.current(rotation), sideProvisional)
            SideAskCard(
                modifier = Modifier
                    .align(Alignment.TopCenter)
                    .padding(top = (m.headerH + 4f).dp),
                onFlip = { NunchukSide.setProvisional(shown.flipped()) },
                onKeep = { NunchukSide.save(context, shown) }
            )
        }

        NoticeBanner(
            Modifier
                .align(Alignment.TopCenter)
                .padding(top = 84.dp)
        )
    }
}

/** Pregunta de la primera vez: ¿el mando está bien así? Darle la vuelta / Así lo quiero. */
@Composable
private fun SideAskCard(modifier: Modifier, onFlip: () -> Unit, onKeep: () -> Unit) {
    Column(
        modifier = modifier
            .widthIn(max = 340.dp)
            .background(PepoColors.Card, RoundedCornerShape(14.dp))
            .border(1.5.dp, PepoColors.Blue, RoundedCornerShape(14.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp)
    ) {
        Text(stringResource(R.string.side_ask), style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        Text(stringResource(R.string.side_ask_sub), style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            ModeChip(stringResource(R.string.side_flip), selected = false, onClick = onFlip)
            ModeChip(stringResource(R.string.side_keep), selected = true, onClick = onKeep)
        }
    }
}
