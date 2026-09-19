package dev.pepotech.pepomote.ui.components

import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.control.LocaleHelper
import dev.pepotech.pepomote.service.UpdateCheckStatus
import dev.pepotech.pepomote.service.UpdateFailure
import dev.pepotech.pepomote.service.UpdateNotice
import dev.pepotech.pepomote.service.UpdatePhase
import dev.pepotech.pepomote.service.UpdateUiState
import dev.pepotech.pepomote.ui.theme.PepoColors

@Composable
fun UpdateHost(activity: ComponentActivity, foreground: Boolean, safeScreen: Boolean, canOffer: Boolean) {
    val state by UpdateNotice.state.collectAsState()
    val safe by rememberUpdatedState(safeScreen)
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        UpdateNotice.permissionReturned(activity)
    }
    val installer = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        UpdateNotice.installerReturned()
    }
    fun install() = UpdateNotice.install(activity,
        canLaunch = { safe && activity.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) },
        launch = { intent, isPermission -> if (isPermission) permission.launch(intent) else installer.launch(intent) })

    LaunchedEffect(state.release?.version, foreground, canOffer) {
        UpdateNotice.maybeOffer(activity, foreground, canOffer)
    }
    LaunchedEffect(safeScreen) { if (!safeScreen) UpdateNotice.close() }
    LaunchedEffect(state.phase, state.autoInstall, state.dialog, foreground, safeScreen) {
        if (state.phase == UpdatePhase.Ready && state.autoInstall && state.dialog && foreground && safeScreen) install()
    }
    if (state.dialog && foreground && safeScreen) UpdateDialog(state, LocaleHelper.activeCode(activity),
        onDownload = { UpdateNotice.download(activity) }, onInstall = { install() },
        onCancel = { UpdateNotice.cancel() }, onClose = { UpdateNotice.close() })
}

@Composable
fun UpdateDialog(state: UpdateUiState, language: String, onDownload: () -> Unit, onInstall: () -> Unit, onCancel: () -> Unit, onClose: () -> Unit) {
    val release = state.release ?: return
    AlertDialog(
        containerColor = PepoColors.Card,
        titleContentColor = PepoColors.Text,
        textContentColor = PepoColors.Text,
        onDismissRequest = onClose,
        title = { Text(stringResource(if (state.installable) R.string.update_available else R.string.update_installed, release.version.toString())) },
        text = {
            Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (state.installable) Text(stringResource(R.string.update_question))
                for (note in release.notesFor(language)) Text("• $note", style = MaterialTheme.typography.bodyMedium)
                if (state.installable) Text(stringResource(R.string.update_install_hint), style = MaterialTheme.typography.bodySmall)
                UpdateStatus(state)
            }
        },
        confirmButton = {
            if (state.installable) when (state.phase) {
                UpdatePhase.Downloading, UpdatePhase.Verifying -> TextButton(onClick = onCancel, enabled = !state.cancelling) { Text(stringResource(R.string.update_cancel_download)) }
                UpdatePhase.Ready, UpdatePhase.InstallerOpened -> TextButton(onClick = onInstall) { Text(stringResource(R.string.update_install)) }
                UpdatePhase.PermissionRequired -> TextButton(onClick = onInstall) { Text(stringResource(R.string.update_permission_button)) }
                UpdatePhase.Failed -> TextButton(onClick = if (state.failure == UpdateFailure.Installer) onInstall else onDownload,
                    enabled = state.check != UpdateCheckStatus.Checking) { Text(stringResource(R.string.update_retry)) }
                else -> TextButton(onClick = onDownload, enabled = state.check != UpdateCheckStatus.Checking) { Text(stringResource(R.string.update_download_install)) }
            }
        },
        dismissButton = { TextButton(onClick = onClose) { Text(stringResource(if (state.installable) R.string.update_later else R.string.update_close)) } },
    )
}

@Composable
private fun UpdateStatus(state: UpdateUiState) {
    if (state.check == UpdateCheckStatus.Failed) Text(stringResource(R.string.update_check_failed))
    if (state.cancelling) { Text(stringResource(R.string.update_cancelling)); return }
    when (state.phase) {
        UpdatePhase.Downloading -> {
            val fraction = (state.bytes.toFloat() / (state.release?.asset?.size ?: 1)).coerceIn(0f, 1f)
            LinearProgressIndicator(progress = { fraction }, modifier = Modifier.fillMaxWidth(), trackColor = PepoColors.CardBorder)
            Text(stringResource(R.string.update_progress, (fraction * 100).toInt()))
        }
        UpdatePhase.Verifying -> { LinearProgressIndicator(Modifier.fillMaxWidth()); Text(stringResource(R.string.update_verifying)) }
        UpdatePhase.Ready -> Text(stringResource(R.string.update_ready))
        UpdatePhase.PermissionRequired -> Text(stringResource(R.string.update_permission_hint))
        UpdatePhase.InstallerOpened -> Text(stringResource(R.string.update_installer_hint))
        UpdatePhase.Cancelled -> Text(stringResource(R.string.update_cancelled))
        UpdatePhase.Failed -> Text(stringResource(when (state.failure) {
            UpdateFailure.Verification -> R.string.update_verify_failed
            UpdateFailure.Installer -> R.string.update_install_failed
            else -> R.string.update_download_failed
        }))
        else -> Unit
    }
}

@Composable
fun UpdateSettingsCard() {
    val context = LocalContext.current
    val state by UpdateNotice.state.collectAsState()
    Card(Modifier.fillMaxWidth(), colors = CardDefaults.cardColors(containerColor = PepoColors.Card)) {
        Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(stringResource(R.string.update_status_title), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(when (state.check) {
                UpdateCheckStatus.Checking -> R.string.update_checking
                UpdateCheckStatus.Current -> R.string.update_current
                UpdateCheckStatus.Failed -> R.string.update_check_failed
                UpdateCheckStatus.Available -> R.string.update_found
                else -> R.string.update_check_hint
            }))
            state.release?.let { release ->
                Text(stringResource(if (state.installable) R.string.update_available else R.string.update_installed, release.version.toString()))
                for (note in release.notesFor(LocaleHelper.activeCode(context))) Text("• $note", style = MaterialTheme.typography.bodyMedium)
                UpdateStatus(state)
                TextButton(onClick = { UpdateNotice.show(context) }) { Text(stringResource(if (state.installable) R.string.update_view else R.string.update_view_notes)) }
            }
            TextButton(onClick = { UpdateNotice.checkIfDue(context, manual = true) }, enabled = !state.busy && state.check != UpdateCheckStatus.Checking) {
                Text(stringResource(R.string.update_check_now))
            }
        }
    }
}
