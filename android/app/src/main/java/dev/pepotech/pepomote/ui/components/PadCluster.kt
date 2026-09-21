package dev.pepotech.pepomote.ui.components

import android.view.HapticFeedbackConstants
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.BiasAlignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.control.Haptics
import dev.pepotech.pepomote.control.RetroColor
import dev.pepotech.pepomote.control.RetroLayout
import dev.pepotech.pepomote.control.RetroLayouts
import dev.pepotech.pepomote.control.RetroShape
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlin.math.roundToInt

/** Etiqueta de un botón de plantilla: las palabras (`$coin`, `$fire`) se traducen. */
@Composable
fun retroLabel(label: String): String = when (label) {
    "\$coin" -> stringResource(R.string.retro_coin)
    "\$fire" -> stringResource(R.string.retro_fire)
    else -> label
}

/** Relleno de un botón de consola (`CARD` = el de siempre). */
@Composable
private fun tint(color: RetroColor): Color? = when (color) {
    RetroColor.CARD -> null
    RetroColor.RED -> Color(0xFFD94A4A)
    RetroColor.YELLOW -> C_YELLOW
    RetroColor.GREEN -> Color(0xFF3FA55B)
    RetroColor.BLUE -> PepoColors.Blue
    RetroColor.PINK -> Color(0xFFD96BB0)
    RetroColor.PURPLE -> Color(0xFF8A63C9)
}

/** Amarillo de los botones C de N64. */
private val C_YELLOW = Color(0xFFE0B42A)

/**
 * Los botones frontales de la plantilla en una caja de [boxW]×[boxH]: el
 * rombo de siempre (X arriba, Y izquierda, A a la derecha en azul, B abajo:
 * con el RetroPad es exactamente el `FaceButtons` de antes), dos en fila (a
 * distinta altura si la consola los tenía así), uno, seis en dos filas o tres
 * en fila. El texto crece con el botón y los colores son los de cada consola.
 */
@Composable
fun FaceCluster(layout: RetroLayout, boxW: Dp, boxH: Dp) {
    val base: Dp = when (layout.shape) {
        RetroShape.DIAMOND -> minOf(boxW, boxH) / 2.6f
        RetroShape.TWO -> minOf(boxH * 0.55f, boxW * 0.42f)
        RetroShape.SINGLE -> minOf(boxW, boxH) * 0.7f
        RetroShape.GRID2X3 -> minOf(boxW / 3.3f, boxH / 2.2f)
        RetroShape.ROW3 -> minOf(boxW / 3.3f, boxH * 0.8f)
    }
    Box(Modifier.size(boxW, boxH)) {
        for (f in layout.face) {
            val (bx, by) = when (layout.shape to f.slot) {
                RetroShape.DIAMOND to "top" -> 0f to -1f
                RetroShape.DIAMOND to "left" -> -1f to 0f
                RetroShape.DIAMOND to "right" -> 1f to 0f
                RetroShape.DIAMOND to "bottom" -> 0f to 1f
                RetroShape.TWO to "left" -> -1f to 0.6f * layout.stagger
                RetroShape.TWO to "right" -> 1f to -0.6f * layout.stagger
                RetroShape.SINGLE to "center" -> 0f to 0f
                RetroShape.GRID2X3 to "tl" -> -1f to -1f
                RetroShape.GRID2X3 to "tm" -> 0f to -1f
                RetroShape.GRID2X3 to "tr" -> 1f to -1f
                RetroShape.GRID2X3 to "bl" -> -1f to 1f
                RetroShape.GRID2X3 to "bm" -> 0f to 1f
                RetroShape.GRID2X3 to "br" -> 1f to 1f
                RetroShape.ROW3 to "l" -> -1f to 0f
                RetroShape.ROW3 to "m" -> 0f to 0f
                RetroShape.ROW3 to "r" -> 1f to 0f
                else -> 0f to 0f
            }
            val btn = base * f.size
            val fill = tint(f.color)
            Box(Modifier.align(BiasAlignment(bx, by))) {
                when {
                    f.primary -> RoundButton(
                        retroLabel(f.label), btn, f.bit,
                        background = fill ?: PepoColors.Blue,
                        pressedColor = if (fill != null) lerp(fill, Color.Black, 0.25f) else PepoColors.BlueHover,
                        textColor = PepoColors.OnAccent,
                        textSize = (btn.value * 20f / 44f).roundToInt(),
                        pop = true
                    )
                    fill != null -> RoundButton(
                        retroLabel(f.label), btn, f.bit,
                        background = fill,
                        pressedColor = lerp(fill, Color.Black, 0.25f),
                        textColor = PepoColors.OnAccent,
                        textSize = (btn.value * 18f / 44f).roundToInt()
                    )
                    else -> RoundButton(retroLabel(f.label), btn, f.bit, textSize = (btn.value * 18f / 44f).roundToInt())
                }
            }
        }
    }
}

