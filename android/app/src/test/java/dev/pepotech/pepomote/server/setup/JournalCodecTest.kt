package dev.pepotech.pepomote.server.setup

import org.junit.Assert.*
import org.junit.Test

class JournalCodecTest {
    @Test fun roundTripPreservesOriginalBytesSettingsAndPendingRollback() {
        val setting = ManagedSetting(IniKey("Controls", "player_0_type"), "  player_0_type = 1  ", "0", "player_0_type")
        val file = ManagedFile("config.ini", "\uFEFF[Controls]\r\nplayer_0_type=1\r\n".toByteArray(), "[Controls]\nplayer_0_type=0\n".toByteArray(), listOf(setting), canRestoreWhole = false)
        val journal = SetupJournal(listOf(file), PendingTransaction(listOf(PendingWrite("config.ini", file.original, file.applied)), listOf(file)))
        val decoded = JournalCodec.decode(JournalCodec.encode(journal))
        assertArrayEquals(file.original, decoded.files.single().original)
        assertArrayEquals(file.applied, decoded.files.single().applied)
        assertEquals(listOf(setting), decoded.files.single().settings)
        assertFalse(decoded.files.single().canRestoreWhole)
        assertArrayEquals(file.original, decoded.pending!!.writes.single().before)
        assertEquals("config.ini", decoded.pending.previousFiles.single().path)
    }

    @Test fun newlyCreatedFilesKeepNullPreimage() {
        val journal = SetupJournal(listOf(ManagedFile("config.ini", null, "[Controls]\n".toByteArray(), emptyList())))
        assertNull(JournalCodec.decode(JournalCodec.encode(journal)).files.single().original)
    }

    @Test fun truncatedOrChangedBackupIsRejected() {
        val encoded = JournalCodec.encode(SetupJournal(emptyList()))
        assertThrows(SetupException::class.java) { JournalCodec.decode(encoded.copyOf(encoded.size - 1)) }
        val modified = encoded.copyOf()
        modified[0] = (modified[0].toInt() xor 1).toByte()
        assertThrows(SetupException::class.java) { JournalCodec.decode(modified) }
    }
}
