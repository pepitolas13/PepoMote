package dev.pepotech.pepomote.ui.components

import androidx.compose.foundation.background
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Zona-gatillo: banda ancha a todo el ancho, como el gatillo trasero del
 * Wiimote. Por defecto es B (en modo puntero, el clic derecho); el Nunchuk la
 * usa para Z (azul) y, con otros colores, para C encima del stick. Cómo se
 * pulsa y se suelta lo decide `pressBit` con los ajustes de pulsación.
 */
@Composable
fun TriggerZone(
    bit: Int = ButtonState.B,
    label: String = "B",
    height: Dp = 88.dp,
    background: Color = PepoColors.Blue,
    pressedColor: Color = PepoColors.BlueHover,
    textColor: Color = PepoColors.OnAccent,
    modifier: Modifier = Modifier
) {
    var down by remember { mutableStateOf(false) }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(height)
            .background(
                if (down) pressedColor else background,
                RoundedCornerShape(24.dp)
            )
            .pressBit(bit) { down = it },
        contentAlignment = Alignment.Center
    ) {
        Text(
            label,
            style = MaterialTheme.typography.headlineMedium.copy(color = textColor)
        )
    }
}
