package dev.pepotech.pepomote.server.setup

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ProviderInfo
import android.net.Uri
import android.os.Build
import android.os.Process
import android.provider.DocumentsContract

/** Last completed setup, not a claim that an emulator/game is currently reading those settings. */
data class SetupStatus(
    val packageName: String?,
    val permissionUri: Uri?,
    val hasBackup: Boolean,
    val prepared: Boolean = false,
    val configuredPlayers: List<SetupPlayer> = emptyList(),
    val recoveryRequired: Boolean = false,
    val backupPackageName: String? = null,
)

/** Blocking IO; call prepare/restore on Dispatchers.IO. Both emulator processes must restart afterwards. */
object EmulatorSetup {
    private const val PREFS = "emulator_setup"
    private const val RW = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION

    @Synchronized
    fun prepare(
        context: Context,
        target: EmulatorTarget,
        treeUri: Uri,
        players: List<SetupPlayer> = listOf(SetupPlayer(0)),
        beforeWrite: (String) -> Unit = {},
    ): SetupResult = accessible {
        val app = context.applicationContext
        val packageName = validateProvider(app, target, treeUri)
        val backups = AndroidBackupStore(app, target)
        val prefs = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        if (backups.exists() && prefs.getString("${target.name}.package", null) != packageName) {
            throw SetupException("Restore the previous ${target.name} setup before preparing a different installed variant.", reason = SetupErrorReason.Conflict)
        }
        requirePersistedGrant(app, treeUri)
        val store = SafDocumentStore(app.contentResolver, target, treeUri, packageName)
        if (!prefs.edit().putString("${target.name}.tree", treeUri.toString()).putString("${target.name}.package", packageName).commit()) {
            throw SetupException("PepoMote could not save the selected permission for restoration. No emulator settings were changed.", reason = SetupErrorReason.Backup)
        }
        if (target == EmulatorTarget.Eden && store.read("config.ini") == null) {
            invalidatePrepared(app, target)
            throw SetupException("Open this Eden installation once to finish its initial setup, then return to PepoMote and tap Prepare again. Its permission has been saved, but config/config.ini does not exist yet.", reason = SetupErrorReason.Initialize)
        }
        // The app retains the restart step before the transaction can perform recovery or writes.
        beforeWrite(packageName)
        invalidatePrepared(app, target)
        val result = SetupTransaction(store, backups).prepare(target, players).result(target, packageName)
        val completedPlayers = players.sortedBy { it.slot }.joinToString(";") { "${it.slot}:${it.ownNunchuk}" }
        if (!prefs.edit().putString("${target.name}.prepared_players", completedPlayers).commit()) {
            throw SetupException("The settings and private backup were saved, but the completed setup could not be recorded. Restart the emulator and prepare it again.", reason = SetupErrorReason.Backup)
        }
        result
    }

    @Synchronized
    fun restore(context: Context, target: EmulatorTarget, beforeWrite: (String) -> Unit = {}): SetupResult = accessible {
        val app = context.applicationContext
        val backups = AndroidBackupStore(app, target)
        if (!backups.exists()) throw SetupException("No saved ${target.name} setup is available to restore.", reason = SetupErrorReason.NoBackup)
        val prefs = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val uri = permissionUri(app, target) ?: prefs.getString("${target.name}.tree", null)?.let(Uri::parse)
            ?: throw SetupException("The saved emulator folder permission is missing. Select the same emulator folder again.")
        val packageName = validateProvider(app, target, uri)
        if (packageName != prefs.getString("${target.name}.package", null)) throw SetupException("The saved emulator variant no longer matches its document provider.", reason = SetupErrorReason.Conflict)
        requirePersistedGrant(app, uri)
        val store = SafDocumentStore(app.contentResolver, target, uri, packageName)
        beforeWrite(packageName)
        invalidatePrepared(app, target)
        SetupTransaction(store, backups).restore().result(target, packageName)
    }

    fun hasBackup(context: Context, target: EmulatorTarget): Boolean = AndroidBackupStore(context.applicationContext, target).exists()

    /** Inspect grants and installed-provider metadata only: do not start the emulator process. */
    fun permissionUri(context: Context, target: EmulatorTarget): Uri? {
        val app = context.applicationContext
        val packageName = installedPackage(app, target) ?: return null
        val saved = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString("${target.name}.tree", null)
        val grants = runCatching {
            app.contentResolver.persistedUriPermissions.filter { it.isReadPermission && it.isWritePermission }.map { it.uri }
        }.getOrDefault(emptyList())
        return grants.sortedBy { it.toString() != saved }.firstOrNull { uri ->
            runCatching { validateProvider(app, target, uri) == packageName }.getOrDefault(false)
        }
    }

    /** Reads only PepoMote's private state; checking status must not undo a user's force-stop. */
    fun setupStatus(context: Context, target: EmulatorTarget): SetupStatus {
        val app = context.applicationContext
        val prefs = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val backups = AndroidBackupStore(app, target)
        val hasBackup = backups.exists()
        val journal = runCatching { backups.load() }
        val packageName = installedPackage(app, target)
        val savedPackage = prefs.getString("${target.name}.package", null)?.takeIf { it in packages(target) }
        val players = completedPlayers(prefs.getString("${target.name}.prepared_players", null))
        val prepared = packageName != null && packageName == savedPackage && players.isNotEmpty() &&
            journal.getOrNull()?.let { it.pending == null && it.files.isNotEmpty() } == true
        return SetupStatus(
            packageName = packageName,
            permissionUri = permissionUri(app, target),
            hasBackup = hasBackup,
            prepared = prepared,
            configuredPlayers = if (prepared) players else emptyList(),
            recoveryRequired = hasBackup && (journal.isFailure || journal.getOrNull()?.pending != null),
            backupPackageName = savedPackage.takeIf { hasBackup },
        )
    }