/**
 * N64: los cuatro botones C en el hueco del stick derecho. Emiten los ejes
 * de ese stick a ±127 (así los lee Mupen64Plus), con un dedo por botón:
 * [onStick] recibe la suma de los pulsados (+y arriba, como el stick).
 */
@Composable
fun CButtons(sizeDp: Dp, onStick: (x: Int, y: Int) -> Unit) {
    val view = LocalView.current
    var held by remember { mutableIntStateOf(0) }
    val btn = sizeDp * 0.34f
    fun emit(mask: Int) {
        held = mask
        val x = (if (mask and 8 != 0) 127 else 0) - (if (mask and 4 != 0) 127 else 0)
        val y = (if (mask and 1 != 0) 127 else 0) - (if (mask and 2 != 0) 127 else 0)
        onStick(x, y)
    }
    Box(Modifier.size(sizeDp), contentAlignment = Alignment.Center) {
        for ((dir, label, bx, by) in listOf(
            Quad(0, "▲", 0f, -1f), Quad(1, "▼", 0f, 1f), Quad(2, "◀", -1f, 0f), Quad(3, "▶", 1f, 0f)
        )) {
            val bit = 1 shl dir
            val down = held and bit != 0
            Box(
                Modifier
                    .align(BiasAlignment(bx * 0.72f, by * 0.72f))
                    .size(btn)
                    .shadow(if (down) 1.dp else 4.dp, CircleShape)
                    .background(if (down) lerp(C_YELLOW, Color.Black, 0.25f) else C_YELLOW, CircleShape)
                    .pointerInput(dir) {
                        awaitEachGesture {
                            val first = awaitFirstDown()
                            first.consume()
                            Haptics.tap(view, HapticFeedbackConstants.CLOCK_TICK)
                            emit(held or bit)
                            waitForUpOrCancellation()
                            emit(held and bit.inv())
                        }
                    },
                contentAlignment = Alignment.Center
            ) {
                Text(label, color = PepoColors.OnAccent, fontSize = 13.sp, fontWeight = FontWeight.Bold)
            }
        }
    }
}

private data class Quad(val dir: Int, val label: String, val bx: Float, val by: Float)

/**
 * Selector del mando de consola en RetroArch: «Automático (Mega Drive)», las
 * consolas, «Mega Drive (3)» y «RetroPad completo». La elección se recuerda
 * para el juego cargado (o hasta que RetroArch cargue uno).
 */
@Composable
fun RetroLayoutPicker(link: UiLink.Connected, onClose: () -> Unit) {
    val context = LocalContext.current
    val choice by LinkState.retroLayoutChoice.collectAsState()
    val game = link.game
    val path = game?.path?.takeIf { it.isNotEmpty() }
    val chosen = choice.choiceFor(path)
    val autoName = RetroLayouts.byId(RetroLayouts.effective(game?.console, null))?.name ?: RetroLayouts.RETROPAD.name
    Dialog(onDismissRequest = onClose) {
        Column(
            Modifier
                .fillMaxWidth()
                .heightIn(max = 520.dp)
                .background(PepoColors.Background, RoundedCornerShape(16.dp))
                .border(1.dp, PepoColors.CardBorder, RoundedCornerShape(16.dp))
                .padding(14.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Text(stringResource(R.string.retro_layout_picker_title), style = MaterialTheme.typography.titleMedium, color = PepoColors.Text)
            Text(
                if (game != null && game.title.isNotEmpty()) stringResource(R.string.retro_layout_hint_game, game.title)
                else stringResource(R.string.retro_layout_hint_no_game),
                style = MaterialTheme.typography.bodyMedium.copy(fontSize = 12.sp),
                color = PepoColors.TextDim,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(bottom = 8.dp)
            )
            Column(
                Modifier
                    .weight(1f, fill = false)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                PickerRow(stringResource(R.string.retro_layout_auto, autoName), selected = chosen == null) {
                    LinkState.pickRetroLayout(context, path, null)
                    onClose()
                }
                for (l in RetroLayouts.ALL) {
                    val label = if (l.id == RetroLayouts.RETROPAD.id) stringResource(R.string.retro_layout_full) else l.name
                    PickerRow(label, selected = chosen == l.id) {
                        LinkState.pickRetroLayout(context, path, l.id)
                        onClose()
                    }
                }
            }
            TextButton(onClick = onClose) { Text(stringResource(R.string.close), color = PepoColors.Text) }
        }
    }
}

@Composable
private fun PickerRow(label: String, selected: Boolean, onClick: () -> Unit) {
    Box(
        Modifier
            .fillMaxWidth()
            .background(if (selected) PepoColors.Blue else PepoColors.Card, RoundedCornerShape(10.dp))
            .border(1.dp, PepoColors.CardBorder, RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(vertical = 9.dp, horizontal = 12.dp),
        contentAlignment = Alignment.Center
    ) {
        Text(label, color = if (selected) PepoColors.OnAccent else PepoColors.Text, style = MaterialTheme.typography.bodyMedium)
    }
}
