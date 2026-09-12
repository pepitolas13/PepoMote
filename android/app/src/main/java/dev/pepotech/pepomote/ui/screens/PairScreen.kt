package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AlertDialog
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.net.Discovery
import dev.pepotech.pepomote.net.PairList
import dev.pepotech.pepomote.net.Pairing
import dev.pepotech.pepomote.net.ReceiverInfo
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlinx.coroutines.delay
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

/**
 * Conectar: tus PCs guardados (toca uno para conectar; mantén pulsado para
 * olvidarlo), el escáner del QR del PC y los receptores nuevos vistos en la
 * red. `reason` (no null) = se llega aquí porque el PC no responde o ya no
 * reconoce el emparejamiento: se explica arriba, en rojo.
 */
@Composable
fun PairScreen(
    onScanQr: () -> Unit,
    onBack: () -> Unit,
    reason: String? = null,
    saved: List<Pairing> = emptyList(),
    currentToken: String? = null,
    onChoose: (Pairing) -> Unit = {},
    onForget: (Pairing) -> Unit = {}
) {
    var receivers by remember { mutableStateOf(listOf<ReceiverInfo>()) }
    var scanning by remember { mutableStateOf(true) }
    var forgetting by remember { mutableStateOf<Pairing?>(null) }

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
        Text(
            if (reason != null) stringResource(R.string.pair_title_other) else stringResource(R.string.channel_connect),
            style = MaterialTheme.typography.headlineMedium
        )
        Text(
            if (saved.isEmpty()) stringResource(R.string.pair_sub_scan)
            else stringResource(R.string.pair_sub_saved),
            style = MaterialTheme.typography.bodyMedium
        )
        if (reason != null) {
            Spacer(Modifier.height(14.dp))
            Card(
                shape = MaterialTheme.shapes.medium,
                colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
                border = BorderStroke(1.5.dp, PepoColors.Error),
                elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
            ) {
                Text(
                    reason,
                    style = MaterialTheme.typography.bodyMedium.copy(color = PepoColors.Text),
                    modifier = Modifier.padding(16.dp)
                )
            }
        }
        Spacer(Modifier.height(20.dp))

        val unknown = PairList.unknown(receivers, saved)
        LazyColumn(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            if (saved.isNotEmpty()) {
                item { Text(stringResource(R.string.your_pcs), style = MaterialTheme.typography.titleMedium) }
                items(saved, key = { it.token }) { p ->
                    SavedPcCard(
                        p,
                        current = p.token == currentToken,
                        online = PairList.isOnline(p, receivers),
                        onClick = { onChoose(p) },
                        onLongClick = { forgetting = p }
                    )
                }
                item { Spacer(Modifier.height(4.dp)) }
            }
            item {
                Button(
                    onClick = onScanQr,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(64.dp),
                    shape = MaterialTheme.shapes.medium,
                    colors = ButtonDefaults.buttonColors(containerColor = PepoColors.Blue)
                ) {
                    Text(
                        if (saved.isEmpty()) stringResource(R.string.scan_qr) else stringResource(R.string.scan_qr_other),
                        style = MaterialTheme.typography.titleMedium.copy(color = PepoColors.OnAccent)
                    )
                }
            }
            item {
                Spacer(Modifier.height(10.dp))
                Text(
                    when {
                        scanning && receivers.isEmpty() -> stringResource(R.string.searching)
                        unknown.isEmpty() && saved.isEmpty() -> stringResource(R.string.no_receivers)
                        unknown.isEmpty() -> stringResource(R.string.no_new_pcs)
                        else -> stringResource(R.string.on_your_network)
                    },
                    style = MaterialTheme.typography.bodyMedium
                )
            }
            items(unknown) { r ->
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
                                stringResource(R.string.receiver_hint, "${r.host}:${r.tcpPort}"),
                                style = MaterialTheme.typography.bodyMedium
                            )
                        }
                    }
                }
            }
        }

        TextButton(onClick = onBack, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(R.string.back), color = PepoColors.TextDim)
        }
        Spacer(Modifier.height(16.dp))
    }

    forgetting?.let { p ->
        AlertDialog(
            onDismissRequest = { forgetting = null },
            containerColor = PepoColors.Card,
            title = { Text(stringResource(R.string.forget_title, p.pcName), style = MaterialTheme.typography.titleLarge) },
            text = {
                Text(
                    stringResource(R.string.forget_body),
                    style = MaterialTheme.typography.bodyMedium
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    onForget(p)
                    forgetting = null
                }) { Text(stringResource(R.string.forget), color = PepoColors.Error) }
            },
            dismissButton = {
                TextButton(onClick = { forgetting = null }) { Text(stringResource(R.string.cancel), color = PepoColors.TextDim) }
            }
        )
    }
}

/** Un PC guardado: punto verde si se ve en la red, borde azul si es el actual. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SavedPcCard(
    p: Pairing,
    current: Boolean,
    online: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit
) {
    Card(
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
        border = BorderStroke(1.5.dp, if (current) PepoColors.Blue else PepoColors.CardBorder),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp),
        modifier = Modifier
            .fillMaxWidth()
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Box(
                Modifier
                    .size(10.dp)
                    .background(if (online) PepoColors.Ok else PepoColors.TextDim, CircleShape)
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(p.pcName, style = MaterialTheme.typography.titleMedium)
                Text(
                    if (online) stringResource(R.string.pc_online, "${p.host}:${p.port}") else "${p.host}:${p.port}",
                    style = MaterialTheme.typography.bodyMedium
                )
            }
            if (current) {
                Text(stringResource(R.string.current), style = MaterialTheme.typography.bodyMedium.copy(color = PepoColors.Blue))
            }
        }
    }
}
