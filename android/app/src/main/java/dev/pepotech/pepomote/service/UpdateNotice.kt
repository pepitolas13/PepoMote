package dev.pepotech.pepomote.service

import android.content.Context
import android.content.Intent
import android.util.Log
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.control.UpdateCheck
import dev.pepotech.pepomote.control.UpdateManifest
import dev.pepotech.pepomote.control.UpdateSchedule
import dev.pepotech.pepomote.net.UpdateClient
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

enum class UpdatePhase { Idle, Downloading, Verifying, Ready, PermissionRequired, InstallerOpened, Failed, Cancelled }
enum class UpdateCheckStatus { Idle, Checking, Current, Available, Failed }
enum class UpdateFailure { Download, Verification, Installer }
data class UpdateUiState(
    val release: UpdateManifest? = null,
    val check: UpdateCheckStatus = UpdateCheckStatus.Idle,
    val phase: UpdatePhase = UpdatePhase.Idle,
    val bytes: Long = 0,
    val autoInstall: Boolean = false,
    val dialog: Boolean = false,
    val failure: UpdateFailure? = null,
    val externalPending: Boolean = false,
    val cancelling: Boolean = false,
    val installable: Boolean = true,
) {
    val busy: Boolean get() = phase == UpdatePhase.Downloading || phase == UpdatePhase.Verifying
    fun permissionResult(granted: Boolean): UpdateUiState = copy(
        phase = if (granted) UpdatePhase.Ready else UpdatePhase.PermissionRequired,
        autoInstall = granted, dialog = true, externalPending = false)
    // Returning from the system UI is not proof that installation succeeded.
    fun installerReturned(): UpdateUiState = copy(phase = UpdatePhase.InstallerOpened, autoInstall = false, externalPending = false)
    fun canCheck(checkRunning: Boolean, transferRunning: Boolean): Boolean = !checkRunning && !transferRunning && !externalPending
    fun mayLaunchInstaller(foreground: Boolean): Boolean = dialog && foreground
}

/** Process-owned jobs survive rotations; checks, downloads and installer handoffs never overlap. */
object UpdateNotice {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val mutable = MutableStateFlow(UpdateUiState())
    val state: StateFlow<UpdateUiState> = mutable
    private val _pending = MutableStateFlow<UpdateCheck.Version?>(null)
    val pending: StateFlow<UpdateCheck.Version?> = _pending
    private var initialized = false
    private var checkJob: Job? = null
    private var transferJob: Job? = null

    private fun prefs(context: Context) = context.getSharedPreferences("update_state", Context.MODE_PRIVATE)

    fun current(context: Context): UpdateCheck.Version? = runCatching {
        UpdateCheck.parse(context.packageManager.getPackageInfo(context.packageName, 0).versionName)
    }.getOrNull()

    fun refresh(context: Context) {
        if (initialized) { refreshPending(context); return }
        initialized = true
        val saved = prefs(context).getString("manifest", null)
        val release = saved?.let { runCatching { UpdateManifest.parse(it) }.getOrNull() }
            ?.takeIf { it.version >= (current(context) ?: return@takeIf false) }
        if (release != null) {
            val newer = release.version > requireNotNull(current(context))
            val cached = newer && runCatching { UpdateInstaller.file(context, release).isFile }.getOrDefault(false)
            mutable.value = UpdateUiState(release, if (newer) UpdateCheckStatus.Available else UpdateCheckStatus.Current,
                if (prefs(context).getBoolean("permissionRequested", false) && cached) UpdatePhase.PermissionRequired
                else if (cached) UpdatePhase.Ready else UpdatePhase.Idle, installable = newer)
        }
        refreshPending(context)
    }

    private fun refreshPending(context: Context) {
        _pending.value = if (AppPrefs.updateCheckEnabled(context)) UpdateCheck.pending(current(context),
            mutable.value.release?.version, UpdateCheck.parse(AppPrefs.updateDismissed(context))) else null
    }

