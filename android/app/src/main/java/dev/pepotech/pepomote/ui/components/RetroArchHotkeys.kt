package dev.pepotech.pepomote.ui.components

import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Una tecla rápida de RetroArch tal como la pide el móvil (mensaje `hotkey`,
 * PROTOCOL.md §3): nombre del protocolo, etiqueta y si es de mantener
 * (rebobinar: pulsada mientras el dedo esté encima) o de un toque.
 */
data class RetroHotkey(val name: String, val label: Int, val hold: Boolean = false)

/** Las que caben en una tarjeta; el resto de la tabla del receptor no hace falta a un toque. */
val RETRO_HOTKEYS: List<RetroHotkey> = listOf(
    RetroHotkey("save_state", R.string.hotkey_save),
    RetroHotkey("load_state", R.string.hotkey_load),
    RetroHotkey("slot_minus", R.string.hotkey_slot_minus),
    RetroHotkey("slot_plus", R.string.hotkey_slot_plus),
    RetroHotkey("rewind", R.string.hotkey_rewind, hold = true),
    RetroHotkey("pause", R.string.hotkey_pause),
    RetroHotkey("screenshot", R.string.hotkey_screenshot),
    RetroHotkey("reset", R.string.hotkey_reset)
)

/**
 * Fila de teclas rápidas de RetroArch para las tarjetas de cabecera: guardar
 * y cargar estado, ranura − / +, rebobinar (mantener), pausa, captura y
 * reiniciar. Cada una va por el canal de control al receptor, que la
 * traduce al comando de red de RetroArch; el avance rápido y el menú están
 * en el propio mando (Rápido y Menú).
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun RetroArchHotkeys(link: UiLink.Connected, compact: Boolean = false, modifier: Modifier = Modifier) {
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
            verticalArrangement = Arrangement.spacedBy(6.dp)
        ) {
            for (h in RETRO_HOTKEYS) {
                HotkeyChip(stringResource(h.label), h.hold, compact) { down ->
                    LinkState.sendHotkey?.invoke(h.name, down)
                }
            }
        }
        if (!compact) {
            Text(
                stringResource(R.string.hotkeys_hint),
                style = MaterialTheme.typography.bodyMedium.copy(color = PepoColors.TextDim, fontSize = 11.sp),
                modifier = Modifier.padding(top = 4.dp),
                maxLines = 1
            )
        }
    }
    // Si la tarjeta se pliega con Rebobinar bajo el dedo, el receptor no se
    // queda rebobinando: suelta todas al desaparecer
    DisposableEffect(link.slot) {
        onDispose { LinkState.sendHotkey?.invoke("rewind", false) }
    }
}

/**
 * Chip de tecla rápida. Las de un toque disparan al pulsar (`down` true una
 * vez); las de mantener mandan true al bajar el dedo y false al levantarlo o
 * al perder el toque, y se pintan pulsadas mientras tanto.
 */
@Composable
private fun HotkeyChip(label: String, hold: Boolean, compact: Boolean, onPress: (Boolean) -> Unit) {
    val current by rememberUpdatedState(onPress)
    var pressed by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(16.dp)
    val fill = if (pressed) PepoColors.Blue else PepoColors.Card
    val text = if (pressed) PepoColors.OnAccent else PepoColors.Text
    Box(
        modifier = Modifier
            .background(fill, shape)
            .border(1.5.dp, if (pressed) PepoColors.Blue else PepoColors.CardBorder, shape)
            .semantics { role = Role.Button }
            .pointerInput(label, hold) {
                detectTapGestures(
                    onPress = {
                        pressed = true
                        current(true)
                        tryAwaitRelease()
                        pressed = false
                        if (hold) current(false)
                    }
                )
            }
            .padding(horizontal = if (compact) 10.dp else 12.dp, vertical = if (compact) 5.dp else 7.dp)
    ) {
        Text(label, style = MaterialTheme.typography.bodyMedium.copy(color = text, fontSize = if (compact) 12.sp else 13.sp), maxLines = 1)
    }
}
