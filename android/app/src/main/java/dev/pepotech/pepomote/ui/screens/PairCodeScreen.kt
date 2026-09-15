package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.net.PairList
import dev.pepotech.pepomote.net.ReceiverCapabilities
import dev.pepotech.pepomote.net.ReceiverInfo
import dev.pepotech.pepomote.ui.theme.PepoColors

internal val ReceiverInfoSaver = listSaver<ReceiverInfo?, Any>(
    save = { receiver -> receiver?.let { listOf(it.name, it.host, it.tcpPort, it.platform) } ?: emptyList() },
    restore = { values ->
        if (values.isEmpty()) null
        else ReceiverInfo(values[0] as String, values[1] as String, values[2] as Int, values[3] as String)
    }
)

/** A full page keeps the code field and Connect button reachable above the keyboard. */
@Composable
internal fun PairCodeScreen(
    receivers: List<ReceiverInfo>,
    scanning: Boolean,
    selected: ReceiverInfo?,
    code: String,
    onSelect: (ReceiverInfo) -> Unit,
    onCodeChange: (String) -> Unit,
    onBack: () -> Unit,
    onScanQr: () -> Unit,
    onConnect: () -> Unit,
) {
    Column(
        Modifier.fillMaxSize().background(PepoColors.Background).statusBarsPadding()
            .navigationBarsPadding().displayCutoutPadding().imePadding()
            .verticalScroll(rememberScrollState()).padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(R.string.pair_with_code), style = MaterialTheme.typography.headlineMedium,
                modifier = Modifier.weight(1f))
            TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
        }
        if (selected == null) {
            Text(stringResource(R.string.pair_code_choose), color = PepoColors.Text)
            if (receivers.isEmpty()) {
                Text(stringResource(if (scanning) R.string.searching else R.string.pair_code_not_found),
                    color = PepoColors.TextDim)
            }
            receivers.forEach { receiver ->
                Card(
                    modifier = Modifier.fillMaxWidth().clickable { onSelect(receiver) },
                    colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
                    border = BorderStroke(1.5.dp, PepoColors.CardBorder),
                    shape = MaterialTheme.shapes.medium
                ) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(receiver.name, style = MaterialTheme.typography.titleMedium)
                        Text(stringResource(
                            if (receiver.platform == ReceiverCapabilities.ANDROID) R.string.android_receiver_hint
                            else R.string.receiver_hint, "${receiver.host}:${receiver.tcpPort}"),
                            style = MaterialTheme.typography.bodyMedium)
                    }
                }
            }
            OutlinedButton(onClick = onScanQr, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.scan_qr))
            }
        } else {
            Text(selected.name, style = MaterialTheme.typography.titleLarge, color = PepoColors.Blue)
            Text(stringResource(if (selected.platform == ReceiverCapabilities.ANDROID)
                R.string.pair_code_android_hint else R.string.pair_code_pc_hint), color = PepoColors.Text)
            OutlinedTextField(
                value = code, onValueChange = onCodeChange,
                label = { Text(stringResource(R.string.server_pair_code)) },
                modifier = Modifier.fillMaxWidth(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword), singleLine = true
            )
            Button(onClick = onConnect, enabled = PairList.isPairingCode(code), modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.channel_connect))
            }
        }
    }
}