    fun checkIfDue(context: Context, manual: Boolean = false) {
        refresh(context)
        if (!mutable.value.canCheck(checkJob?.isActive == true, transferJob?.isActive == true)) return
        val app = context.applicationContext
        val now = System.currentTimeMillis()
        val lastSuccess = if (prefs(app).contains("manifest")) AppPrefs.updateLastMs(app) else 0L
        if (!manual && !UpdateSchedule.due(AppPrefs.updateCheckEnabled(app), lastSuccess,
                prefs(app).getLong("attempt", 0), now)) return
        prefs(app).edit().putLong("attempt", now).apply()
        mutable.update { it.copy(check = UpdateCheckStatus.Checking) }
        checkJob = scope.launch {
            try {
                val release = UpdateClient.latest()
                val installed = requireNotNull(current(app)) { "Cannot read installed version" }
                AppPrefs.setUpdateResult(app, System.currentTimeMillis(), release.version.toString())
                prefs(app).edit().putString("manifest", release.json).apply()
                if (release.version > installed) {
                    mutable.update { previous ->
                        if (previous.release?.version == release.version) previous.copy(release = release, check = UpdateCheckStatus.Available)
                        else UpdateUiState(release, UpdateCheckStatus.Available, dialog = previous.dialog)
                    }
                } else mutable.update { UpdateUiState(
                    release = release.takeIf { candidate -> candidate.version == installed },
                    check = UpdateCheckStatus.Current, dialog = it.dialog, installable = false) }
                refreshPending(app)
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) {
                Log.i("PepoMote/update", "Check unavailable: ${error.message}")
                mutable.update { it.copy(check = UpdateCheckStatus.Failed) }
            }
        }
    }

    fun maybeOffer(context: Context, foreground: Boolean, safeScreen: Boolean) {
        val release = mutable.value.release ?: return
        if (!mutable.value.installable) return
        if (mutable.value.dialog) return
        if (UpdateSchedule.offer(AppPrefs.updateCheckEnabled(context), foreground, safeScreen,
                release.version.toString(), prefs(context).getString("offered", AppPrefs.updateDismissed(context)) ?: "")) {
            prefs(context).edit().putString("offered", release.version.toString()).apply()
            mutable.update { it.copy(dialog = true) }
        }
    }

    fun show(context: Context) {
        mutable.value.release?.let { prefs(context).edit().putString("offered", it.version.toString()).apply() }
        mutable.update { it.copy(dialog = true) }
    }
    fun close() { mutable.update { it.copy(dialog = false, autoInstall = false) } }
    fun dismiss(context: Context, version: UpdateCheck.Version) {
        AppPrefs.setUpdateDismissed(context, version.toString())
        prefs(context).edit().putString("offered", version.toString()).apply()
        refreshPending(context)
        close()
    }
    fun settingsChanged(context: Context) { refreshPending(context); if (AppPrefs.updateCheckEnabled(context)) checkIfDue(context) }