    fun initialUri(target: EmulatorTarget): Uri = initialUri(target, packages(target).first())

    fun initialUri(context: Context, target: EmulatorTarget): Uri? = installedPackage(context, target)?.let { initialUri(target, it) }

    fun installedPackage(context: Context, target: EmulatorTarget): String? {
        val saved = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString("${target.name}.package", null)
        val savedCandidate = listOfNotNull(saved).filter { it in packages(target) }
        // Existing backups belong to that exact variant; never silently move them to a new install.
        val candidates = if (hasBackup(context, target)) savedCandidate else (savedCandidate + packages(target)).distinct()
        return candidates.firstOrNull { packageName ->
            @Suppress("DEPRECATION")
            val provider = context.packageManager.resolveContentProvider("$packageName.user", 0)
            provider?.packageName == packageName && safeProvider(provider, "$packageName.user")
        }
    }

    fun packages(target: EmulatorTarget): List<String> = SetupSafety.packages(target)

    private fun initialUri(target: EmulatorTarget, packageName: String): Uri {
        require(packageName in packages(target))
        // API 29+ DocumentsUI opens provider roots directly. These emulator providers do not
        // implement findDocumentPath, which the document-URI navigation route requires.
        return if (Build.VERSION.SDK_INT >= 29) DocumentsContract.buildRootUri("$packageName.user", "root")
        else DocumentsContract.buildDocumentUri("$packageName.user", "root/")
    }

    private fun completedPlayers(value: String?): List<SetupPlayer> {
        if (value == null) return emptyList()
        val players = value.split(';').map { item ->
            val parts = item.split(':')
            if (parts.size != 2) return emptyList()
            val slot = parts[0].toIntOrNull()?.takeIf { it in 0..3 } ?: return emptyList()
            val ownNunchuk = parts[1].toBooleanStrictOrNull() ?: return emptyList()
            SetupPlayer(slot, ownNunchuk)
        }
        return players.takeIf { it.size in 1..4 && it.map { player -> player.slot }.distinct().size == it.size }.orEmpty()
    }

    private fun invalidatePrepared(context: Context, target: EmulatorTarget) {
        if (!context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().remove("${target.name}.prepared_players").commit()) {
            throw SetupException("PepoMote could not save the setup's pending state. No new emulator writes were allowed.", reason = SetupErrorReason.Backup)
        }
    }

    private fun validateProvider(context: Context, target: EmulatorTarget, uri: Uri): String {
        if (uri.scheme != "content" || !DocumentsContract.isTreeUri(uri) || uri.query != null || uri.fragment != null) {
            throw SetupException("Choose the emulator root or configuration folder with Android's folder picker.", reason = SetupErrorReason.Selection)
        }
        val authority = uri.authority ?: throw SetupException("The selected folder has no emulator provider.")
        if (uri != DocumentsContract.buildTreeDocumentUri(authority, DocumentsContract.getTreeDocumentId(uri))) {
            throw SetupException("Allow access to the emulator using Android's folder permission screen.", reason = SetupErrorReason.Selection)
        }
        @Suppress("DEPRECATION")
        val provider = context.packageManager.resolveContentProvider(authority, 0)
            ?: throw SetupException("The emulator document provider is unavailable. Install an official supported build and open it once.")
        val packageName = provider.packageName ?: throw SetupException("The selected folder has no verified emulator owner.")
        SetupSafety.validateRoot(target, authority, packageName, DocumentsContract.getTreeDocumentId(uri))
        if (!safeProvider(provider, authority)) {
            throw SetupException("The selected emulator provider does not support safe folder grants.")
        }
        return packageName
    }

    private fun safeProvider(provider: ProviderInfo, authority: String): Boolean =
        provider.enabled && provider.applicationInfo?.enabled != false && provider.exported && provider.grantUriPermissions &&
            provider.authority?.split(';')?.contains(authority) == true

    private fun requirePersistedGrant(context: Context, uri: Uri) {
        fun granted() = context.contentResolver.persistedUriPermissions.any { it.uri == uri && it.isReadPermission && it.isWritePermission }
        if (granted()) return
        if (context.checkUriPermission(uri, Process.myPid(), Process.myUid(), RW) != PackageManager.PERMISSION_GRANTED) {
            throw SetupException("Select the emulator folder again and allow both reading and writing.", reason = SetupErrorReason.Permission)
        }
        try {
            context.contentResolver.takePersistableUriPermission(uri, RW)
        } catch (error: SecurityException) {
            throw SetupException("This folder grant cannot be kept for restoration. Select the emulator folder through Android's system picker again.", error, SetupErrorReason.Permission)
        }
        if (!granted()) throw SetupException("Android did not retain read and write access to the emulator folder. Select it again before setup.", reason = SetupErrorReason.Permission)
    }

    private fun SetupExecution.result(target: EmulatorTarget, packageName: String) = SetupResult(target, packageName, changedFiles, profileNames, preservedUserChanges = preservedUserChanges)

    private inline fun <T> accessible(action: () -> T): T = try {
        action()
    } catch (error: SetupException) {
        throw error
    } catch (error: Exception) {
        throw SetupException("The emulator folder could not be accessed safely. Close the emulator and select its folder again; existing private backups have been kept.", error)
    }
}
