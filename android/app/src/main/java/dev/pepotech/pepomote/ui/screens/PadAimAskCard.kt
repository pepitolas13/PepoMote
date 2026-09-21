package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.ui.components.pressShield
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Pregunta de la primera vez del mando universal: mover el móvil mueve el
 * stick derecho, y eso en unos juegos es riqueza y en otros un estorbo (en
 * The Binding of Isaac disparas hacia donde muevas el mando; en Until Dawn
 * hay que dejarlo completamente quieto). «Sin giro» y «Con giro» se guardan
 * para siempre ([dev.pepotech.pepomote.control.AppPrefs.setPadAim]) y
 * Ajustes lo cambia. Mientras la tarjeta está en pantalla el giro va
 * apagado, así que el juego no hace nada raro mientras se lee.
 */
@Composable
internal fun PadAimAskCard(modifier: Modifier = Modifier, onPick: (Boolean) -> Unit) {
    Column(
        modifier = modifier
            .widthIn(max = 360.dp)
            .background(PepoColors.Card, RoundedCornerShape(14.dp))
            .border(1.5.dp, PepoColors.Warn, RoundedCornerShape(14.dp))
            // la tarjeta se queda el dedo que no coja sus chips: va encima de
            // los botones, y «Pulsar deslizando» los pulsaría a través
            .pressShield()
            .padding(horizontal = 14.dp, vertical = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp)
    ) {
        Text(
            stringResource(R.string.pad_aim_ask_title),
            style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center
        )
        Text(
            stringResource(R.string.pad_aim_ask_body),
            style = MaterialTheme.typography.bodyMedium,
            textAlign = TextAlign.Center
        )
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            ModeChip(stringResource(R.string.pad_aim_no), selected = false) { onPick(false) }
            ModeChip(stringResource(R.string.pad_aim_yes), selected = true) { onPick(true) }
        }
    }
}
