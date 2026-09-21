package dev.pepotech.pepomote.control

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * La máquina de la vibración (PROTOCOL.md §4.5), con los mismos casos que
 * `RumbleTrackTests` de iOS y el test de Rust: si esto diverge, un móvil
 * vibra distinto de otro con el mismo receptor.
 */
class RumbleTrackTest {
    /** Nivel de un Start/Change (null si es Stop o no hay orden). */
    private fun level(c: RumbleCommand?): Float? = when (c) {
        is RumbleCommand.Start -> c.level
        is RumbleCommand.Change -> c.level
        else -> null
    }

    @Test
    fun laVibracionEmpiezaCambiaYPara() {
        val t = RumbleTrack()
        assertEquals(RumbleCommand.Start(1f), t.apply(1, 255, 0, 400, 0))
        val change = t.apply(2, 128, 0, 400, 50)
        assertTrue("$change", change is RumbleCommand.Change)
        assertEquals(128f / 255f, level(change)!!, 1e-3f)
        assertEquals(RumbleCommand.Stop, t.apply(3, 0, 0, 0, 100))
        assertNull("ya parada: nada", t.apply(4, 0, 0, 0, 150))
    }

    @Test
    fun unSeqViejoODuplicadoSeIgnora() {
        val t = RumbleTrack()
        assertTrue(t.apply(5, 255, 255, 400, 0) is RumbleCommand.Start)
        assertNull("repetido", t.apply(5, 0, 0, 0, 10))
        assertNull("viejo", t.apply(4, 0, 0, 0, 20))
        assertNull("sigue vibrando", t.tick(100))
        assertEquals(RumbleCommand.Stop, t.apply(6, 0, 0, 0, 30))
        // Desbordamiento: 0 es «más nuevo» que 0xFFFFFFFF
        val w = RumbleTrack()
        assertTrue(w.apply(-1, 255, 0, 400, 0) is RumbleCommand.Start)
        assertEquals(RumbleCommand.Stop, w.apply(0, 0, 0, 0, 1))
    }

    @Test
    fun sinRefrescoLaVibracionCaducaSola() {
        val t = RumbleTrack()
        assertTrue(t.apply(1, 200, 0, 400, 1000) is RumbleCommand.Start)
        assertNull(t.tick(1399))
        assertEquals(RumbleCommand.Stop, t.tick(1400))
        assertNull("ya parada: nada", t.tick(1500))
    }

    @Test
    fun elRefrescoAlargaLaVibracion() {
        val t = RumbleTrack()
        assertTrue(t.apply(1, 200, 0, 400, 1000) is RumbleCommand.Start)
        assertNull("el refresco solo alarga", t.apply(2, 200, 0, 400, 1300))
        assertNull(t.tick(1400))
        assertEquals(RumbleCommand.Stop, t.tick(1700))
    }

    @Test
    fun elMismoNivelNoRepiteOrdenes() {
        val t = RumbleTrack()
        val start = t.apply(1, 100, 40, 400, 0)
        assertTrue(start is RumbleCommand.Start)
        assertEquals(100f / 255f, level(start)!!, 1e-3f)
        assertNull(t.apply(2, 100, 40, 400, 10))
        assertNull("el máximo es el mismo", t.apply(3, 40, 100, 400, 20))
    }

    @Test
    fun resetParaSiEstabaVibrando() {
        val t = RumbleTrack()
        assertNull("sobre nuevo, nada", t.reset())
        assertTrue(t.apply(1, 255, 0, 400, 0) is RumbleCommand.Start)
        assertEquals(RumbleCommand.Stop, t.reset())
        assertNull(t.reset())
        assertTrue("lastSeq borrado", t.apply(1, 255, 0, 400, 0) is RumbleCommand.Start)
    }

    @Test
    fun sinTtlSeUsanCuatrocientosMs() {
        val t = RumbleTrack()
        assertTrue(t.apply(1, 255, 0, 0, 0) is RumbleCommand.Start)
        assertNull(t.tick(399))
        assertEquals(RumbleCommand.Stop, t.tick(400))
    }

    @Test
    fun laEscalaDelAjuste() {
        assertEquals(1f, RumblePref.scale("high"), 1e-3f)
        assertEquals(0.65f, RumblePref.scale("normal"), 1e-3f)
        assertEquals(0.35f, RumblePref.scale("low"), 1e-3f)
        assertEquals(0f, RumblePref.scale("off"), 1e-3f)
        assertEquals("normal", RumblePref.normalize(null))
        assertEquals("desconocido → normal", "normal", RumblePref.normalize("fuerte"))
        assertEquals("low", RumblePref.normalize("low"))
        assertEquals(listOf("high", "normal", "low", "off"), RumblePref.ALL.map { it.key })
    }
}
