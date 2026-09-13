package dev.pepotech.pepomote.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.control.UpdateCheck
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Aviso de versión nueva en el inicio: «PepoMote X disponible», el enlace a
 * la release de GitHub y «Ocultar» (esa versión no se vuelve a anunciar).
 */
@Composable
fun UpdateCard(
    version: UpdateCheck.Version,
    onOpen: () -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    val shape = RoundedCornerShape(14.dp)
    Column(
        modifier
            .fillMaxWidth()
            .background(PepoColors.Card, shape)
            .border(1.5.dp, PepoColors.Blue, shape)
            .padding(horizontal = 16.dp, vertical = 10.dp)
    ) {
        Text(
            stringResource(R.string.update_available, version.toString()),
            style = MaterialTheme.typography.titleMedium
        )
        Row(verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onOpen, contentPadding = PaddingValues(horizontal = 4.dp)) {
                Text(stringResource(R.string.update_download), color = PepoColors.Blue)
            }
            Spacer(Modifier.width(8.dp))
            TextButton(onClick = onDismiss, contentPadding = PaddingValues(horizontal = 4.dp)) {
                Text(stringResource(R.string.update_dismiss), color = PepoColors.TextDim)
            }
        }
    }
}
