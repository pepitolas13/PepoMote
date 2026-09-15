package dev.pepotech.pepomote.server.setup

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.security.MessageDigest

/** Bounded, checksummed local journal; never accepts Java object deserialization. */
internal object JournalCodec {
    private const val MAGIC = 0x504D5331

    fun encode(journal: SetupJournal): ByteArray {
        val bytes = ByteArrayOutputStream()
        DataOutputStream(bytes).use { output ->
            output.writeInt(MAGIC)
            output.writeInt(1)
            output.files(journal.files)
            output.writeBoolean(journal.pending != null)
            journal.pending?.let { pending ->
                requireCount(pending.writes.size, 64)
                output.writeInt(pending.writes.size)
                pending.writes.forEach { write ->
                    output.writeUTF(write.path)
                    output.blob(write.before)
                    output.blob(write.after)
                }
                output.files(pending.previousFiles)
            }
        }
        val body = bytes.toByteArray()
        if (body.size + 32 > SetupSafety.MAX_BACKUP_BYTES) throw SetupException("The configuration backup is too large. No emulator settings were changed.")
        return body + sha256(body)
    }

    fun decode(bytes: ByteArray): SetupJournal = try {
        if (bytes.size !in 45..SetupSafety.MAX_BACKUP_BYTES) throw SetupException("The private setup backup is truncated or too large.")
        val body = bytes.copyOf(bytes.size - 32)
        if (!MessageDigest.isEqual(sha256(body), bytes.copyOfRange(bytes.size - 32, bytes.size))) {
            throw SetupException("The private setup backup failed its integrity check; emulator files were not changed.")
        }
        DataInputStream(ByteArrayInputStream(body)).use { input ->
            if (input.readInt() != MAGIC || input.readInt() != 1) throw SetupException("Unsupported emulator setup backup version.")
            val files = input.files()
            val pending = if (input.readBoolean()) {
                val count = input.readInt().also { requireCount(it, 64) }
                val writes = List(count) { PendingWrite(input.path(), input.blob(), input.blob()) }
                PendingTransaction(writes, input.files())
            } else null
            if (input.available() != 0) throw SetupException("The emulator setup backup has unexpected trailing data.")
            SetupJournal(files, pending)
        }
    } catch (error: SetupException) {
        throw SetupException(error.message ?: "The private setup backup is invalid.", error, SetupErrorReason.Backup)
    } catch (error: Exception) {
        throw SetupException("The private setup backup could not be read; emulator files were not changed.", error, SetupErrorReason.Backup)
    }

    private fun DataOutputStream.files(files: List<ManagedFile>) {
        requireCount(files.size, 64)
        writeInt(files.size)
        for (file in files) {
            writeUTF(file.path)
            blob(file.original)
            blob(file.applied)
            writeBoolean(file.profile)
            writeBoolean(file.canRestoreWhole)
            requireCount(file.settings.size, 1_024)
            writeInt(file.settings.size)
            for (setting in file.settings) {
                writeUTF(setting.key.section)
                writeUTF(setting.key.name)
                writeBoolean(setting.originalLine != null)
                setting.originalLine?.let(::writeUTF)
                writeUTF(setting.appliedValue)
                writeUTF(setting.group)
            }
        }
    }

    private fun DataInputStream.files(): List<ManagedFile> {
        val count = readInt().also { requireCount(it, 64) }
        val files = List(count) {
            val path = path()
            val original = blob()
            val applied = blob() ?: throw SetupException("Invalid applied bytes in setup backup.")
            val profile = readBoolean()
            val whole = readBoolean()
            val settingCount = readInt().also { requireCount(it, 1_024) }
            val settings = List(settingCount) {
                val key = IniKey(readUTF(), readUTF())
                val originalLine = if (readBoolean()) readUTF() else null
                ManagedSetting(key, originalLine, readUTF(), readUTF())
            }
            if (settings.map { it.key }.distinct().size != settings.size) throw SetupException("Duplicate settings in setup backup.")
            ManagedFile(path, original, applied, settings, profile, whole)
        }
        if (files.map { it.path }.distinct().size != files.size) throw SetupException("Duplicate files in setup backup.")
        return files
    }

    private fun DataOutputStream.blob(bytes: ByteArray?) {
        if (bytes == null) {
            writeInt(-1)
        } else {
            requireCount(bytes.size, SetupSafety.MAX_FILE_BYTES)
            writeInt(bytes.size)
            write(bytes)
        }
    }

    private fun DataInputStream.blob(): ByteArray? {
        val size = readInt()
        if (size == -1) return null
        requireCount(size, SetupSafety.MAX_FILE_BYTES)
        if (size > available()) throw SetupException("Truncated configuration bytes in setup backup.")
        return ByteArray(size).also(::readFully)
    }

    private fun DataInputStream.path(): String = readUTF().also { path ->
        if (path.length > 160 || path.startsWith('/') || '\\' in path || path.split('/').any { it.isEmpty() || it == "." || it == ".." }) {
            throw SetupException("Unsafe path in setup backup.")
        }
    }

    private fun requireCount(count: Int, maximum: Int) {
        if (count !in 0..maximum) throw SetupException("The emulator setup backup exceeds its safety limits.")
    }

    private fun sha256(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes)
}
