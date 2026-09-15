package dev.pepotech.pepomote.server

import android.content.ActivityNotFoundException
import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.ui.theme.PepoColors

/** Normal pairing has one automatic address. Routed VPN access is an explicit, separate choice. */
@Composable
internal fun ServerPairingCard(state: ServerUiState) {
    val context = LocalContext.current
    var useVpn by rememberSaveable { mutableStateOf(false) }
    var chosenLocal by rememberSaveable { mutableStateOf<String?>(null) }
    var showDetails by rememberSaveable { mutableStateOf(false) }
    var showVpnHelp by rememberSaveable { mutableStateOf(false) }
    var showQr by rememberSaveable { mutableStateOf(true) }
    var shareError by remember { mutableStateOf(false) }
    val peers = state.receiver?.peers.orEmpty()
    LaunchedEffect(peers.size) { showQr = peers.isEmpty() }
    val host = if (useVpn) state.vpnHosts.firstOrNull()
        else chosenLocal?.takeIf { it in state.hosts } ?: state.hosts.firstOrNull()
    val pairUrl = state.config?.let { config -> host?.let { ServerIdentity.pairUrl(config, it) } }

    ServerCard {
        Text(stringResource(R.string.server_pair_step), style = MaterialTheme.typography.titleLarge)
        if (peers.isNotEmpty()) {
            peers.forEach { peer ->
                Text(stringResource(R.string.server_player, peer.slot + 1, peer.name),
                    style = MaterialTheme.typography.titleMedium)
                Text(stringResource(if (peer.lastInputAgeMs != null && peer.lastInputAgeMs < 1000)
                    R.string.server_receiving else R.string.server_waiting_input, peer.inputHz.toInt()),
                    color = if (peer.lastInputAgeMs != null && peer.lastInputAgeMs < 1000) PepoColors.Ok else PepoColors.TextDim,
                    style = MaterialTheme.typography.bodyMedium)
            }
            TextButton(onClick = { showQr = !showQr }) {
                Text(stringResource(if (showQr) R.string.server_hide_qr else R.string.server_add_controller))
            }
        }
        if (showQr) {
            Text(stringResource(if (useVpn) R.string.server_remote_active else R.string.server_local_network),
                style = MaterialTheme.typography.titleMedium, color = PepoColors.Blue)
            Text(stringResource(if (useVpn) R.string.server_share_hint else R.string.server_pair_hint),
                style = MaterialTheme.typography.bodyMedium)
            if (pairUrl != null) {
                val qr = remember(pairUrl) {
                    val matrix = QRCodeWriter().encode(pairUrl, BarcodeFormat.QR_CODE, 400, 400,
                        mapOf(EncodeHintType.MARGIN to 3))
                    Bitmap.createBitmap(matrix.width, matrix.height, Bitmap.Config.ARGB_8888).apply {
                        setPixels(IntArray(matrix.width * matrix.height) { index ->
                            if (matrix[index % matrix.width, index / matrix.width]) android.graphics.Color.BLACK
                            else android.graphics.Color.WHITE
                        }, 0, matrix.width, 0, 0, matrix.width, matrix.height)
                    }.asImageBitmap()
                }
                Image(qr, stringResource(R.string.server_qr_description),
                    Modifier.size(216.dp).align(Alignment.CenterHorizontally))
                Text(state.config!!.name, style = MaterialTheme.typography.bodyMedium)
                if (useVpn) {
                    Button(onClick = {
                        try {
                            context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply {
                                type = "text/plain"
                                putExtra(Intent.EXTRA_TEXT, pairUrl)
                            }, context.getString(R.string.server_share_link)))
                        } catch (_: ActivityNotFoundException) { shareError = true }
                    }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.server_share_link)) }
                } else {
                    Text(stringResource(R.string.server_pair_code), style = MaterialTheme.typography.bodyMedium)
                    Text(state.config.pairCode.chunked(3).joinToString(" "),
                        style = MaterialTheme.typography.headlineLarge.copy(fontFamily = FontFamily.Monospace),
                        color = PepoColors.Blue)
                    Text(stringResource(R.string.server_network_automatic),
                        style = MaterialTheme.typography.bodySmall, color = PepoColors.TextDim)
                }
                TextButton(onClick = { showDetails = !showDetails }) {
                    Text(stringResource(R.string.server_network_details))
                }
                if (showDetails) {
                    Text(stringResource(R.string.server_connection_address, "$host:${state.config.port}"),
                        style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace))
                    if (!useVpn && state.hosts.size > 1) {
                        Text(stringResource(R.string.server_alternate_network), style = MaterialTheme.typography.bodySmall)
                        state.hosts.filter { it != host }.forEach { alternate ->
                            TextButton(onClick = { chosenLocal = alternate }) { Text(alternate) }
                        }
                    }
                }
            } else Text(stringResource(if (useVpn) R.string.server_remote_lost else R.string.server_no_network))
        }
        TextButton(onClick = {
            if (useVpn) { useVpn = false; showDetails = false; showQr = true }
            else showVpnHelp = true
        }) { Text(stringResource(if (useVpn) R.string.server_back_local else R.string.server_other_network)) }
        if (shareError) Text(stringResource(R.string.server_no_share), color = PepoColors.Error)
    }

    if (showVpnHelp) AlertDialog(
        containerColor = PepoColors.Card,
        onDismissRequest = { showVpnHelp = false },
        title = { Text(stringResource(R.string.server_remote_title)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.server_remote_intro))
                Text(stringResource(R.string.server_remote_wireguard), style = MaterialTheme.typography.titleSmall,
                    color = PepoColors.Blue)
                Text(stringResource(R.string.server_remote_wireguard_steps))
                TextButton(onClick = {
                    try { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://www.wireguard.com/install/"))) }
                    catch (_: ActivityNotFoundException) { shareError = true }
                }) { Text(stringResource(R.string.server_remote_wireguard_guide)) }
                HorizontalDivider()
                Text(stringResource(R.string.server_remote_tailscale), style = MaterialTheme.typography.titleSmall)
                Text(stringResource(R.string.server_remote_tailscale_steps))
                TextButton(onClick = {
                    try { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://tailscale.com/docs/install/android"))) }
                    catch (_: ActivityNotFoundException) { shareError = true }
                }) { Text(stringResource(R.string.server_remote_guide)) }
                Text(stringResource(R.string.server_remote_steps))
                Text(stringResource(R.string.server_remote_limits), style = MaterialTheme.typography.bodySmall)
                if (state.vpnHosts.isEmpty()) Text(stringResource(R.string.server_remote_missing))
            }
        },
        confirmButton = {
            TextButton(enabled = state.vpnHosts.isNotEmpty(), onClick = {
                useVpn = true; showQr = true; showDetails = false; showVpnHelp = false
            }) { Text(stringResource(R.string.server_remote_available)) }
        },
        dismissButton = { TextButton(onClick = { showVpnHelp = false }) { Text(stringResource(R.string.back)) } }
    )
}
