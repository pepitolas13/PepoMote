package dev.pepotech.pepomote.ui.components

import android.content.Context
import android.view.accessibility.AccessibilityManager
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.screens.ModeChips
import dev.pepotech.pepomote.ui.screens.NunchukChip
import dev.pepotech.pepomote.ui.screens.isWiiUAsWiimote
import dev.pepotech.pepomote.ui.screens.modeLabel
import dev.pepotech.pepomote.ui.screens.showModeChips
import dev.pepotech.pepomote.ui.screens.showNunchukChip
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlinx.coroutines.delay

/**
 * Cuándo se pliega y cuándo se despliega sola la cabecera de los mandos
 * apaisados. Sin Compose: se prueba en la JVM.
 */
object HeaderCollapse {
    /** Lo que tarda en plegarse sola tras el último toque. */
    const val AUTO_MS = 4000L

    /**
     * Se pliega sola solo con el enlace vivo (sin él hace falta «Salir» a la
     * vista) y nunca con un lector de pantalla: ahí el usuario va tocando y
     * escuchando, y una cabecera que desaparece sola se pierde.
     */
    fun autoCollapses(connected: Boolean, screenReader: Boolean): Boolean = connected && !screenReader

    /**
     * Se despliega sola al caerse el enlace (hay que poder salir). Mientras
     * se reconecta, no: el servicio lo está arreglando solo y el mando sigue
     * en la mano. Tampoco por avisos ni por cambios de modo del PC.
     */
    fun expandsOn(link: UiLink): Boolean = link is UiLink.Disconnected
}

/**
 * Los dos textos de la cabecera plegable del GamePad (Wii U, Pro y Switch),
 * ya traducidos por quien llama. Sin Compose: se prueban en la JVM.
 */
object GamePadHeaderText {
    /**
     * Lo que dice la pastilla: con el enlace vivo, el modo que se quiere
     * («Wii U», «Switch»), que es lo que se está jugando aunque el eco del
     * receptor aún no haya llegado; si no, el estado del enlace.
     *
     * Reconectando no es asunto suyo: la pastilla pinta entonces el punto
     * latiendo ([ReconnectingLabel]) y no este texto, así que aquí ese caso
     * se reduce a «sin conexión».
     */
    fun handle(connected: Boolean, connecting: Boolean, mode: String, connectingText: String, disconnectedText: String): String = when {
        connected -> mode
        connecting -> connectingText
        else -> disconnectedText
    }

    /**
     * La línea de estado de la tarjeta, la de siempre: «J1 · GamePad» con el
     * ida y vuelta cuando se sabe («J1 · GamePad · 23 ms»). Sin el modo
     * confirmado todavía, el aviso de que se está activando.
     *
     * En la tarjeta hay sitio de sobra, así que aquí no se recorta nada por
     * ancho (la cabecera de una línea sí lo hacía).
     */
    fun status(operative: Boolean, player: Int, padName: String, rttMs: Float?, activating: String): String {
        if (!operative) return activating
        return buildString {
            append("J")
            append(player)
            append(" · ")
            append(padName)
            if (rttMs != null) append(" · ${"%.0f".format(rttMs)} ms")
        }
    }
}

/**
 * ¿Hay ahora mismo un lector de pantalla explorando por toque (TalkBack)? Se
 * mira en vivo, no una vez al entrar: encenderlo con el mando ya abierto
 * desactiva el pliegue automático en el acto, y apagarlo lo vuelve a activar
 * (iOS lee `UIAccessibility.isVoiceOverRunning` cada vez, igual).
 */
@Composable
fun rememberScreenReader(): Boolean {
    val context = LocalContext.current
    var on by remember { mutableStateOf(false) }
    DisposableEffect(context) {
        val manager = context.getSystemService(Context.ACCESSIBILITY_SERVICE) as? AccessibilityManager
        on = manager != null && manager.isEnabled && manager.isTouchExplorationEnabled
        val listener = AccessibilityManager.TouchExplorationStateChangeListener { exploring ->
            on = exploring && manager?.isEnabled == true
        }
        manager?.addTouchExplorationStateChangeListener(listener)
        onDispose { manager?.removeTouchExplorationStateChangeListener(listener) }
    }
    return on
}

