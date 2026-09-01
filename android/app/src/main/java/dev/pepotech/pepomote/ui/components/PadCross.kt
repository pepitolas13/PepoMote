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
import dev.pepotech.pepomote.ui.theme.PepoColors

/** Cruceta interactiva: cuatro brazos momentáneos (flechas del PC). */
@Composable
fun PadCross(sizeDp: Dp) {
    val arm = sizeDp / 3

    Box(modifier = Modifier.size(sizeDp), contentAlignment = Alignment.Center) {
        PadArm("▲", arm, ButtonState.DPAD_UP, BiasAlignment(0f, -1f))
        PadArm("▼", arm, ButtonState.DPAD_DOWN, BiasAlignment(0f, 1f))
        PadArm("◀", arm, ButtonState.DPAD_LEFT, BiasAlignment(-1f, 0f))
        PadArm("▶", arm, ButtonState.DPAD_RIGHT, BiasAlignment(1f, 0f))
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
        Text(label, color = PepoColors.TextDim, fontSize = 14.sp)
    }
}
