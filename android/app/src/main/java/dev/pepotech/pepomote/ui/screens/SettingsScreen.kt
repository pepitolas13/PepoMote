package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.ui.theme.PepoColors

@Composable
fun SettingsScreen(onNewPairing: () -> Unit, onBack: () -> Unit) {
    val context = LocalContext.current
    var volB by remember { mutableStateOf(AppPrefs.volDownIsB(context)) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .padding(horizontal = 20.dp)
    ) {
        Spacer(Modifier.height(24.dp))
        Text("Ajustes", style = MaterialTheme.typography.headlineMedium)
        Spacer(Modifier.height(20.dp))

        Card(
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
            border = BorderStroke(1.5.dp, PepoColors.CardBorder),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(18.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(Modifier.weight(1f)) {
                    Text("Volumen-abajo = gatillo B", style = MaterialTheme.typography.titleMedium)
                    Text(
                        "El botón físico de bajar volumen actúa como B mientras el mando está abierto: tacto real, latencia cero",
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
                Switch(
                    checked = volB,
                    onCheckedChange = {
                        volB = it
                        AppPrefs.setVolDownIsB(context, it)
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
        }

        Spacer(Modifier.height(14.dp))
        Card(
            onClick = onNewPairing,
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
            border = BorderStroke(1.5.dp, PepoColors.CardBorder),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
        ) {
            Column(Modifier.fillMaxWidth().padding(18.dp)) {
                Text("Vincular con otro PC", style = MaterialTheme.typography.titleMedium)
                Text("Escanear un QR nuevo", style = MaterialTheme.typography.bodyMedium)
            }
        }

        Spacer(Modifier.height(14.dp))
        Text(
            "La sensibilidad del puntero se ajusta en la ventana de PepoMote del PC.",
            style = MaterialTheme.typography.bodyMedium
        )

        Spacer(Modifier.weight(1f))
        Text(
            "PepoMote · GPL-3.0 · hecho por PepoTech",
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.align(Alignment.CenterHorizontally)
        )
        TextButton(onClick = onBack, modifier = Modifier.fillMaxWidth()) {
            Text("Volver", color = PepoColors.TextDim)
        }
        Spacer(Modifier.height(16.dp))
    }
}
