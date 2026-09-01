package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
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

@Composable
fun OnboardingScreen(onDone: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .padding(horizontal = 24.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Spacer(Modifier.height(48.dp))
        Text("PepoMote", style = MaterialTheme.typography.displayLarge)
        Text(
            "Tu móvil, mando de tu PC en 3 pasos",
            style = MaterialTheme.typography.bodyMedium
        )
        Spacer(Modifier.height(32.dp))

        Step(1, "Abre PepoMote en tu PC", "Un solo archivo, sin instalar nada. En pepomote verás un QR.")
        Spacer(Modifier.height(14.dp))
        Step(2, "Escanea el QR", "Conectar → apunta con la cámara. Emparejado para siempre.")
        Spacer(Modifier.height(14.dp))
        Step(3, "Apunta. Haz clic. Juega.", "El móvil mueve el cursor como un mando de Wii. Y en Dolphin, juegas de verdad.")

        Spacer(Modifier.weight(1f))
        Button(
            onClick = onDone,
            modifier = Modifier
                .fillMaxWidth()
                .height(60.dp),
            shape = MaterialTheme.shapes.medium,
            colors = ButtonDefaults.buttonColors(containerColor = PepoColors.Blue)
        ) {
            Text("¡Vamos!", style = MaterialTheme.typography.titleMedium.copy(color = PepoColors.Card))
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
                Text("$n", style = MaterialTheme.typography.titleMedium.copy(color = PepoColors.Card))
            }
            Spacer(Modifier.width(14.dp))
            Column {
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(body, style = MaterialTheme.typography.bodyMedium)
            }
        }
    }
}
