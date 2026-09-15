package dev.pepotech.pepomote.server.setup

enum class EmulatorTarget { Dolphin, Eden }

data class SetupPlayer(val slot: Int, val ownNunchuk: Boolean = false)

data class SetupResult(
    val target: EmulatorTarget,
    val packageName: String,
    val changedFiles: List<String>,
    val profileNames: List<String>,
    val needsRestart: Boolean = true,
    val preservedUserChanges: List<String> = emptyList()
)

/** Stable UI categories; message retains detailed diagnostics without losing recovery warnings. */
enum class SetupErrorReason { Access, Selection, Permission, Initialize, Conflict, Backup, RolledBack, RecoveryRequired, NoBackup }

class SetupException(
    message: String,
    cause: Throwable? = null,
    val reason: SetupErrorReason = SetupErrorReason.Access
) : java.io.IOException(message, cause)

internal data class IniKey(val section: String, val name: String)

internal data class ManagedSetting(val key: IniKey, val originalLine: String?, val appliedValue: String, val group: String)

internal data class ManagedFile(
    val path: String,
    val original: ByteArray?,
    val applied: ByteArray,
    val settings: List<ManagedSetting>,
    val profile: Boolean = false,
    val canRestoreWhole: Boolean = true
)

internal data class PendingWrite(val path: String, val before: ByteArray?, val after: ByteArray?)
internal data class PendingTransaction(val writes: List<PendingWrite>, val previousFiles: List<ManagedFile>)
internal data class SetupJournal(val files: List<ManagedFile>, val pending: PendingTransaction? = null)

internal data class SetupExecution(
    val changedFiles: List<String> = emptyList(),
    val profileNames: List<String> = emptyList(),
    val preservedUserChanges: List<String> = emptyList()
)

internal interface SetupDocumentStore {
    /** Paths are relative to the validated emulator Config/config directory. */
    fun read(path: String): ByteArray?
    fun write(path: String, bytes: ByteArray)
    fun delete(path: String)
}

internal interface SetupBackupStore {
    fun load(): SetupJournal?
    fun save(journal: SetupJournal)
    fun clear()
}
