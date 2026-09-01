package dev.pepotech.pepomote.ui.components

import android.view.HapticFeedbackConstants
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.control.UiSounds
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Botón circular momentáneo: mantiene el bit activo mientras está pulsado.
 * Háptica en cada pulsación.
 */
@Composable
fun RoundButton(
    label: String,
    sizeDp: Dp,
    bit: Int,
    background: Color = PepoColors.Card,
    pressedColor: Color = PepoColors.Glow,
    textColor: Color = PepoColors.Text,
    textSize: Int = 20,
    pop: Boolean = false
) {
    val view = LocalView.current
    var down by remember { mutableStateOf(false) }
    val scale by animateFloatAsState(if (down) 0.90f else 1f, label = "press")

    Box(
        modifier = Modifier
            .size(sizeDp)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .shadow(if (down) 1.dp else 6.dp, CircleShape)
            .background(if (down) pressedColor else background, CircleShape)
            .pointerInput(bit) {
                detectTapGestures(onPress = {
                    down = true
                    ButtonState.set(bit, true)
                    view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                    if (pop) UiSounds.pop() else UiSounds.blip()
                    tryAwaitRelease()
                    down = false
                    ButtonState.set(bit, false)
                })
            },
        contentAlignment = Alignment.Center
    ) {
        Text(
            label,
            style = TextStyle(
                fontFamily = MaterialTheme.typography.titleLarge.fontFamily,
                fontWeight = MaterialTheme.typography.headlineMedium.fontWeight,
                fontSize = textSize.sp,
                color = textColor
            )
        )
    }
}
