package dev.pepotech.pepomote.server.setup

import org.junit.Assert.*
import org.junit.Test

class SetupTransactionTest {
    @Test fun recoveringAnInterruptedFirstPreparationReturnsSuccessInsteadOfNoBackup() {
        val original = "[Controls]\nplayer_0_type=1\n"
        val store = MemoryDocuments(mapOf("config.ini" to original))
        val backups = MemoryBackups()
        val transaction = SetupTransaction(store, backups)
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0)))
        val prepared = backups.journal!!.files.single()
        backups.journal = SetupJournal(listOf(prepared), PendingTransaction(listOf(PendingWrite("config.ini", prepared.original, prepared.applied)), emptyList()))
        val restored = transaction.restore()
        assertEquals(listOf("config.ini"), restored.changedFiles)
        assertEquals(original, store.text("config.ini"))
        assertNull(backups.journal)
    }

    @Test fun unknownPartialWriteKeepsItsOriginalBackupAndReportsRecoveryRequired() {
        val store = MemoryDocuments(mapOf("config.ini" to "[Controls]\nplayer_0_type=1\n"))
        val backups = MemoryBackups()
        store.failWriteNumber = 1
        store.failureImage = "partial".toByteArray()
        val failure = assertThrows(SetupException::class.java) { SetupTransaction(store, backups).prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0))) }
        assertEquals(SetupErrorReason.RecoveryRequired, failure.reason)
        assertEquals("partial", store.text("config.ini"))
        assertNotNull(backups.journal)
    }

    @Test fun rollbackPreservesAConcurrentEmulatorEditAfterAnEarlierWriteWasVerified() {
        val store = MemoryDocuments(mapOf("DSUClient.ini" to "[Server]\nEnabled=False\n", "WiimoteNew.ini" to "[Wiimote1]\nSource=0\n"))
        val backups = MemoryBackups()
        store.readObserver = { path ->
            if (path == "WiimoteNew.ini" && store.writeCount == 1) {
                store.readObserver = null
                store.put("DSUClient.ini", store.text("DSUClient.ini")!! + "[User]\nnote=keep this newer edit\n")
                store.put("WiimoteNew.ini", "[Wiimote1]\nSource=2\n")
            }
        }
        val failure = assertThrows(SetupException::class.java) { SetupTransaction(store, backups).prepare(EmulatorTarget.Dolphin, listOf(SetupPlayer(0))) }
        assertEquals(SetupErrorReason.RecoveryRequired, failure.reason)
        assertTrue(store.text("DSUClient.ini")!!.contains("note=keep this newer edit"))
        assertEquals("[Wiimote1]\nSource=2\n", store.text("WiimoteNew.ini"))
        assertNotNull(backups.journal)
    }

    @Test fun edenCanonicalSaveDoesNotPreventRestoringItsBindingsAndDefaults() {
        val original = "[Controls]\nplayer_0_type=1\nplayer_0_type\\default=false\nplayer_0_button_a=engine:android,button:9\nplayer_0_button_a\\default=false\nmotion_enabled=false\nmotion_enabled\\default=false\n"
        val store = MemoryDocuments(mapOf("config.ini" to original))
        val transaction = SetupTransaction(store, MemoryBackups())
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0)))
        store.put("config.ini", officialEdenSave(store.text("config.ini")!!))
        val result = transaction.restore()
        val ini = IniDocument.parse(store.text("config.ini")!!)
        assertEquals("engine:android,button:9", ini.value(IniKey("Controls", "player_0_button_a")))
        assertEquals("1", ini.value(IniKey("Controls", "player_0_type")))
        assertEquals("false", ini.value(IniKey("Controls", "motion_enabled")))
        assertNull(ini.value(IniKey("Controls", "player_0_lstick")))
        assertTrue(result.preservedUserChanges.isEmpty())
    }

    @Test fun edenCanonicalSaveDoesNotKeepAnUnrequestedExtraPlayerConnected() {
        val store = MemoryDocuments(mapOf("config.ini" to "[Controls]\nplayer_1_type=1\nplayer_1_type\\default=false\nplayer_1_connected=false\nplayer_1_connected\\default=false\n"))
        val transaction = SetupTransaction(store, MemoryBackups())
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0), SetupPlayer(1)))
        store.put("config.ini", officialEdenSave(store.text("config.ini")!!))
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0)))
        val ini = IniDocument.parse(store.text("config.ini")!!)
        assertEquals("false", ini.value(IniKey("Controls", "player_1_connected")))
        assertEquals("1", ini.value(IniKey("Controls", "player_1_type")))
        assertNull(ini.value(IniKey("Controls", "player_1_button_a")))
    }

    @Test fun explicitReprepareRetainsTheUsersNewerManagedBindingAsItsRestorePreimage() {
        val store = MemoryDocuments(mapOf("config.ini" to "[Controls]\nplayer_0_button_a=original\nplayer_0_button_a\\default=false\n"))
        val backups = MemoryBackups()
        val transaction = SetupTransaction(store, backups)
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0)))
        store.put("config.ini", IniDocument.parse(store.text("config.ini")!!).merge(mapOf(IniKey("Controls", "player_0_button_a") to "engine:android,button:42")))
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0)))
        assertTrue(String(backups.journal!!.files.single().original!!).contains("player_0_button_a=original"))
        transaction.restore()
        assertEquals("engine:android,button:42", IniDocument.parse(store.text("config.ini")!!).value(IniKey("Controls", "player_0_button_a")))
    }

    @Test fun aProfileEditedBetweenNameSelectionAndPlanningIsNeverOverwritten() {
        val store = MemoryDocuments(emptyMap())
        val transaction = SetupTransaction(store, MemoryBackups())
        transaction.prepare(EmulatorTarget.Dolphin, listOf(SetupPlayer(0)))
        val path = "Profiles/Wiimote/PepoMote Android J1.ini"
        val userEdit = store.text(path)!! + "; user changed this profile\n"
        var reads = 0
        store.readObserver = { reading -> if (reading == path && ++reads == 2) store.put(path, userEdit) }
        assertThrows(SetupException::class.java) { transaction.prepare(EmulatorTarget.Dolphin, listOf(SetupPlayer(0))) }
        assertEquals(userEdit, store.text(path))
    }

    @Test fun smallerLayoutRestoresOnlyTheRemovedPlayersOwnedSettings() {
        val store = MemoryDocuments(mapOf("WiimoteNew.ini" to "[Wiimote2]\nSource = 2\nDevice = Real device\n"))
        val transaction = SetupTransaction(store, MemoryBackups())
        transaction.prepare(EmulatorTarget.Dolphin, listOf(SetupPlayer(0), SetupPlayer(1)))
        transaction.prepare(EmulatorTarget.Dolphin, listOf(SetupPlayer(0)))
        val ini = IniDocument.parse(store.text("WiimoteNew.ini")!!)
        assertEquals("1", ini.value(IniKey("Wiimote1", "Source")))
        assertEquals("2", ini.value(IniKey("Wiimote2", "Source")))
        assertEquals("Real device", ini.value(IniKey("Wiimote2", "Device")))
    }

    @Test fun failedRestoreRestoresThePreparedStateAndKeepsItsBackup() {
        val store = MemoryDocuments(mapOf("config.ini" to "[Controls]\nplayer_0_type=1\n"))
        val backups = MemoryBackups()
        val transaction = SetupTransaction(store, backups)
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0)))
        val prepared = store.texts()
        store.failWriteNumber = store.writeCount + 1
        assertThrows(SetupException::class.java) { transaction.restore() }
        assertEquals(prepared, store.texts())
        assertNotNull(backups.journal)
        transaction.restore()
        assertEquals("[Controls]\nplayer_0_type=1\n", store.text("config.ini"))
    }

    @Test fun oversizedInputFailsBeforeAnyWritesOrBackupReplacement() {
        val store = MemoryDocuments(mapOf("config.ini" to "x".repeat(1_048_577)))
        val backups = MemoryBackups()
        assertThrows(SetupException::class.java) { SetupTransaction(store, backups).prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0))) }
        assertEquals(0, store.writeCount)
        assertNull(backups.journal)
    }

    @Test fun dolphinOnlyActivatesRequestedPlayerAndNeverOverwritesForeignProfile() {
        val store = MemoryDocuments(mapOf(
            "DSUClient.ini" to "[Server]\nEnabled = False\nEntries = Other:10.0.0.4:26760;\n",
            "WiimoteNew.ini" to "[Wiimote1]\nSource = 0\n[Wiimote2]\nSource = 2\nDevice = RealDevice\n",
            "Profiles/Wiimote/PepoMote Android J1.ini" to "[Profile]\nDevice = My custom controller\n"
        ))
        val result = SetupTransaction(store, MemoryBackups()).prepare(EmulatorTarget.Dolphin, listOf(SetupPlayer(0)))
        val active = IniDocument.parse(store.text("WiimoteNew.ini")!!)
        assertEquals("1", active.value(IniKey("Wiimote1", "Source")))
        assertEquals("2", active.value(IniKey("Wiimote2", "Source")))
        assertEquals("RealDevice", active.value(IniKey("Wiimote2", "Device")))
        assertEquals("[Profile]\nDevice = My custom controller\n", store.text("Profiles/Wiimote/PepoMote Android J1.ini"))
        assertTrue(result.profileNames.contains("PepoMote Android J1 (2)"))
        assertEquals(8, result.profileNames.size)
    }

    @Test fun failedWriteRollsBackEveryAttemptedFileAndRetainsOriginalBytes() {
        val original = mapOf("DSUClient.ini" to "[Server]\r\nEnabled = False\r\n", "WiimoteNew.ini" to "[Wiimote1]\r\nSource = 0\r\n")
        val store = MemoryDocuments(original)
        val backups = MemoryBackups()
        store.failWriteNumber = 2
        assertThrows(SetupException::class.java) { SetupTransaction(store, backups).prepare(EmulatorTarget.Dolphin, listOf(SetupPlayer(0))) }
        assertEquals(original, store.texts())
        assertNull(backups.journal)
    }

    @Test fun unknownReadbackContentsAreKeptWithBackupInsteadOfAssumedToBeOurs() {
        val store = MemoryDocuments(mapOf("config.ini" to "[Controls]\nmotion_enabled=false\n"))
        val backups = MemoryBackups()
        store.corruptWriteNumber = 1
        val failure = assertThrows(SetupException::class.java) { SetupTransaction(store, backups).prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0))) }
        assertEquals(SetupErrorReason.RecoveryRequired, failure.reason)
        assertEquals("truncated", store.text("config.ini"))
        assertNotNull(backups.journal)
    }

    @Test fun backupFailureHappensBeforeAnyEmulatorWrite() {
        val store = MemoryDocuments(emptyMap())
        val backups = MemoryBackups().also { it.failSave = true }
        assertThrows(SetupException::class.java) { SetupTransaction(store, backups).prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0))) }
        assertEquals(0, store.writeCount)
    }

    @Test fun repeatedPrepareDoesNotReplaceOriginalBackupAndRestoresExactly() {
        val original = "\uFEFF; user\r\n[Controls]\r\nplayer_0_type = 1\r\nplayer_0_type\\default = false\r\n[Renderer]\r\nbackend = 1\r\n"
        val store = MemoryDocuments(mapOf("config.ini" to original))
        val backups = MemoryBackups()
        val transaction = SetupTransaction(store, backups)
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0)))
        val firstWrites = store.writeCount
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0)))
        assertEquals(firstWrites, store.writeCount)
        assertEquals(original, String(backups.journal!!.files.single().original!!, Charsets.UTF_8))
        transaction.restore()
        assertEquals(original, store.text("config.ini"))
        assertNull(backups.journal)
    }

    @Test fun restoreKeepsLaterUserBindingAndItsDefaultMarkerTogether() {
        val store = MemoryDocuments(mapOf("config.ini" to "[Controls]\nplayer_0_button_a=old\nplayer_0_button_a\\default=true\n[Renderer]\nbackend=1\n"))
        val backups = MemoryBackups()
        val transaction = SetupTransaction(store, backups)
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0)))
        val changed = IniDocument.parse(store.text("config.ini")!!).merge(linkedMapOf(
            IniKey("Controls", "player_0_button_a") to "engine:android,button:42",
            IniKey("Renderer", "backend") to "2"
        ))
        store.put("config.ini", changed)
        val result = transaction.restore()
        val restored = IniDocument.parse(store.text("config.ini")!!)
        assertEquals("engine:android,button:42", restored.value(IniKey("Controls", "player_0_button_a")))
        assertEquals("false", restored.value(IniKey("Controls", "player_0_button_a\\default")))
        assertEquals("2", restored.value(IniKey("Renderer", "backend")))
        assertNull(restored.value(IniKey("Controls", "enable_udp_controller")))
        assertTrue(result.preservedUserChanges.isNotEmpty())
    }

    @Test fun restoreLeavesProfileEditedByUserAndRemovesOnlyUnchangedGeneratedProfiles() {
        val store = MemoryDocuments(emptyMap())
        val transaction = SetupTransaction(store, MemoryBackups())
        transaction.prepare(EmulatorTarget.Dolphin, listOf(SetupPlayer(0)))
        val path = "Profiles/Wiimote/PepoMote Android J1.ini"
        store.put(path, store.text(path)!! + "; My edited profile\n")
        transaction.restore()
        assertTrue(store.text(path)!!.endsWith("; My edited profile\n"))
        assertEquals(setOf(path), store.texts().keys)
    }

    @Test fun repeatedPreparePreservesUnrelatedChangesMadeBetweenRuns() {
        val store = MemoryDocuments(mapOf("config.ini" to "[Controls]\nplayer_0_type=1\n[Renderer]\nbackend=1\n"))
        val transaction = SetupTransaction(store, MemoryBackups())
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0)))
        store.put("config.ini", store.text("config.ini")!!.replace("backend=1", "backend=2"))
        transaction.prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0), SetupPlayer(1)))
        transaction.restore()
        val restored = IniDocument.parse(store.text("config.ini")!!)
        assertEquals("2", restored.value(IniKey("Renderer", "backend")))
        assertEquals("1", restored.value(IniKey("Controls", "player_0_type")))
        assertNull(restored.value(IniKey("Controls", "player_1_connected")))
    }

    @Test fun rollbackFailureRetainsDurableBackupForRecovery() {
        val store = MemoryDocuments(mapOf("config.ini" to "[Controls]\nplayer_0_type=1\n"))
        val backups = MemoryBackups()
        store.failAllWrites = true
        assertThrows(SetupException::class.java) { SetupTransaction(store, backups).prepare(EmulatorTarget.Eden, listOf(SetupPlayer(0))) }
        assertNotNull(backups.journal)
    }

    @Test fun invalidAndDuplicateSlotsFailBeforeMutation() {
        for (players in listOf(emptyList(), listOf(SetupPlayer(4)), listOf(SetupPlayer(0), SetupPlayer(0)))) {
            val store = MemoryDocuments(emptyMap())
            assertThrows(SetupException::class.java) { SetupTransaction(store, MemoryBackups()).prepare(EmulatorTarget.Eden, players) }
            assertTrue(store.texts().isEmpty())
        }
    }
}