/**
 * El mecanismo de la cabecera plegable, sin decidir qué va dentro: la
 * pastilla ([label], y a su lado lo que pida [beside], que se ve siempre) y,
 * debajo, la tarjeta que dibuje [card] con el marco de siempre.
 *
 * Nace desplegada y se pliega sola a los cuatro segundos
 * ([HeaderCollapse.AUTO_MS]), para que la tarjeta no quede encima de los
 * botones que hay debajo. Cualquier toque dentro de la tarjeta reinicia la
 * cuenta sin robarle el toque a nadie ([PointerEventPass.Initial], sin
 * consumir). [card] recibe el «pliégate» por si algo de dentro (el teclado)
 * tiene que quitar la tarjeta de en medio.
 *
 * [topPadding]: el hueco sobre la caja táctil de 44 dp de la pastilla. Los
 * mandos de lado le dan 6 dp; el GamePad, 0, que así sus 44 dp son justo la
 * banda de cabecera y el hueco que ya tenía el layout.
 */
@Composable
fun CollapsibleHeader(
    link: UiLink,
    label: String,
    modifier: Modifier = Modifier,
    topPadding: Dp = 6.dp,
    beside: @Composable RowScope.() -> Unit = {},
    onExpandedChange: (Boolean) -> Unit = {},
    // Además de las reglas de HeaderCollapse: el GamePad no se pliega solo
    // mientras espera el eco del modo (el mando está inerte y la tarjeta es
    // lo único que explica por qué, con los chips y «Salir» a mano)
    autoCollapse: Boolean = true,
    card: @Composable (collapse: () -> Unit) -> Unit
) {
    var expanded by rememberSaveable { mutableStateOf(true) }
    // Cada toque dentro de la tarjeta sube el contador y con él vuelve a
    // empezar la cuenta atrás del pliegue
    var touches by remember { mutableIntStateOf(0) }

    val screenReader = rememberScreenReader()
    val auto = autoCollapse && HeaderCollapse.autoCollapses(link is UiLink.Connected, screenReader)
    LaunchedEffect(expanded, touches, auto) {
        if (expanded && auto) {
            delay(HeaderCollapse.AUTO_MS)
            expanded = false
        }
    }
    // Se cayó el enlace: la cabecera vuelve sola (hay que poder salir)
    val wantsExpanded = HeaderCollapse.expandsOn(link)
    LaunchedEffect(wantsExpanded) {
        if (wantsExpanded) expanded = true
    }
    val notify by rememberUpdatedState(onExpandedChange)
    LaunchedEffect(expanded) { notify(expanded) }

    Column(
        modifier = modifier.padding(top = topPadding),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            HeaderHandle(link, label, expanded) { expanded = !expanded }
            beside()
        }
        AnimatedVisibility(
            visible = expanded,
            enter = fadeIn(tween(140)) + expandVertically(tween(160)),
            exit = fadeOut(tween(120)) + shrinkVertically(tween(140))
        ) {
            HeaderCardFrame(onTouch = { touches++ }) {
                card { expanded = false }
            }
        }
    }
}

/**
 * Cabecera plegable de los mandos de Wii apaisados (mando + Nunchuk y mando
 * de lado): la pastilla con el modo y, dentro de la tarjeta, el nombre del
 * PC, los chips, el Nunchuk, el selector de mando y «Salir».
 *
 * [alwaysNunchukChip]: el chip «Nunchuk» siempre (el mando + Nunchuk, donde
 * es la forma de volver al mando de siempre); si no, con las reglas de
 * [showNunchukChip]. [onKeyboard]: abre el teclado de Cemu (y pliega); solo
 * sale en modo Wii U. [onExpandedChange] avisa a la pantalla (el mando +
 * Nunchuk esconde su tarjeta de «¿está bien así?» mientras está desplegada).
 */
@Composable
fun CollapsibleHeader(
    link: UiLink,
    showChips: Boolean,
    modifier: Modifier = Modifier,
    alwaysNunchukChip: Boolean = false,
    onKeyboard: (() -> Unit)? = null,
    onExpandedChange: (Boolean) -> Unit = {},
    onDisconnect: () -> Unit
) {
    CollapsibleHeader(
        link = link,
        label = handleLabel(link),
        modifier = modifier,
        onExpandedChange = onExpandedChange
    ) { collapse ->
        HeaderCard(
            link = link,
            showChips = showChips,
            alwaysNunchukChip = alwaysNunchukChip,
            onKeyboard = onKeyboard?.let { open -> { collapse(); open() } },
            onDisconnect = onDisconnect
        )
    }
}

/**
 * La pastilla: caja táctil de 44 dp de alto (el dedo la coge sin apuntar)
 * con una píldora visible de 26 dp dentro, el modo y la flecha.
 */
