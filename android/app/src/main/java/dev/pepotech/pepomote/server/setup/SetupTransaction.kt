package dev.pepotech.pepomote.server.setup

internal class SetupTransaction(private val store: SetupDocumentStore, private val backups: SetupBackupStore) {
    private data class Recovery(val journal: SetupJournal?, val changedFiles: List<String> = emptyList(), val interrupted: Boolean = false)
    fun prepare(target: EmulatorTarget, players: List<SetupPlayer>): SetupExecution = protectErrors {
        if (players.isEmpty() || players.size > 4 || players.any { it.slot !in 0..3 } || players.map { it.slot }.distinct().size != players.size) {
            throw SetupException("Select one to four different controller slots (1–4).", reason = SetupErrorReason.Selection)
        }
        val recovered = recoverInterrupted(backups.load())
        val previous = recovered.journal?.files.orEmpty()
        val tracked = previous.associateByTo(linkedMapOf()) { it.path }
        val writes = mutableListOf<PendingWrite>()
        val names = mutableListOf<String>()

        fun configure(path: String, values: Map<IniKey, String>, profile: Boolean = false) {
            SetupSafety.validatePath(target, path)
            val before = read(path)
            val old = tracked[path]
            if (profile && before != null && (old?.profile != true || !equalBytes(before, old.applied))) {
                throw SetupException("$path changed while profiles were being prepared. It has been kept; try setup again to choose another profile name.", reason = SetupErrorReason.Conflict)
            }
            val originalDoc = IniDocument.parse(IniDocument.decode(before ?: byteArrayOf()))
            var doc = originalDoc
            var retained = old?.settings.orEmpty()
            if (!profile && old != null) {
                // Keep the first full-file backup, but never mistake a user's newer binding for ours.
                retained = retained.groupBy { it.key.section to it.group }.values.flatMap { group ->
                    if (group.any { it.key in values } && !SettingOwnership.matches(path, originalDoc, group)) {
                        group.map { it.copy(originalLine = originalDoc.rawLine(it.key)) }
                    } else group
                }
                // A smaller requested layout must not leave previously enabled extra players active.
                val retired = retained.groupBy { it.key.section to it.group }.values.filter { group -> group.none { it.key in values } }
                val reverted = retired.filter { group -> SettingOwnership.matches(path, doc, group) }.flatten()
                if (reverted.isNotEmpty()) {
                    doc = IniDocument.parse(doc.restoreLines(reverted.associate { it.key to it.originalLine }, originalSections(old)))
                    retained = retained.filter { it !in reverted }
                }
            }
            val settings = retained.associateByTo(linkedMapOf()) { it.key }
            for ((key, value) in values) {
                settings[key] = ManagedSetting(
                    key,
                    if (settings.containsKey(key)) settings.getValue(key).originalLine else doc.rawLine(key),
                    value,
                    key.name.removeSuffix("\\default")
                )
            }
            val after = doc.merge(values).toByteArray(Charsets.UTF_8)
            val record = ManagedFile(
                path = path,
                original = if (old == null) before else old.original,
                applied = after,
                settings = settings.values.toList(),
                profile = profile,
                canRestoreWhole = old == null || (old.canRestoreWhole && equalBytes(before, old.applied)) || (profile && before == null)
            )
            tracked[path] = record
            if (!equalBytes(before, after)) writes += PendingWrite(path, before, after)
        }

        when (target) {
            EmulatorTarget.Dolphin -> {
                val server = IniDocument.parse(IniDocument.decode(read("DSUClient.ini") ?: byteArrayOf()))
                configure("DSUClient.ini", linkedMapOf(
                    IniKey("Server", "Enabled") to "True",
                    IniKey("Server", "Entries") to ProfileBindings.dolphinServers(server.value(IniKey("Server", "Entries")))
                ))
                val active = linkedMapOf<IniKey, String>()
                for (player in players.sortedBy { it.slot }) {
                    val section = "Wiimote${player.slot + 1}"
                    active[IniKey(section, "Source")] = "1"
                    ProfileBindings.dolphin(player.slot, player.ownNunchuk).forEach { (key, value) -> active[IniKey(section, key)] = value }
                }
                configure("WiimoteNew.ini", active)
                for (slot in 0..3) for (nunchuk in listOf(false, true)) {
                    val baseName = "PepoMote Android J${slot + 1}" + if (nunchuk) " Nunchuk" else ""
                    var name: String? = null
                    for (number in 1..999) {
                        val candidate = baseName + if (number == 1) "" else " ($number)"
                        val path = "Profiles/Wiimote/$candidate.ini"
                        val current = read(path)
                        val owned = tracked[path]
                        if (current == null || (owned?.profile == true && equalBytes(current, owned.applied))) {
                            name = candidate
                            break
                        }
                    }
                    val selected = name ?: throw SetupException("Too many existing PepoMote profiles. Restore the previous setup before preparing new profiles.")
                    configure("Profiles/Wiimote/$selected.ini", ProfileBindings.dolphin(slot, nunchuk).mapKeys { IniKey("Profile", it.key) }, profile = true)
                    names += selected
                }
            }
            EmulatorTarget.Eden -> {
                val config = IniDocument.parse(IniDocument.decode(read("config.ini") ?: byteArrayOf()))
                configure("config.ini", ProfileBindings.eden(players, config.value(IniKey("Controls", "udp_input_servers"))).mapKeys { IniKey("Controls", it.key) })
            }
        }
        if (tracked.size > 64) throw SetupException("There are too many saved PepoMote profile versions. Restore the previous setup first.")
        applyTransaction(previous, tracked.values.toList(), writes, restoring = false)
        SetupExecution((recovered.changedFiles + writes.map { it.path }).distinct(), names)
    }