/** Mirrors only Eden's audited save transformations: quote comma strings and recompute defaults. */
private fun officialEdenSave(text: String): String {
    val ini = IniDocument.parse(text)
    return text.lines().joinToString("\n") { line ->
        if ('=' !in line) line else {
            val key = line.substringBefore('=').trim()
            val value = line.substringAfter('=').trim()
            if (key.endsWith("\\default")) {
                val base = key.removeSuffix("\\default")
                val default = when {
                    base.endsWith("_type") -> "0"
                    base.endsWith("_connected") -> if (base == "player_0_connected") "true" else "false"
                    base == "motion_enabled" -> "true"
                    base == "enable_udp_controller" -> "false"
                    base == "udp_input_servers" -> "127.0.0.1:26760"
                    else -> ""
                }
                "$key=${ini.value(IniKey("Controls", base))?.removeSurrounding("\"") == default}"
            } else "$key=" + if (',' in value) "\"${value.removeSurrounding("\"")}\"" else value
        }
    }
}

internal class MemoryDocuments(initial: Map<String, String>) : SetupDocumentStore {
    private val files = initial.mapValues { it.value.toByteArray() }.toMutableMap()
    var writeCount = 0
    var failWriteNumber = -1
    var corruptWriteNumber = -1
    var failAllWrites = false
    var failureImage: ByteArray? = null
    var readObserver: ((String) -> Unit)? = null
    override fun read(path: String): ByteArray? { readObserver?.invoke(path); return files[path]?.copyOf() }
    override fun write(path: String, bytes: ByteArray) {
        writeCount++
        files[path] = if (writeCount == corruptWriteNumber) "truncated".toByteArray() else bytes.copyOf()
        if (writeCount == failWriteNumber || failAllWrites) {
            failureImage?.let { files[path] = it.copyOf() }
            throw java.io.IOException("Simulated provider write failure")
        }
    }
    override fun delete(path: String) { files.remove(path) }
    fun put(path: String, value: String) { files[path] = value.toByteArray() }
    fun text(path: String) = files[path]?.toString(Charsets.UTF_8)
    fun texts() = files.mapValues { it.value.toString(Charsets.UTF_8) }
}

internal class MemoryBackups : SetupBackupStore {
    var journal: SetupJournal? = null
    var failSave = false
    override fun load() = journal
    override fun save(journal: SetupJournal) {
        if (failSave) throw java.io.IOException("No private storage")
        this.journal = journal
    }
    override fun clear() { journal = null }
}
