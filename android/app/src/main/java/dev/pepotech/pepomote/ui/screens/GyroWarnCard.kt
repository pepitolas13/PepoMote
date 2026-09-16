package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.sensor.GyroClass
import dev.pepotech.pepomote.sensor.GyroDetect
import dev.pepotech.pepomote.sensor.MotionSource
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.components.pressShield
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Aviso de la primera vez en modo puntero cuando el móvil NO tiene giroscopio
 * real ([GyroDetect]): qué tiene («algo parecido», sin prometer que no tenga
 * deriva, retardo o inconsistencias), qué va a usar PepoMote (el
 * acelerómetro: inclinar) y dónde se cambia (Ajustes). Si el receptor es
 * anterior y no entiende la inclinación, lo dice. «Entendido» lo guarda para
 * no volver a salir. Con giroscopio real no aparece nunca.
 */
@Composable
internal fun GyroWarnCard(link: UiLink, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val connected = link as? UiLink.Connected ?: return
    if (connected.mode != LinkState.MODE_POINTER) return
    val gyroClass = remember { GyroDetect.classify(context) }
    if (gyroClass == GyroClass.REAL) return
    var shown by remember { mutableStateOf(!AppPrefs.gyroWarnShown(context)) }
    if (!shown) return
    val tilt by MotionSource.tilt.collectAsState()

    val title = if (gyroClass == GyroClass.VIRTUAL) R.string.gyro_warn_title_virtual else R.string.gyro_warn_title_none
    val body = when {
        !connected.supportsTilt -> R.string.gyro_warn_body_old_receiver
        !tilt -> R.string.gyro_warn_body_manual_gyro
        gyroClass == GyroClass.VIRTUAL -> R.string.gyro_warn_body_virtual
        else -> R.string.gyro_warn_body_none
    }
    Column(
        modifier = modifier
            .widthIn(max = 360.dp)
            .background(PepoColors.Card, RoundedCornerShape(14.dp))
            .border(1.5.dp, PepoColors.Warn, RoundedCornerShape(14.dp))
            // la tarjeta se queda el dedo que no coja «Entendido»: va encima de
            // los botones, y «Pulsar deslizando» los pulsaría a través
            .pressShield()
            .padding(horizontal = 14.dp, vertical = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp)
    ) {
        Text(stringResource(title), style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        Text(stringResource(body), style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
        ModeChip(stringResource(R.string.gyro_warn_ok), selected = true) {
            AppPrefs.setGyroWarnShown(context)
            shown = false
        }
    }
}
