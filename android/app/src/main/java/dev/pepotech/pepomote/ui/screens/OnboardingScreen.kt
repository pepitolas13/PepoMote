package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.displayCutoutPadding
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.ui.theme.PepoColors
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

/**
 * Los pasos van en una columna que se desplaza y el botón queda fuera de
 * ella, pegado abajo: con la letra del sistema grande (o en un móvil
 * pequeño) los tres pasos no caben, y antes empujaban «¡Vamos!» fuera de la
 * pantalla sin forma de avanzar. Así el botón se ve siempre y lo que sobra
 * se desplaza. El botón crece con la letra en vez de recortarla.
 */
@Composable
fun OnboardingScreen(onDone: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .displayCutoutPadding()
            .padding(horizontal = 24.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Spacer(Modifier.height(48.dp))
            Text("PepoMote", style = MaterialTheme.typography.displayLarge)
            Text(
                stringResource(R.string.onboarding_sub),
                style = MaterialTheme.typography.bodyMedium
            )
            Spacer(Modifier.height(32.dp))

            Step(1, stringResource(R.string.ob1_title), stringResource(R.string.ob1_body))
            Spacer(Modifier.height(14.dp))
            Step(2, stringResource(R.string.ob2_title), stringResource(R.string.ob2_body))
            Spacer(Modifier.height(14.dp))
            Step(3, stringResource(R.string.ob3_title), stringResource(R.string.ob3_body))
            Spacer(Modifier.height(16.dp))
        }

        Button(
            onClick = onDone,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 60.dp),
            shape = MaterialTheme.shapes.medium,
            colors = ButtonDefaults.buttonColors(containerColor = PepoColors.Blue)
        ) {
            Text(stringResource(R.string.lets_go), style = MaterialTheme.typography.titleMedium.copy(color = PepoColors.OnAccent))
        }
        Spacer(Modifier.height(24.dp))
    }
}

@Composable
private fun Step(n: Int, title: String, body: String) {
    Card(
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
        border = BorderStroke(1.5.dp, PepoColors.CardBorder),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Box(
                modifier = Modifier
                    .size(38.dp)
                    .background(PepoColors.Blue, CircleShape),
                contentAlignment = Alignment.Center
            ) {
                Text("$n", style = MaterialTheme.typography.titleMedium.copy(color = PepoColors.OnAccent))
            }
            Spacer(Modifier.width(14.dp))
            Column {
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(body, style = MaterialTheme.typography.bodyMedium)
            }
        }
    }
}