    fun restore(): SetupExecution = protectErrors {
        val recovered = recoverInterrupted(backups.load())
        val journal = recovered.journal ?: if (recovered.interrupted) {
            return@protectErrors SetupExecution(recovered.changedFiles)
        } else throw SetupException("No saved emulator setup is available to restore.", reason = SetupErrorReason.NoBackup)
        val writes = mutableListOf<PendingWrite>()
        val preserved = mutableListOf<String>()
        for (file in journal.files) {
            val current = read(file.path)
            if (current == null) {
                if (file.original != null) preserved += file.path
                continue
            }
            val restored = when {
                equalBytes(current, file.applied) && file.canRestoreWhole -> file.original
                file.profile -> {
                    preserved += file.path
                    current
                }
                else -> {
                    val ini = IniDocument.parse(IniDocument.decode(current))
                    val restore = linkedMapOf<IniKey, String?>()
                    for (group in file.settings.groupBy { it.key.section to it.group }.values) {
                        if (SettingOwnership.matches(file.path, ini, group)) {
                            group.forEach { restore[it.key] = it.originalLine }
                        } else {
                            preserved += "${file.path}: ${group.first().key.name.removeSuffix("\\default")}"
                        }
                    }
                    val result = ini.restoreLines(restore, originalSections(file))
                    if (file.original == null && result.removePrefix("\uFEFF").isBlank()) null else result.toByteArray(Charsets.UTF_8)
                }
            }
            if (!equalBytes(current, restored)) writes += PendingWrite(file.path, current, restored)
        }
        applyTransaction(journal.files, emptyList(), writes, restoring = true)
        SetupExecution((recovered.changedFiles + writes.map { it.path }).distinct(), preservedUserChanges = preserved.distinct())
    }

