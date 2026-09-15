package dev.pepotech.pepomote.server

import android.content.Context
import android.net.Uri
import dev.pepotech.pepomote.server.setup.EmulatorSetup
import dev.pepotech.pepomote.server.setup.EmulatorTarget
import dev.pepotech.pepomote.server.setup.SetupErrorReason
import dev.pepotech.pepomote.server.setup.SetupException
import dev.pepotech.pepomote.server.setup.SetupPlayer
import dev.pepotech.pepomote.server.setup.SetupResult
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

internal data class SetupJobState(
    val busy: Boolean = false,
    val version: Long = 0,
    val result: SetupResult? = null,
    val error: Exception? = null,
    val restart: Map<EmulatorTarget, String> = emptyMap(),
    val target: EmulatorTarget? = null,
)

/** A folder transaction belongs to the app, not to the composable that opened the picker. */
internal object ServerSetupJobs {
    private const val PREFS = "server_setup_pending"
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutable = MutableStateFlow(SetupJobState())
    val flow = mutable.asStateFlow()

    fun refresh(context: Context) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val pending = EmulatorTarget.entries.mapNotNull { target ->
            prefs.getString(target.name, null)?.takeIf { it in EmulatorSetup.packages(target) }?.let { target to it }
        }.toMap()
        mutable.update { it.copy(restart = pending) }
    }

    fun prepare(context: Context, target: EmulatorTarget, uri: Uri, players: List<SetupPlayer>) {
        run(context, target, null) { app ->
            EmulatorSetup.prepare(app, target, uri, players) { packageName -> pending(app, target, packageName) }
        }
    }

    /** False means the guide should request the initial Android permission; existing grants need no picker. */
    @Synchronized
    fun prepare(context: Context, target: EmulatorTarget, players: List<SetupPlayer>): Boolean {
        if (mutable.value.busy) return true
        val uri = EmulatorSetup.permissionUri(context, target) ?: return false
        prepare(context, target, uri, players)
        return true
    }

    fun restore(context: Context, target: EmulatorTarget) =
        run(context, target, null) { app ->
            EmulatorSetup.restore(app, target) { packageName -> pending(app, target, packageName) }
        }

    @Synchronized
    internal fun run(context: Context, target: EmulatorTarget, candidate: String?, action: (Context) -> SetupResult) {
        if (mutable.value.busy) return
        val app = context.applicationContext
        mutable.update { it.copy(busy = true, result = null, error = null, target = target) }
        scope.launch {
            try {
                // Before any possible external write, retain the restart step across process death.
                if (candidate != null) pending(app, target, candidate)
                val result = action(app)
                if (result.needsRestart) pending(app, target, result.packageName)
                mutable.update { it.copy(busy = false, version = it.version + 1, result = result) }
            } catch (error: Exception) {
                mutable.update { it.copy(busy = false, version = it.version + 1, error = error) }
            }
        }
    }

    private fun pending(context: Context, target: EmulatorTarget, packageName: String) {
        if (!context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(target.name, packageName).commit()) {
            throw SetupException("PepoMote could not retain the emulator restart step. No new emulator writes were allowed.", reason = SetupErrorReason.Backup)
        }
        mutable.update { it.copy(restart = it.restart + (target to packageName)) }
    }

    fun restartHandled(context: Context, packageName: String) {
        val targets = mutable.value.restart.filterValues { it == packageName }.keys
        val editor = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
        targets.forEach { editor.remove(it.name) }
        editor.apply()
        mutable.update { it.copy(restart = it.restart - targets) }
    }

    fun dismissError() { mutable.update { it.copy(error = null) } }
}
