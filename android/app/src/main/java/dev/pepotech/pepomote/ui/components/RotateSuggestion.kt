package dev.pepotech.pepomote.ui.components

import android.content.pm.ActivityInfo
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.service.RotationSuggester
import dev.pepotech.pepomote.ui.theme.PepoColors

/**
 * Botón «Girar la pantalla»: aparece cuando has girado el móvil con el giro
 * automático del sistema bloqueado ([RotationSuggester]) y desaparece solo.
 * Tocarlo gira la app hacia el lado al que has puesto el móvil; nunca gira
 * sola.
 */
@Composable
fun RotateSuggestion(modifier: Modifier = Modifier) {
    val proposal by RotationSuggester.proposal.collectAsState()
    val p = proposal ?: return
    val toPortrait = p == ActivityInfo.SCREEN_ORIENTATION_PORTRAIT || p == ActivityInfo.SCREEN_ORIENTATION_REVERSE_PORTRAIT
    Text(
        "⟳  " + stringResource(if (toPortrait) R.string.rotate_to_portrait else R.string.rotate_to_landscape),
        modifier = modifier
            .background(PepoColors.Blue, RoundedCornerShape(18.dp))
            .clickable(role = Role.Button) { RotationSuggester.accept() }
            .padding(horizontal = 16.dp, vertical = 10.dp),
        style = MaterialTheme.typography.bodyMedium.copy(color = PepoColors.Card)
    )
}
