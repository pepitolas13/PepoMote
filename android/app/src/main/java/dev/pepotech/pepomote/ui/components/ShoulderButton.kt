package dev.pepotech.pepomote.ui.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
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
 * Botón rectangular momentáneo (rectángulo redondeado con texto): los
 * gatillos L/ZL/R/ZR del GamePad y las pastillas pequeñas TV/Pad y Soplar.
 * Mantiene el bit mientras está pulsado; háptico y sonido como los demás
 * (todo eso lo lleva `pressBit`, con los ajustes de pulsación).
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
    var down by remember { mutableStateOf(false) }
    val scale by animateFloatAsState(if (down) 0.94f else 1f, label = "press")
    val shape = RoundedCornerShape(height / 2.5f)

    Box(
        modifier = modifier
            .width(width)
            .height(height)
            .semantics {
                role = Role.Button
                contentDescription = label
                onClick { ButtonState.set(bit, true); ButtonState.set(bit, false); true }
            }
            // La zona pulsable se mide antes del encogido: la pastilla
            // pulsada sigue ocupando lo mismo para el dedo que pasa por encima
            .pressBit(bit) { down = it }
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .shadow(if (down) 1.dp else 4.dp, shape)
            .background(if (down) PepoColors.Glow else PepoColors.Card, shape),
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
