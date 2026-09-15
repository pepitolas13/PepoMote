package dev.pepotech.pepomote.server.setup

import android.content.Context
import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption

internal class AndroidBackupStore(context: Context, target: EmulatorTarget) : SetupBackupStore {
    private val directory = File(context.noBackupFilesDir, "emulator-setup")
    private val file = File(directory, "${target.name.lowercase()}.backup")

    fun exists() = file.exists()

    override fun load(): SetupJournal? {
        if (!exists()) return null
        return JournalCodec.decode(readBytes())
    }

    private fun readBytes(): ByteArray = file.inputStream().use { input ->
            val output = java.io.ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            while (true) {
                val count = input.read(buffer)
                if (count == -1) break
                if (output.size() + count > SetupSafety.MAX_BACKUP_BYTES) throw SetupException("The private setup backup is too large.", reason = SetupErrorReason.Backup)
                output.write(buffer, 0, count)
            }
            output.toByteArray()
    }

    override fun save(journal: SetupJournal) {
        val bytes = JournalCodec.encode(journal)
        if (!directory.isDirectory && !directory.mkdirs()) throw SetupException("PepoMote cannot create its private backup folder. Free some device storage and try again.", reason = SetupErrorReason.Backup)
        val temporary = File(directory, file.name + ".new")
        try {
            FileOutputStream(temporary).use { output ->
                output.write(bytes)
                output.fd.sync()
            }
            // API 26+: same-directory atomic replacement, with errors propagated to the transaction.
            // Unlike File.renameTo, this also has explicit replacement semantics on desktop JVM tests.
            Files.move(temporary.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        } catch (error: Exception) {
            throw SetupException("PepoMote could not save its private backup. No new emulator changes are allowed without a backup.", error, SetupErrorReason.Backup)
        } finally {
            Files.deleteIfExists(temporary.toPath())
        }
        val verified = readBytes()
        if (!bytes.contentEquals(verified)) throw SetupException("The private setup backup could not be verified.", reason = SetupErrorReason.Backup)
    }

    override fun clear() {
        Files.deleteIfExists(file.toPath())
        if (exists()) throw SetupException("The restored setup backup could not be removed.", reason = SetupErrorReason.Backup)
    }
}
