package dev.pepotech.pepomote.ui.components

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Medidas del GamePad: mismos vectores que iOS (PadMetricsTests). Con
 * pantalla un móvil mide lo de siempre y una tablet llena la columna lateral;
 * sin pantalla, en fila en el móvil y apilado (columnas anchas) en la tablet.
 */
class PadMetricsTest {
    private val eps = 0.5f

    @Test
    fun switchHasOneCapturePillAndNeverAStreamArea() {
        for ((w, h) in listOf(640f to 240f, 734f to 320f, 852f to 393f, 1133f to 744f, 1376f to 1032f)) {
            val m = padMetrics(w, h, pro = true, switchPad = true)
            assertTrue(m.noScreen)
            assertTrue("capture available at $w x $h", m.pillW > 0f)
            assertEquals(0f, m.touchW, 0f)
            assertEquals(0f, m.touchH, 0f)
            assertTrue(m.sideW * 2 + m.centerW + m.gap * 2 <= w + 0.1f)
            val centerExtent = if (m.row) m.roundBtn * 3 + m.pillH + m.rowGap * 3
                else m.roundBtn * 3 + m.pillW + m.rowGap * 3
            assertTrue("center fits at $w x $h", centerExtent <= (if (m.row) m.bodyH else m.centerW) + 0.1f)
        }
    }

    @Test
    fun conPantallaUnMovilMideLoDeSiempreYUnaTabletLlenaLaColumna() {
        val phone = padMetrics(852f, 393f)
        assertEquals(1f, phone.k, 1e-3f)
        assertFalse(phone.row)
        assertFalse(phone.noScreen)
        assertEquals(114.4f, phone.padSize, eps)
        assertEquals(247.1f, phone.sideW, eps)
        assertEquals(345.8f, phone.touchW, eps)
        assertEquals(44f, phone.faceBtn, eps)
        assertEquals(34.3f, phone.clickSize, eps)
        assertEquals(40.9f, phone.roundBtn, eps)
        assertEquals(66f, phone.pillW, eps)
        assertEquals(27.1f, phone.shoulderH, eps)

        val tablet13 = padMetrics(1376f, 1032f)
        assertEquals(1.439f, tablet13.k, 1e-3f)
        assertFalse(tablet13.row)
        assertEquals("antes 247: el stick y L3 llenan la columna", 329.7f, tablet13.padSize, eps)
        assertEquals(399.0f, tablet13.sideW, eps)
        assertEquals(565.9f, tablet13.touchW, eps)
        assertEquals(126.8f, tablet13.faceBtn, eps)
        assertEquals(281.9f, padMetrics(1180f, 820f).padSize, eps)
        assertEquals(269.6f, padMetrics(1133f, 744f).padSize, eps)
    }

    @Test
    fun sinPantallaEnFilaEnElMovilYApiladoEnLaTablet() {
        val phone = padMetrics(852f, 393f, noScreen = true)
        assertTrue(phone.noScreen)
        assertTrue(phone.row)
        assertEquals(190.5f, phone.padSize, eps)
        assertEquals(387f, phone.sideW, eps)
        assertEquals(66f, phone.centerW, eps)
        assertEquals(73.3f, phone.faceBtn, eps)
        assertEquals(44f, phone.clickSize, eps)
        assertEquals(40.9f, phone.roundBtn, eps)
        assertEquals(66f, phone.pillW, eps)
        // móvil con recortes (734 dp útiles): sigue en fila
        val safe = padMetrics(734f, 393f, noScreen = true)
        assertTrue(safe.row)
        assertEquals(161f, safe.padSize, eps)
        assertEquals(66f, safe.centerW, eps)

        val tablet13 = padMetrics(1376f, 1032f, noScreen = true)
        assertFalse(tablet13.row)
        assertEquals(377.6f, tablet13.padSize, eps)
        assertEquals(446.9f, tablet13.sideW, eps)
        assertEquals(470.2f, tablet13.centerW, eps)
        val tablet11 = padMetrics(1180f, 820f, noScreen = true)
        assertFalse(tablet11.row)
        assertEquals("lo limita la altura", 305.6f, tablet11.padSize, eps)
        val mini = padMetrics(1133f, 744f, noScreen = true)
        assertFalse(mini.row)
        assertEquals(269.6f, mini.padSize, eps)
    }

    @Test
    fun proControllerNuncaTienePantalla() {
        val phone = padMetrics(852f, 393f, pro = true)
        assertTrue(phone.noScreen)
        assertTrue(phone.pro)
        assertTrue(phone.row)
        assertEquals(196.8f, phone.padSize, eps)
        assertEquals("solo −, Home y + en columna", 40.9f, phone.centerW, eps)
        assertEquals(0f, phone.pillW, 1e-3f)
        val tablet13 = padMetrics(1376f, 1032f, pro = true)
        assertFalse(tablet13.row)
        assertEquals("lo limita la altura", 403.4f, tablet13.padSize, eps)
        assertEquals(260.2f, tablet13.centerW, eps)
    }
}
