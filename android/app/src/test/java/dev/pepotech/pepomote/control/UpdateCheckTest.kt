package dev.pepotech.pepomote.control

import dev.pepotech.pepomote.control.UpdateCheck.Version
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Mismos vectores que desktop/src/update.rs e iOS (UpdateCheckTests). */
class UpdateCheckTest {
    @Test
    fun parseaEtiquetasConYSinV() {
        assertEquals(Version(1, 6, 0), UpdateCheck.parse("v1.6.0"))
        assertEquals(Version(1, 6, 0), UpdateCheck.parse("1.6.0"))
        assertEquals(Version(1, 10, 2), UpdateCheck.parse(" V1.10.2 "))
        assertEquals(Version(2, 0, 0), UpdateCheck.parse("v2.0"))
        for (bad in listOf("v1.6.0-beta", "abc", "", "v", "1", "1.2.3.4", "1..2", "1.x.0", null)) {
            assertNull(bad, UpdateCheck.parse(bad))
        }
    }

    @Test
    fun comparaNumericamenteNoPorTexto() {
        assertTrue(Version(1, 10, 0) > Version(1, 9, 9))
        assertTrue(Version(2, 0, 0) > Version(1, 99, 99))
        assertTrue(Version(1, 6, 1) > Version(1, 6, 0))
        assertEquals(0, Version(1, 6, 0).compareTo(Version(1, 6, 0)))
        assertEquals("1.6.0", Version(1, 6, 0).toString())
    }

    @Test
    fun laLocationDeGitHubDaLaVersion() {
        val loc = "https://github.com/pepitolas13/PepoMote/releases/tag/v1.5.0"
        assertEquals(Version(1, 5, 0), UpdateCheck.versionFromLocation(loc))
        assertEquals(Version(1, 5, 0), UpdateCheck.versionFromLocation("$loc?x=1"))
        assertEquals(Version(1, 5, 0), UpdateCheck.versionFromLocation("$loc/"))
        assertNull(UpdateCheck.versionFromLocation("https://github.com/pepitolas13/PepoMote/releases"))
        assertNull(UpdateCheck.versionFromLocation(""))
        assertNull(UpdateCheck.versionFromLocation(null))
    }

    @Test
    fun pendingSoloSiEsMayorYNoDescartada() {
        val cur = Version(1, 5, 0)
        assertNull(UpdateCheck.pending(cur, null, null))
        assertNull(UpdateCheck.pending(null, Version(1, 6, 0), null))
        assertNull(UpdateCheck.pending(cur, Version(1, 5, 0), null))
        assertNull(UpdateCheck.pending(cur, Version(1, 4, 9), null))
        assertEquals(Version(1, 6, 0), UpdateCheck.pending(cur, Version(1, 6, 0), null))
        assertNull(UpdateCheck.pending(cur, Version(1, 6, 0), Version(1, 6, 0)))
        // se ocultó la 1.6.0, pero la 1.7.0 es otra: se anuncia
        assertEquals(Version(1, 7, 0), UpdateCheck.pending(cur, Version(1, 7, 0), Version(1, 6, 0)))
    }

    @Test
    fun dueRespetaActivadoY24h() {
        val day = 24L * 3600 * 1000
        assertTrue(UpdateCheck.due(true, 0, 1))
        assertFalse(UpdateCheck.due(false, 0, 1_000_000_000_000L))
        assertFalse(UpdateCheck.due(true, 1000, 1000 + day - 1))
        assertTrue(UpdateCheck.due(true, 1000, 1000 + day))
        // reloj hacia atrás: no dispara
        assertFalse(UpdateCheck.due(true, 5000, 4000))
    }

    @Test
    fun laUrlDeLaRelease() {
        assertEquals(
            "https://github.com/pepitolas13/PepoMote/releases/tag/v1.6.0",
            UpdateCheck.releaseUrl(Version(1, 6, 0))
        )
    }
}
