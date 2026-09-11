package dev.pepotech.pepomote.ui.components

import android.view.HapticFeedbackConstants
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Zona-gatillo: banda inferior ancha, como el gatillo trasero del Wiimote.
 * Por defecto es B (en modo puntero, el clic derecho); el Nunchuk la usa
 * para Z.
 */
@Composable
fun TriggerZone(bit: Int = ButtonState.B, label: String = "B", height: Dp = 88.dp) {
    val view = LocalView.current
    var down by remember { mutableStateOf(false) }

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(height)
            .background(
                if (down) PepoColors.BlueHover else PepoColors.Blue,
                RoundedCornerShape(24.dp)
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
        Text(
            label,
            style = MaterialTheme.typography.headlineMedium.copy(color = PepoColors.Card)
        )
    }
}
