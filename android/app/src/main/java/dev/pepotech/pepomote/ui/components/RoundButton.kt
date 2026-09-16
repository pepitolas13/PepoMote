package dev.pepotech.pepomote.ui.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Botón circular momentáneo: mantiene el bit activo mientras está pulsado.
 * Háptica en cada pulsación. Cómo se pulsa y se suelta (mantener al salirse,
 * pulsar deslizando) lo decide `pressBit` con los ajustes.
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
    var down by remember { mutableStateOf(false) }
    val scale by animateFloatAsState(if (down) 0.90f else 1f, label = "press")

    Box(
        modifier = Modifier
            .size(sizeDp)
            .semantics {
                role = Role.Button
                contentDescription = label
                onClick { ButtonState.set(bit, true); ButtonState.set(bit, false); true }
            }
            // La zona pulsable se mide antes del encogido: el botón pulsado
            // sigue ocupando lo mismo para el dedo que pasa por encima
            .pressBit(bit, circular = true, pop = pop) { down = it }
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .shadow(if (down) 1.dp else 6.dp, CircleShape)
            .background(if (down) pressedColor else background, CircleShape),
        contentAlignment = Alignment.Center
    ) {
        // Se encoge hasta caber: «Home» en un círculo pequeño no se corta
        FitText(
            label,
            style = TextStyle(
                fontFamily = MaterialTheme.typography.titleLarge.fontFamily,
                fontWeight = MaterialTheme.typography.headlineMedium.fontWeight,
                color = textColor
            ),
            maxSize = textSize.sp,
            modifier = Modifier.padding(horizontal = 3.dp)
        )
    }
}
