package dev.pepotech.pepomote.server

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.server.core.ReceiverMode
import dev.pepotech.pepomote.server.retro.RetroArchApp
import dev.pepotech.pepomote.server.retro.RetroFolder
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlin.math.roundToInt

/**
 * RetroArch en el panel del servidor, con el estilo de las guías de Dolphin y
 * Eden: instalar; activar una sola vez en el propio RetroArch el mando en red
 * y los comandos de red (Android no deja que otra app toque `Android/data`,
 * donde RetroArch guarda su configuración), con comprobación automática y
 * recordada; jugar; y, aparte, la carpeta RetroArch para el mando de cada
 * consola (un permiso de lectura de un toque).
 */
@Composable
internal fun RetroArchCard(state: ServerUiState, onError: (String) -> Unit) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var version by remember { mutableIntStateOf(0) }
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_RESUME) version++ }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }
    val installed = remember(version) { RetroArchApp.installedPackage(context) }
    val verified = RetroArchApp.verifiedVersion(context)
    val folder = remember(version) { RetroFolder.granted(context) }
    val retro = state.receiver?.retro
    val live = state.running && retro?.reachable == true
    var guideOpen by rememberSaveable { mutableStateOf(false) }
    var folderDialog by rememberSaveable { mutableStateOf(false) }

    val picker = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val uri: Uri? = result.data?.data
        if (result.resultCode == Activity.RESULT_OK && uri != null) {
            if (RetroFolder.save(context, uri)) version++ else onError(context.getString(R.string.server_retroarch_folder_denied))
        }
    }

    fun openRetroArch() {
        if (!state.running) { onError(context.getString(R.string.server_start_before_open)); return }
        val launch = RetroArchApp.launchIntent(context)
        if (launch == null) { onError(context.getString(R.string.server_retroarch_missing)); return }
        ServerForegroundService.setMode(ReceiverMode.RetroArch)
        try { context.startActivity(launch) } catch (_: ActivityNotFoundException) { onError(context.getString(R.string.server_retroarch_missing)) }
    }

    Text(stringResource(R.string.server_retroarch_details), style = MaterialTheme.typography.bodyMedium)
    when {
        live -> {
            Text(stringResource(R.string.server_retroarch_connection_ready), color = PepoColors.Ok, style = MaterialTheme.typography.titleMedium)
            Text(stringResource(R.string.server_retroarch_responding, retro?.version ?: "?", (retro?.pollsPerSec ?: 0f).roundToInt()))
            Button(onClick = { openRetroArch() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.server_play_target, "RetroArch")) }
        }
        verified != null && installed != null -> {
            Text(stringResource(R.string.server_retroarch_connection_ready), color = PepoColors.Ok, style = MaterialTheme.typography.titleMedium)
            Text(stringResource(R.string.server_retroarch_verified_hint, verified))
            Button(onClick = { openRetroArch() }, enabled = state.running, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.server_play_target, "RetroArch"))
            }
        }
        installed == null -> {
            Text(stringResource(R.string.server_retroarch_install_first))
            Button(onClick = {
                try { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(RetroArchApp.DOWNLOAD_URL))) }
                catch (_: ActivityNotFoundException) { onError(context.getString(R.string.server_retroarch_missing)) }
            }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.server_install_target, "RetroArch")) }
        }
        else -> {
            Text(stringResource(R.string.server_retroarch_setup_title), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(R.string.server_retroarch_setup_why), style = MaterialTheme.typography.bodySmall)
            RetroArchGuide()
            Button(onClick = { openRetroArch() }, enabled = state.running, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.server_retroarch_open))
            }
            Text(stringResource(if (state.running) R.string.server_retroarch_waiting else R.string.server_start_before_open),
                style = MaterialTheme.typography.bodySmall, color = PepoColors.TextDim)
        }
    }
    if (live || (verified != null && installed != null)) {
        Text(stringResource(R.string.server_retroarch_check_pad), style = MaterialTheme.typography.bodySmall)
        TextButton(onClick = { guideOpen = !guideOpen }) { Text(stringResource(R.string.server_retroarch_show_guide)) }
        if (guideOpen) RetroArchGuide()
    }
    if (state.running && retro?.degraded == true) Text(stringResource(R.string.server_retroarch_degraded), color = PepoColors.Warn)
    Text(stringResource(R.string.server_retroarch_players_hint), style = MaterialTheme.typography.bodySmall)

    HorizontalDivider(color = PepoColors.CardBorder)
    Text(stringResource(R.string.server_retroarch_folder_title), style = MaterialTheme.typography.titleMedium)
    Text(stringResource(R.string.server_retroarch_folder_hint), style = MaterialTheme.typography.bodySmall)
    if (folder == null) {
        OutlinedButton(onClick = { folderDialog = true }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(R.string.server_retroarch_folder_pick), color = PepoColors.Blue)
        }
    } else {
        val located = state.retroFolder?.located
        if (located == false) Text(stringResource(R.string.server_retroarch_folder_missing), color = PepoColors.Warn)
        else Text(stringResource(R.string.server_retroarch_folder_linked), color = PepoColors.Ok)
        val game = retro?.game
        when {
            game != null -> Text(stringResource(R.string.server_retroarch_game, game.title, game.system.ifEmpty { "?" }, game.core),
                style = MaterialTheme.typography.bodySmall)
            located == true -> Text(stringResource(R.string.server_retroarch_no_game), style = MaterialTheme.typography.bodySmall, color = PepoColors.TextDim)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = { folderDialog = true }) { Text(stringResource(R.string.server_retroarch_folder_change)) }
            TextButton(onClick = { RetroFolder.forget(context); version++ }) { Text(stringResource(R.string.server_retroarch_folder_forget)) }
        }
    }

    if (folderDialog) AlertDialog(onDismissRequest = { folderDialog = false }, containerColor = PepoColors.Card,
        title = { Text(stringResource(R.string.server_retroarch_folder_dialog_title)) },
        text = { Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.server_retroarch_folder_dialog_hint), color = PepoColors.Text)
            Text(stringResource(R.string.server_retroarch_folder_dialog_steps), color = PepoColors.Text)
            Text(stringResource(R.string.server_retroarch_folder_dialog_fallback), style = MaterialTheme.typography.bodySmall)
        } },
        confirmButton = { TextButton(onClick = {
            folderDialog = false
            try { picker.launch(RetroFolder.pickerIntent()) } catch (_: ActivityNotFoundException) { onError(context.getString(R.string.server_no_picker)) }
        }) { Text(stringResource(R.string.server_permission_action)) } },
        dismissButton = { TextButton(onClick = { folderDialog = false }) { Text(stringResource(R.string.cancel)) } })
}

@Composable
private fun RetroArchGuide() {
    Text(stringResource(R.string.server_retroarch_steps), modifier = Modifier.padding(start = 4.dp))
}
