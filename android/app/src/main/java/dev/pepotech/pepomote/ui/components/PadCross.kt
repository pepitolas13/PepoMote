package dev.pepotech.pepomote.ui.components

import android.view.HapticFeedbackConstants
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.BiasAlignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.control.SidewaysDpad
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Cruceta interactiva: cuatro brazos momentáneos. En modo puntero el
 * receptor hace ↑/↓ = flechas del PC y ←/→ = atrás/adelante del navegador.
 * Con [sideways] (mando de lado en un juego) manda los botones del mando
 * girado con el IR a la izquierda ([SidewaysDpad]).
 */
@Composable
fun PadCross(sizeDp: Dp, glyphSp: Int = 14, sideways: Boolean = false) {
    val arm = sizeDp / 3

    Box(modifier = Modifier.size(sizeDp), contentAlignment = Alignment.Center) {
        PadArm("▲", arm, glyphSp, SidewaysDpad.up(sideways), BiasAlignment(0f, -1f))
        PadArm("▼", arm, glyphSp, SidewaysDpad.down(sideways), BiasAlignment(0f, 1f))
        PadArm("◀", arm, glyphSp, SidewaysDpad.left(sideways), BiasAlignment(-1f, 0f))
        PadArm("▶", arm, glyphSp, SidewaysDpad.right(sideways), BiasAlignment(1f, 0f))
        // Centro
        Box(
            Modifier
                .size(arm)
                .background(PepoColors.Card, RoundedCornerShape(6.dp))
        )
    }
}

@Composable
private fun androidx.compose.foundation.layout.BoxScope.PadArm(
    label: String,
    arm: Dp,
    glyphSp: Int,
    bit: Int,
    alignment: BiasAlignment
) {
    val view = LocalView.current
    var down by remember { mutableStateOf(false) }
    Box(
        modifier = Modifier
            .align(alignment)
            .size(arm)
            .background(
                if (down) PepoColors.Glow else PepoColors.Card,
                RoundedCornerShape(10.dp)
            )
            .pointerInput(bit) {
                detectTapGestures(onPress = {
                    down = true
                    ButtonState.set(bit, true)
                    view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                    dev.pepotech.pepomote.control.UiSounds.blip()
                    tryAwaitRelease()
                    down = false
                    ButtonState.set(bit, false)
                })
            },
        contentAlignment = Alignment.Center
    ) {
        Text(label, color = PepoColors.TextDim, fontSize = glyphSp.sp)
    }
}