@Composable
private fun HeaderHandle(link: UiLink, label: String, expanded: Boolean, onToggle: () -> Unit) {
    val state = stringResource(if (expanded) R.string.header_hide else R.string.header_show)
    // Plegada, la pastilla es lo único que se ve del enlace: el lector tiene
    // que decir también el modo («Controles · Dolphin · Nunchuk»), que si no
    // el modo y el estado del enlace se quedan fuera del lector y de la voz
    val name = stringResource(R.string.header_controls) + " · " + handleSpoken(link, label)
    Box(
        modifier = Modifier
            .height(44.dp)
            .clickable(role = Role.Button, onClick = onToggle)
            .semantics(mergeDescendants = true) {
                contentDescription = name
                stateDescription = state
            },
        contentAlignment = Alignment.Center
    ) {
        Row(
            modifier = Modifier
                .height(26.dp)
                // ancho = el del texto + 44, entre 96 y 240 dp
                .widthIn(min = 96.dp, max = 240.dp)
                .background(PepoColors.Card.copy(alpha = 0.85f), RoundedCornerShape(13.dp))
                .padding(horizontal = 22.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally)
        ) {
            val style = MaterialTheme.typography.bodyMedium.copy(color = PepoColors.TextDim)
            if (link is UiLink.Reconnecting) {
                ReconnectingLabel(link, style, Modifier.weight(1f, fill = false))
            } else {
                Text(
                    label,
                    style = style,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false)
                )
            }
            Text(if (expanded) "▴" else "▾", style = style)
        }
    }
}

/**
 * Lo que se lee de la pastilla: lo mismo que se ve. Reconectando, el texto
 * del punto latiendo ([ReconnectingLabel]), que no es el de la pastilla.
 */
@Composable
private fun handleSpoken(link: UiLink, label: String): String = when (link) {
    is UiLink.Reconnecting -> stringResource(R.string.status_reconnecting, link.pcName)
    else -> label
}

/** Lo que dice la pastilla de los mandos de Wii: el modo, como siempre. */
@Composable
private fun handleLabel(link: UiLink): String = when {
    link !is UiLink.Connected -> stringResource(
        if (link is UiLink.Connecting) R.string.status_connecting else R.string.status_disconnected
    )
    // el mando lleva su propio Nunchuk (esta pantalla solo sale así)
    link.mode == LinkState.MODE_DOLPHIN && link.ownNunchuk -> stringResource(R.string.mode_dolphin_nunchuk)
    isWiiUAsWiimote(link) -> stringResource(R.string.wiiu_as_wiimote)
    else -> modeLabel(link.mode)
}

/**
 * El marco de la tarjeta, igual para todas las cabeceras plegables: bajo la
 * pastilla, centrada, con tope de ancho, y con los dedos vigilados.
 */
@Composable
private fun HeaderCardFrame(onTouch: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val shape = RoundedCornerShape(14.dp)
    Column(
        modifier = Modifier
            .padding(top = 4.dp)
            .widthIn(max = 420.dp)
            .background(PepoColors.Card, shape)
            .border(1.5.dp, PepoColors.CardBorder, shape)
            // mirar los dedos antes que nadie sin quedárselos: la cuenta atrás
            // del pliegue vuelve a empezar y el chip de dentro recibe su toque
            .pointerInput(Unit) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent(PointerEventPass.Initial)
                        if (event.changes.any { it.pressed != it.previousPressed }) onTouch()
                    }
                }
            }
            // ...y quedarse el dedo que no coja ningún hijo: la tarjeta tapa a
            // B, Z y C, y sin esto «Pulsar deslizando» los pulsaría a través
            .pressShield()
            .padding(horizontal = 14.dp, vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
        content = content
    )
}

/**
 * La tarjeta de los mandos de Wii: las mismas filas que tenía la cabecera de
 * siempre, una debajo de otra ahora que no hay que meterlas en una línea.
 */
@Composable
private fun HeaderCard(
    link: UiLink,
    showChips: Boolean,
    alwaysNunchukChip: Boolean,
    onKeyboard: (() -> Unit)?,
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
        // Modo Wii U: texto para el teclado en pantalla de Cemu
        if (onKeyboard != null && link is UiLink.Connected && link.mode == LinkState.MODE_CEMU) {
            KeyboardButton(compact = true, onClick = onKeyboard)
        }
        TextButton(onClick = onDisconnect) {
            Text(stringResource(R.string.exit), color = PepoColors.Error, style = MaterialTheme.typography.bodyMedium)
        }
    }
    if (link is UiLink.Connected) {
        if (showModeChips(link, showChips)) {
            ModeChips(
                current = link.mode,
                supportsCemu = link.supportsCemu,
                supportsSwitch = link.supportsSwitch,
                androidReceiver = link.platform == "android",
                compact = true
            )
        }
        if (alwaysNunchukChip || showNunchukChip(link)) {
            NunchukChip(link, compact = true)
        }
        if (isWiiUAsWiimote(link)) {
            PadSelector(link, compact = true)
        }
    }
}
