package dev.pepotech.pepomote.server

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.server.core.ReceiverMode
import dev.pepotech.pepomote.server.setup.*
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
@OptIn(ExperimentalLayoutApi::class)
fun ServerScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val state by ServerState.flow.collectAsState()
    val jobs by ServerSetupJobs.flow.collectAsState()
    var selectedName by rememberSaveable { mutableStateOf(state.receiver?.mode?.name ?: "Dolphin") }
    // Dolphin y Eden se preparan por su carpeta (EmulatorTarget); RetroArch tiene su propia tarjeta
    val retroSelected = selectedName == ReceiverMode.RetroArch.name
    val target = EmulatorTarget.entries.firstOrNull { it.name == selectedName } ?: EmulatorTarget.Dolphin
    var resumeVersion by remember { mutableIntStateOf(0) }
    var statuses by remember { mutableStateOf(emptyMap<EmulatorTarget, SetupStatus>()) }
    var permissionTarget by rememberSaveable { mutableStateOf<String?>(null) }
    var pendingTarget by rememberSaveable { mutableStateOf<String?>(null) }
    var restartTarget by rememberSaveable { mutableStateOf<String?>(null) }
    var visitedAppInfo by rememberSaveable { mutableStateOf(false) }
    var openedInitialize by rememberSaveable { mutableStateOf(false) }
    var localError by rememberSaveable { mutableStateOf<String?>(null) }
    var showErrorDetails by remember { mutableStateOf(false) }
    var restoreTarget by remember { mutableStateOf<EmulatorTarget?>(null) }
    var options by rememberSaveable { mutableStateOf(false) }
    var notice by rememberSaveable { mutableStateOf<String?>(null) }

    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) { resumeVersion++; ServerSetupJobs.refresh(context) }
        }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }
    LaunchedEffect(Unit) { ServerSetupJobs.refresh(context) }
    LaunchedEffect(state.receiver?.mode) { state.receiver?.mode?.let { selectedName = it.name } }
    LaunchedEffect(jobs.version, resumeVersion) {
        if (!jobs.busy) statuses = withContext(Dispatchers.IO) {
            EmulatorTarget.entries.associateWith { EmulatorSetup.setupStatus(context, it) }
        }
    }
    LaunchedEffect(jobs.version) {
        jobs.result?.let { result ->
            notice = if (result.preservedUserChanges.isEmpty()) null
                else context.getString(R.string.server_preserved_changes, result.preservedUserChanges.size)
            if (result.needsRestart && jobs.restart[result.target] != null) {
                selectedName = result.target.name
                restartTarget = result.target.name
                visitedAppInfo = false
            }
        }
        if (jobs.error != null) openedInitialize = false
    }

    fun players(): List<SetupPlayer> = state.receiver?.peers.orEmpty()
        .map { SetupPlayer(it.slot, it.ownNunchuk) }.ifEmpty { listOf(SetupPlayer(0)) }

    val picker = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val chosen = pendingTarget?.let { name -> EmulatorTarget.entries.firstOrNull { it.name == name } }
        pendingTarget = null
        val uri = result.data?.data
        if (result.resultCode == android.app.Activity.RESULT_OK && chosen != null && uri != null) {
            try {
                val rw = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                if ((result.data?.flags ?: 0) and rw != rw) throw SecurityException()
                context.contentResolver.takePersistableUriPermission(uri, rw)
                ServerSetupJobs.prepare(context, chosen, uri, players())
            } catch (error: Exception) { localError = setupErrorText(context, error) }
        }
    }

    fun requestPermission(chosen: EmulatorTarget) {
        permissionTarget = null
        val initialUri = EmulatorSetup.initialUri(context, chosen)
        if (initialUri == null) {
            localError = context.getString(R.string.server_emulator_missing)
            resumeVersion++
            return
        }
        pendingTarget = chosen.name
        try {
            picker.launch(Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or Intent.FLAG_GRANT_PREFIX_URI_PERMISSION)
                .putExtra(DocumentsContract.EXTRA_INITIAL_URI, initialUri))
        } catch (_: ActivityNotFoundException) {
            pendingTarget = null
            localError = context.getString(R.string.server_no_picker)
        }
    }

    fun configure(chosen: EmulatorTarget) {
        notice = null
        if (!ServerSetupJobs.prepare(context, chosen, players())) permissionTarget = chosen.name
    }

    fun openEmulator(chosen: EmulatorTarget, confirmRestart: Boolean = false, initialize: Boolean = false) {
        if (!initialize && statuses[chosen]?.recoveryRequired == true) {
            localError = context.getString(R.string.setup_error_recovery)
            return
        }
        if (!initialize && !state.running) {
            localError = context.getString(R.string.server_start_before_open)
            return
        }
        if (!initialize && !confirmRestart && jobs.restart[chosen] != null) {
            restartTarget = chosen.name
            visitedAppInfo = false
            return
        }
        val packageName = jobs.restart[chosen] ?: EmulatorSetup.installedPackage(context, chosen)
        val launch = packageName?.let { context.packageManager.getLaunchIntentForPackage(it) }
        if (launch == null) { localError = context.getString(R.string.server_emulator_missing); return }
        try {
            if (!initialize) ServerForegroundService.setMode(if (chosen == EmulatorTarget.Dolphin) ReceiverMode.Dolphin else ReceiverMode.Eden)
            context.startActivity(launch)
            if (initialize) openedInitialize = true
            if (confirmRestart) {
                ServerSetupJobs.restartHandled(context, packageName)
                restartTarget = null
            }
        } catch (_: ActivityNotFoundException) { localError = context.getString(R.string.server_emulator_missing) }
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize().background(PepoColors.Background)
            .statusBarsPadding().navigationBarsPadding().displayCutoutPadding(),
        contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.channel_server), style = MaterialTheme.typography.headlineLarge,
                    modifier = Modifier.weight(1f))
                TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
            }
            Text(stringResource(R.string.server_intro), style = MaterialTheme.typography.bodyMedium)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(when {
                    state.running -> R.string.server_running_short
                    state.starting -> R.string.server_starting
                    else -> R.string.server_stopped
                }), color = if (state.running) PepoColors.Ok else PepoColors.Text,
                    style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                TextButton(onClick = {
                    if (state.active) ServerForegroundService.stop(context) else ServerForegroundService.start(context)
                }, enabled = !jobs.busy) { Text(stringResource(if (state.active) R.string.server_stop else R.string.server_start)) }
            }
            state.error?.let { Text(it, color = PepoColors.Error) }
        }
        if (state.running) item { ServerPairingCard(state) }
        item {
            ServerCard {
                Text(stringResource(R.string.server_choose_game), style = MaterialTheme.typography.titleLarge)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    ReceiverMode.entries.forEach { choice ->
                        FilterChip(selected = choice.name == selectedName, enabled = !jobs.busy,
                            onClick = {
                                selectedName = choice.name; options = false
                                ServerForegroundService.setMode(choice)
                            }, label = { Text(choice.name, maxLines = 1, softWrap = false) },
                            colors = FilterChipDefaults.filterChipColors(selectedContainerColor = PepoColors.Blue,
                                selectedLabelColor = PepoColors.OnAccent, labelColor = PepoColors.Text))
                    }
                }
                if (retroSelected) RetroArchCard(state, onError = { localError = it }) else {
                val status = statuses[target]
                val pendingRestart = jobs.restart[target] != null
                val changedPlayers = status?.prepared == true && state.receiver?.peers?.isNotEmpty() == true &&
                    players().any { it !in status.configuredPlayers }
                Text(stringResource(if (target == EmulatorTarget.Dolphin) R.string.server_dolphin_details else R.string.server_eden_details),
                    style = MaterialTheme.typography.bodyMedium)
                when {
                    jobs.busy -> {
                        LinearProgressIndicator(Modifier.fillMaxWidth())
                        Text(stringResource(R.string.server_preparing))
                    }
                    status == null -> Text(stringResource(R.string.server_checking_target, target.name))
                    status.packageName == null -> {
                        Text(stringResource(if (status.backupPackageName != null) R.string.server_saved_variant_missing
                            else R.string.server_install_first, target.name))
                        if (status.backupPackageName != null) {
                            TextButton(onClick = { options = !options }) { Text(stringResource(R.string.server_saved_variant_details)) }
                            if (options) Text(status.backupPackageName, style = MaterialTheme.typography.bodySmall)
                        }
                        Button(onClick = {
                            try { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(if (target == EmulatorTarget.Dolphin)
                                "https://dolphin-emu.org/download/" else "https://eden-emu.dev/"))) }
                            catch (_: ActivityNotFoundException) { localError = context.getString(R.string.server_emulator_missing) }
                        }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.server_install_target, target.name)) }
                    }
                    status.recoveryRequired -> {
                        Text(stringResource(R.string.setup_error_recovery), color = PepoColors.Warn)
                        Button(onClick = { restoreTarget = target }, modifier = Modifier.fillMaxWidth()) {
                            Text(stringResource(R.string.server_restore))
                        }
                    }
                    pendingRestart -> {
                        Text(stringResource(R.string.server_controls_pending), color = PepoColors.Blue)
                        Button(onClick = { restartTarget = target.name; visitedAppInfo = false },
                            enabled = state.running, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.server_finish_setup)) }
                    }
                    status.prepared && !changedPlayers -> {
                        Text(stringResource(R.string.server_controls_ready), color = PepoColors.Ok,
                            style = MaterialTheme.typography.titleMedium)
                        Text(stringResource(R.string.server_ready_hint))
                        Button(onClick = { openEmulator(target) }, enabled = state.running,
                            modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.server_play_target, target.name)) }
                    }
                    else -> {
                        Text(stringResource(if (changedPlayers) R.string.server_update_needed else R.string.server_setup_hint))
                        if (status.permissionUri != null) Text(stringResource(R.string.server_permission_saved),
                            style = MaterialTheme.typography.bodySmall)
                        Button(onClick = { configure(target) }, enabled = state.running,
                            modifier = Modifier.fillMaxWidth()) { Text(if (changedPlayers) stringResource(R.string.server_update_controls)
                            else stringResource(R.string.server_configure_target, target.name)) }
                    }
                }
                notice?.let { Text(it, color = PepoColors.Warn) }
                if (status != null && status.packageName != null) {
                    TextButton(onClick = { options = !options }) { Text(stringResource(R.string.server_control_options)) }
                    if (options) {
                        OutlinedButton(onClick = { configure(target) }, enabled = !jobs.busy && state.running) {
                            Text(stringResource(R.string.server_update_controls))
                        }
                        if (status.hasBackup) TextButton(onClick = { restoreTarget = target }, enabled = !jobs.busy) {
                            Text(stringResource(R.string.server_restore))
                        }
                        Text(stringResource(R.string.server_change_players_hint), style = MaterialTheme.typography.bodySmall)
                    }
                }
                if (target == EmulatorTarget.Eden) Text(stringResource(R.string.server_eden_motion_limit),
                    style = MaterialTheme.typography.bodySmall)
                if (state.running) Text(stringResource(if ((state.receiver?.dsuClients ?: 0) > 0)
                    R.string.server_emulator_connected else R.string.server_emulator_waiting),
                    style = MaterialTheme.typography.bodySmall, color = PepoColors.TextDim)
                }
            }
        }
        item { Text(stringResource(R.string.server_background_hint), style = MaterialTheme.typography.bodySmall) }
    }

    permissionTarget?.let { name ->
        val chosen = EmulatorTarget.valueOf(name)
        AlertDialog(onDismissRequest = { permissionTarget = null },
            containerColor = PepoColors.Card,
            title = { Text(stringResource(R.string.server_permission_title, name)) },
            text = { Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.server_permission_hint, name), color = PepoColors.Text)
                Text(stringResource(R.string.server_permission_steps), color = PepoColors.Text)
                Text(stringResource(R.string.server_permission_fallback, name), style = MaterialTheme.typography.bodySmall)
            } },
            confirmButton = { TextButton(onClick = { requestPermission(chosen) }) { Text(stringResource(R.string.server_permission_action)) } },
            dismissButton = { TextButton(onClick = { permissionTarget = null }) { Text(stringResource(R.string.cancel)) } })
    }
    restartTarget?.let { name ->
        val chosen = EmulatorTarget.valueOf(name)
        val packageName = jobs.restart[chosen]
        if (packageName != null && statuses[chosen]?.recoveryRequired == false) AlertDialog(onDismissRequest = { restartTarget = null },
            containerColor = PepoColors.Card,
            title = { Text(stringResource(R.string.server_restart_title, name)) },
            text = { Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.server_restart_simple, name), color = PepoColors.Text)
                Text(stringResource(R.string.server_restart_taps), color = PepoColors.Text)
                OutlinedButton(onClick = {
                    try {
                        context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
                        visitedAppInfo = true
                    } catch (_: ActivityNotFoundException) { localError = context.getString(R.string.server_app_info_missing) }
                }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.server_restart_action, name), color = PepoColors.Blue) }
            } },
            confirmButton = { TextButton(enabled = visitedAppInfo && state.running && !jobs.busy,
                onClick = { openEmulator(chosen, confirmRestart = true) }) {
                    Text(stringResource(R.string.server_restart_finish), color = if (visitedAppInfo && state.running && !jobs.busy)
                        PepoColors.Blue else PepoColors.TextDim.copy(alpha = 0.5f))
                } },
            dismissButton = { TextButton(onClick = { restartTarget = null }) { Text(stringResource(R.string.back)) } })
    }
    restoreTarget?.let { chosen ->
        AlertDialog(onDismissRequest = { restoreTarget = null }, containerColor = PepoColors.Card,
            title = { Text(stringResource(R.string.server_restore)) },
            text = { Text(stringResource(R.string.server_restore_hint, chosen.name)) },
            confirmButton = { TextButton(onClick = { restoreTarget = null; ServerSetupJobs.restore(context, chosen) }) {
                Text(stringResource(R.string.server_restore)) } },
            dismissButton = { TextButton(onClick = { restoreTarget = null }) { Text(stringResource(R.string.cancel)) } })
    }
    val error = jobs.error
    val errorTarget = jobs.target ?: target
    if ((error as? SetupException)?.reason == SetupErrorReason.Initialize && localError == null) {
        AlertDialog(onDismissRequest = { ServerSetupJobs.dismissError() },
            containerColor = PepoColors.Card,
            title = { Text(stringResource(R.string.server_initial_open, errorTarget.name)) },
            text = { Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.server_initialize_hint, errorTarget.name))
                Button(onClick = { openEmulator(errorTarget, initialize = true) }) {
                    Text(stringResource(R.string.server_open_emulator, errorTarget.name))
                }
            } },
            confirmButton = { TextButton(enabled = openedInitialize, onClick = {
                ServerSetupJobs.dismissError(); configure(errorTarget)
            }) { Text(stringResource(R.string.server_initialize_continue)) } },
            dismissButton = { TextButton(onClick = { ServerSetupJobs.dismissError() }) { Text(stringResource(R.string.cancel)) } })
    } else if (error != null || localError != null) {
        AlertDialog(onDismissRequest = { localError = null; ServerSetupJobs.dismissError() },
            containerColor = PepoColors.Card,
            text = { Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(localError ?: setupErrorText(context, error!!))
                if (error != null) {
                    TextButton(onClick = { showErrorDetails = !showErrorDetails }) { Text(stringResource(R.string.server_error_details)) }
                    if (showErrorDetails) Text(error.message.orEmpty(), style = MaterialTheme.typography.bodySmall)
                }
            } },
            confirmButton = { TextButton(onClick = { localError = null; ServerSetupJobs.dismissError() }) { Text(stringResource(R.string.server_done)) } })
    }
}

@Composable
internal fun ServerCard(content: @Composable ColumnScope.() -> Unit) {
    Card(Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = PepoColors.Card), border = BorderStroke(1.dp, PepoColors.CardBorder)) {
        Column(Modifier.fillMaxWidth().padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = content)
    }
}
