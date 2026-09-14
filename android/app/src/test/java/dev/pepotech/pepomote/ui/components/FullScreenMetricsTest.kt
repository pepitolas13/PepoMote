package dev.pepotech.pepomote.ui.components

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Los mismos vectores que `FullScreenMetricsTests.swift`. */
class FullScreenMetricsTest {
    private fun assertRect(r: FitRect, x: Float, y: Float, w: Float, h: Float) {
        assertEquals("x", x, r.x, 0.5f)
        assertEquals("y", y, r.y, 0.5f)
        assertEquals("w", w, r.w, 0.5f)
        assertEquals("h", h, r.h, 0.5f)
    }

    @Test
    fun laImagenSeAjustaYCentra() {
        // iPhone 16 apaisado con un fotograma 854×480: bandas a los lados
        assertRect(FullScreenMetrics.fitRect(852f, 393f, 854f, 480f), 76.4f, 0f, 699.2f, 393f)
        // vertical (no debería pasar, pero no rompe): bandas arriba y abajo
        assertRect(FullScreenMetrics.fitRect(393f, 852f, 854f, 480f), 0f, 315.5f, 393f, 220.9f)
        // sin fotograma: 16:9
        assertRect(FullScreenMetrics.fitRect(2000f, 1000f, 0f, 0f), 111.1f, 0f, 1777.8f, 1000f)
        // contenedor sin tamaño: vacío
        assertEquals(FitRect(0f, 0f, 0f, 0f), FullScreenMetrics.fitRect(0f, 393f, 854f, 480f))
        // proporción exacta: llena todo
        assertEquals(FitRect(0f, 0f, 1280f, 720f), FullScreenMetrics.fitRect(1280f, 720f, 1280f, 720f))
    }

    @Test
    fun elTactilSoloSobreLaImagen() {
        val r1 = FullScreenMetrics.fitRect(852f, 393f, 854f, 480f)
        assertTrue(FullScreenMetrics.contains(r1, 76.4f, 0f))
        assertTrue(FullScreenMetrics.contains(r1, 426f, 196f))
        assertFalse(FullScreenMetrics.contains(r1, 50f, 10f))
        assertFalse(FullScreenMetrics.contains(r1, 800f, 100f))
        assertFalse(FullScreenMetrics.contains(FitRect(0f, 0f, 0f, 0f), 0f, 0f))
    }

    @Test
    fun laFraccionVaDe0A65535() {
        val r2 = FullScreenMetrics.fitRect(1708f, 960f, 854f, 480f)
        assertEquals(FitRect(0f, 0f, 1708f, 960f), r2)
        assertEquals(0 to 0, FullScreenMetrics.fraction(r2, 0f, 0f))
        assertEquals(65535 to 65535, FullScreenMetrics.fraction(r2, 1708f, 960f))
        assertEquals(32768 to 32768, FullScreenMetrics.fraction(r2, 854f, 480f))
        // el vector dorado del táctil (0x8000, 0x4000)
        assertEquals(32768 to 16384, FullScreenMetrics.fraction(r2, 854f, 240f))
        // recortada a los bordes
        assertEquals(0 to 65535, FullScreenMetrics.fraction(r2, -5f, 2000f))
        assertEquals(0 to 0, FullScreenMetrics.fraction(FitRect(0f, 0f, 0f, 0f), 10f, 10f))
    }

    @Test
    fun elTamanoPedidoAlPc() {
        assertEquals(854 to 480, FullScreenMetrics.streamRequest(2340, 1080))
        assertEquals(720 to 405, FullScreenMetrics.streamRequest(720, 405))
        assertEquals(854 to 480, FullScreenMetrics.streamRequest(0, 0))
    }
}
