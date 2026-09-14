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
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Pregunta de la primera vez de un mando apaisado fijo (mando + Nunchuk,
 * GamePad): ¿está bien así? «Darle la vuelta» lo gira 180° y «Así lo quiero»
 * guarda ese lado para siempre ([dev.pepotech.pepomote.service.LandscapeSideState]).
 */
@Composable
internal fun SideAskCard(modifier: Modifier, title: String, onFlip: () -> Unit, onKeep: () -> Unit) {
    Column(
        modifier = modifier
            .widthIn(max = 340.dp)
            .background(PepoColors.Card, RoundedCornerShape(14.dp))
            .border(1.5.dp, PepoColors.Blue, RoundedCornerShape(14.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp)
    ) {
        Text(title, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        Text(stringResource(R.string.side_ask_sub), style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            ModeChip(stringResource(R.string.side_flip), selected = false, onClick = onFlip)
            ModeChip(stringResource(R.string.side_keep), selected = true, onClick = onKeep)
        }
    }
}
