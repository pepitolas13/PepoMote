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
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Zona-gatillo B: banda inferior ancha, como el gatillo trasero del Wiimote.
 * En modo puntero es el clic derecho.
 */
@Composable
fun TriggerZone() {
    val view = LocalView.current
    var down by remember { mutableStateOf(false) }

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(88.dp)
            .background(
                if (down) PepoColors.BlueHover else PepoColors.Blue,
                RoundedCornerShape(24.dp)
            )
            .pointerInput(Unit) {
                detectTapGestures(onPress = {
                    down = true
                    ButtonState.set(ButtonState.B, true)
                    view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                    tryAwaitRelease()
                    down = false
                    ButtonState.set(ButtonState.B, false)
                })
            },
        contentAlignment = Alignment.Center
    ) {
        Text(
            "B",
            style = MaterialTheme.typography.headlineMedium.copy(color = PepoColors.Card)
        )
    }
}