    private fun applyTransaction(previous: List<ManagedFile>, next: List<ManagedFile>, writes: List<PendingWrite>, restoring: Boolean) {
        // Persist every preimage and the pending transaction before touching the provider.
        val pending = SetupJournal(next, PendingTransaction(writes, previous))
        backups.save(pending)
        val attempted = mutableListOf<PendingWrite>()
        try {
            for (write in writes) {
                if (!equalBytes(read(write.path), write.before)) {
                    throw SetupException("${write.path} changed while setup was running. Close the emulator and try again.")
                }
                attempted += write
                putAndVerify(write.path, write.after)
            }
            if (restoring) backups.clear() else backups.save(SetupJournal(next))
        } catch (failure: Exception) {
            var rollbackFailed = false
            for (write in attempted.asReversed()) {
                try {
                    val current = read(write.path)
                    if (equalBytes(current, write.before)) continue
                    // A failed provider close can race a later emulator save. A post-failure read
                    // is not proof that we wrote those bytes; only the intended image is ours.
                    if (!equalBytes(current, write.after)) {
                        throw SetupException("${write.path} changed after our write; its newer contents were kept.", reason = SetupErrorReason.RecoveryRequired)
                    }
                    putAndVerify(write.path, write.before)
                } catch (rollback: Exception) {
                    failure.addSuppressed(rollback)
                    rollbackFailed = true
                }
            }
            if (!rollbackFailed) {
                try {
                    if (previous.isEmpty()) backups.clear() else backups.save(SetupJournal(previous))
                } catch (rollback: Exception) {
                    failure.addSuppressed(rollback)
                    rollbackFailed = true
                }
            }
            val message = if (rollbackFailed) {
                "Setup could not finish and its rollback could not be verified. The private backup is kept. Close the emulator and use Restore before trying setup again."
            } else {
                "Setup could not finish. All attempted file changes were rolled back. Close the emulator, check the folder permission and try again."
            }
            throw SetupException(message, failure, if (rollbackFailed) SetupErrorReason.RecoveryRequired else SetupErrorReason.RolledBack)
        }
    }

    private fun recoverInterrupted(journal: SetupJournal?): Recovery {
        val pending = journal?.pending ?: return Recovery(journal)
        try {
            // Inspect every candidate before starting recovery, then recheck each before writing.
            val affected = pending.writes.filter { write ->
                val current = read(write.path)
                when {
                    equalBytes(current, write.before) -> false
                    equalBytes(current, write.after) -> true
                    else -> throw SetupException("An interrupted setup left ${write.path} with unexpected contents. Its original private backup is kept; no later emulator changes were overwritten.", reason = SetupErrorReason.RecoveryRequired)
                }
            }
            val changed = mutableListOf<String>()
            for (write in affected.asReversed()) {
                val current = read(write.path)
                if (equalBytes(current, write.before)) continue
                if (!equalBytes(current, write.after)) throw SetupException("${write.path} changed during recovery; its newer contents and the backup have been kept.", reason = SetupErrorReason.RecoveryRequired)
                putAndVerify(write.path, write.before)
                changed += write.path
            }
            val previous = if (pending.previousFiles.isEmpty()) {
                backups.clear()
                null
            } else SetupJournal(pending.previousFiles).also(backups::save)
            return Recovery(previous, changed, interrupted = true)
        } catch (failure: Exception) {
            if (failure is SetupException && failure.reason == SetupErrorReason.RecoveryRequired) throw failure
            throw SetupException("The interrupted setup could not be recovered safely. Its private backup has been kept; close the emulator before retrying restoration.", failure, SetupErrorReason.RecoveryRequired)
        }
    }

    private fun putAndVerify(path: String, bytes: ByteArray?) {
        if (bytes == null) store.delete(path) else store.write(path, bytes)
        if (!equalBytes(read(path), bytes)) throw SetupException("The emulator did not retain the verified contents of $path.")
    }

    private fun read(path: String): ByteArray? = store.read(path)?.also {
        if (it.size > SetupSafety.MAX_FILE_BYTES) throw SetupException("$path is too large to edit safely.")
    }

    private fun originalSections(file: ManagedFile) = IniDocument.parse(IniDocument.decode(file.original ?: byteArrayOf())).sections

    private inline fun <T> protectErrors(action: () -> T): T = try {
        action()
    } catch (error: SetupException) {
        throw error
    } catch (error: Exception) {
        throw SetupException("Could not safely access the emulator settings. Open the emulator once, close it, then select its configuration folder again.", error)
    }

    private fun equalBytes(first: ByteArray?, second: ByteArray?) = if (first == null || second == null) first == null && second == null else first.contentEquals(second)
}
