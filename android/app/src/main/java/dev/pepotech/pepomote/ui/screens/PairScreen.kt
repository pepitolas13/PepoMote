package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.net.Discovery
import dev.pepotech.pepomote.net.ReceiverInfo
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlinx.coroutines.delay

@Composable
fun PairScreen(onScanQr: () -> Unit, onBack: () -> Unit) {
    var receivers by remember { mutableStateOf(listOf<ReceiverInfo>()) }
    var scanning by remember { mutableStateOf(true) }

    LaunchedEffect(Unit) {
        while (true) {
            scanning = true
            receivers = Discovery.scan()
            scanning = false
            delay(2500)
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .padding(horizontal = 20.dp)
    ) {
        Spacer(Modifier.height(24.dp))
        Text("Conectar", style = MaterialTheme.typography.headlineMedium)
        Text(
            "Abre PepoMote en tu PC y escanea su QR",
            style = MaterialTheme.typography.bodyMedium
        )
        Spacer(Modifier.height(20.dp))

        Button(
            onClick = onScanQr,
            modifier = Modifier
                .fillMaxWidth()
                .height(64.dp),
            shape = MaterialTheme.shapes.medium,
            colors = ButtonDefaults.buttonColors(containerColor = PepoColors.Blue)
        ) {
            Text("Escanear QR del PC", style = MaterialTheme.typography.titleMedium.copy(color = PepoColors.Card))
        }

        Spacer(Modifier.height(24.dp))
        Text(
            if (scanning) "Buscando receptores en tu red…"
            else if (receivers.isEmpty()) "Ningún receptor a la vista (el QR funciona igualmente)"
            else "En tu red:",
            style = MaterialTheme.typography.bodyMedium
        )
        Spacer(Modifier.height(10.dp))

        LazyColumn(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            items(receivers) { r ->
                Card(
                    shape = MaterialTheme.shapes.medium,
                    colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
                    border = BorderStroke(1.5.dp, PepoColors.CardBorder),
                    elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
                ) {
                    Row(modifier = Modifier.padding(16.dp)) {
                        Column {
                            Text(r.name, style = MaterialTheme.typography.titleMedium)
                            Text(
                                "${r.host}:${r.tcpPort} — escanea su QR para emparejar",
                                style = MaterialTheme.typography.bodyMedium
                            )
                        }
                    }
                }
            }
        }

        Spacer(Modifier.weight(1f))
        TextButton(onClick = onBack, modifier = Modifier.fillMaxWidth()) {
            Text("Volver", color = PepoColors.TextDim)
        }
        Spacer(Modifier.height(16.dp))
    }
}