    fun download(context: Context) {
        if (!mutable.value.installable) return
        if (!mutable.value.canCheck(checkJob?.isActive == true, transferJob?.isActive == true)) return
        val release = mutable.value.release ?: return
        val app = context.applicationContext
        mutable.update { it.copy(phase = UpdatePhase.Downloading, bytes = 0, failure = null, autoInstall = true, cancelling = false) }
        transferJob = scope.launch {
            var failure = UpdateFailure.Download
            try {
                // Refresh the mutable 'latest' release before starting a new package transaction.
                // A changed release requires another click with its own notes visible.
                val fresh = try { UpdateClient.latest() } catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Exception) {
                    mutable.update { it.copy(check = UpdateCheckStatus.Failed, phase = UpdatePhase.Idle, autoInstall = false) }
                    return@launch
                }
                prefs(app).edit().putString("manifest", fresh.json).apply()
                AppPrefs.setUpdateResult(app, System.currentTimeMillis(), fresh.version.toString())
                if (fresh.version != release.version || fresh.asset != release.asset) {
                    val newer = current(app)?.let { fresh.version > it } == true
                    mutable.value = if (newer) UpdateUiState(fresh, UpdateCheckStatus.Available, dialog = mutable.value.dialog)
                        else UpdateUiState(check = UpdateCheckStatus.Current)
                    refreshPending(app)
                    return@launch
                }
                val target = UpdateInstaller.file(app, release)
                withContext(Dispatchers.IO) {
                    UpdateInstaller.directory(app).listFiles()?.filter { it.name.endsWith(".apk") || it.name.endsWith(".part") }
                        ?.forEach { require(it.delete()) { "Cannot clear old update cache" } }
                    require(UpdateInstaller.directory(app).usableSpace > release.asset.size + 8 * 1024 * 1024) { "Not enough storage" }
                }
                var lastProgress = 0L
                UpdateClient.download(release, target) { bytes ->
                    val now = System.nanoTime()
                    if (now - lastProgress >= 100_000_000L || bytes == release.asset.size) {
                        lastProgress = now
                        mutable.update { it.copy(bytes = bytes) }
                    }
                }
                failure = UpdateFailure.Verification
                mutable.update { it.copy(phase = UpdatePhase.Verifying) }
                UpdateInstaller.verify(app, release)
                ensureActive()
                mutable.update { it.copy(phase = UpdatePhase.Ready, check = UpdateCheckStatus.Available) }
            } catch (cancelled: CancellationException) {
                mutable.update { it.copy(phase = UpdatePhase.Cancelled, autoInstall = false, cancelling = false) }
                runCatching { UpdateInstaller.file(app, release).delete() }
                throw cancelled
            } catch (error: Exception) {
                Log.i("PepoMote/update", "Package rejected: ${error.message}")
                runCatching { UpdateInstaller.file(app, release).delete() }
                mutable.update { it.copy(phase = UpdatePhase.Failed, failure = failure, autoInstall = false, cancelling = false) }
            }
        }
    }

    fun cancel() {
        val job = transferJob?.takeIf { it.isActive } ?: return
        mutable.update { it.copy(autoInstall = false, cancelling = true) }
        job.cancel()
    }

    /** Reverify immediately before every permission/installer handoff, including after process death. */
    fun install(context: Context, canLaunch: () -> Boolean, launch: (Intent, Boolean) -> Unit) {
        if (!mutable.value.installable) return
        if (!mutable.value.canCheck(checkJob?.isActive == true, transferJob?.isActive == true)) return
        val release = mutable.value.release ?: return
        val app = context.applicationContext
        mutable.update { it.copy(phase = UpdatePhase.Verifying, autoInstall = false, failure = null, cancelling = false) }
        transferJob = scope.launch {
            var failure = UpdateFailure.Verification
            try {
                val apk = UpdateInstaller.verify(app, release)
                if (!mutable.value.mayLaunchInstaller(canLaunch())) { mutable.update { it.copy(phase = UpdatePhase.Ready) }; return@launch }
                failure = UpdateFailure.Installer
                if (!UpdateInstaller.canInstall(app)) {
                    prefs(app).edit().putBoolean("permissionRequested", true).apply()
                    mutable.update { it.copy(phase = UpdatePhase.PermissionRequired, externalPending = true) }
                    launch(UpdateInstaller.permissionIntent(app), true)
                } else {
                    prefs(app).edit().putBoolean("permissionRequested", false).apply()
                    mutable.update { it.copy(phase = UpdatePhase.InstallerOpened, externalPending = true) }
                    launch(UpdateInstaller.installIntent(app, apk), false)
                }
            } catch (cancelled: CancellationException) {
                mutable.update { it.copy(phase = UpdatePhase.Ready, autoInstall = false, cancelling = false) }
                throw cancelled
            } catch (error: Exception) {
                Log.i("PepoMote/update", "Installer unavailable: ${error.message}")
                if (failure == UpdateFailure.Verification) runCatching { UpdateInstaller.file(app, release).delete() }
                mutable.update { it.copy(phase = UpdatePhase.Failed, failure = failure, autoInstall = false, externalPending = false) }
            }
        }
    }

    fun permissionReturned(context: Context) {
        if (!prefs(context).getBoolean("permissionRequested", false)) return
        prefs(context).edit().putBoolean("permissionRequested", false).apply()
        mutable.update { it.permissionResult(UpdateInstaller.canInstall(context)) }
    }
    fun installerReturned() { mutable.update { it.installerReturned() } }
}
