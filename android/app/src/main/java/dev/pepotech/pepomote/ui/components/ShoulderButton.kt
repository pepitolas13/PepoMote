package dev.pepotech.pepomote.ui.components

import android.view.HapticFeedbackConstants
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
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
 * Botón rectangular momentáneo (rectángulo redondeado con texto): los
 * gatillos L/ZL/R/ZR del GamePad y las pastillas pequeñas TV/Pad y Soplar.
 * Mantiene el bit mientras está pulsado; háptico y sonido como los demás.
 */
@Composable
fun ShoulderButton(
    label: String,
    bit: Int,
    width: Dp,
    height: Dp,
    textSize: Int = 16,
    modifier: Modifier = Modifier
) {
    val view = LocalView.current
    var down by remember { mutableStateOf(false) }
    val scale by animateFloatAsState(if (down) 0.94f else 1f, label = "press")
    val shape = RoundedCornerShape(height / 2.5f)

    Box(
        modifier = modifier
            .width(width)
            .height(height)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .shadow(if (down) 1.dp else 4.dp, shape)
            .background(if (down) PepoColors.Glow else PepoColors.Card, shape)
            .pointerInput(bit) {
                detectTapGestures(onPress = {
                    down = true
                    ButtonState.set(bit, true)
                    view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                    UiSounds.blip()
                    tryAwaitRelease()
                    down = false
                    ButtonState.set(bit, false)
                })
            },
        contentAlignment = Alignment.Center
    ) {
        // Se encoge hasta caber: «TV/Pad» y «Soplar» en una pastilla estrecha
        FitText(
            label,
            style = TextStyle(
                fontFamily = MaterialTheme.typography.titleLarge.fontFamily,
                fontWeight = MaterialTheme.typography.headlineMedium.fontWeight,
                color = PepoColors.Text
            ),
            maxSize = textSize.sp,
            modifier = Modifier.padding(horizontal = 4.dp)
        )
    }
}
