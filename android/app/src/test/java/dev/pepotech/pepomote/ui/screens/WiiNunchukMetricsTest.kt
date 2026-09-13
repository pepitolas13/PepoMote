package dev.pepotech.pepomote.ui.screens

import dev.pepotech.pepomote.ui.components.UiScale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Medidas del mando + Nunchuk apaisado: cabe todo en móviles y tablets. */
class WiiNunchukMetricsTest {
    private fun fits(m: WiiNunchukMetrics, w: Float, h: Float) {
        // stick a la izquierda, columna central y cruceta + A a la derecha, sin solaparse
        assertTrue("ancho $w", m.margin + m.stick + m.centerMin <= w - m.margin - m.cross - m.gap - m.a + 0.01f)
        // el stick cabe bajo las pastillas Z/C
        assertTrue("alto $h", m.headerH + m.pillH + m.gap + m.stick + m.margin <= h + 0.01f)
        assertTrue(m.stick >= 120f)
    }

    @Test
    fun movil() {
        val m = WiiNunchukMetrics(852f, 393f, UiScale.landscape(852f, 393f))
        assertEquals(1f, m.s, 1e-3f)
        assertEquals(280f, m.stick, 0.5f)
        assertEquals(150f, m.cross, 0.5f)
        assertEquals(148f, m.a, 0.5f)
        fits(m, 852f, 393f)
        // iPhone 16 con recortes / móvil estrecho: el stick cede sitio al centro
        val narrow = WiiNunchukMetrics(667f, 375f, 1f)
        assertEquals(174f, narrow.stick, 0.5f)
        fits(narrow, 667f, 375f)
    }

    @Test
    fun tablet() {
        val s = UiScale.landscape(1180f, 820f)
        val m = WiiNunchukMetrics(1180f, 820f, s)
        assertTrue(m.s > 1.5f)
        assertTrue(m.stick > 280f)
        assertTrue(m.cross > 200f && m.a > 200f)
        fits(m, 1180f, 820f)
        assertEquals((16 * s).toInt().toFloat(), m.text(16f).toFloat(), 1f)
    }
}
