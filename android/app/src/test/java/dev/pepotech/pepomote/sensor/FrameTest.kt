package dev.pepotech.pepomote.sensor

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Test
import kotlin.math.sqrt

/** Ejes del móvil apaisado → marco del GamePad (contrato Wii U §4). */
class FrameTest {

    private val eps = 1e-5f
    private val h = 0.70710678f

    @Test
    fun tumbadoPlanoLaGravedadNoCambia() {
        val g = floatArrayOf(0f, 0f, 9.8f)
        assertArrayEquals(g, Frame.remapAccel(g, Frame.ROTATION_90), eps)
        assertArrayEquals(g, Frame.remapAccel(g, Frame.ROTATION_270), eps)
        assertArrayEquals(g, Frame.remapGyro(g, Frame.ROTATION_90), eps)
        assertArrayEquals(g, Frame.remapGyro(g, Frame.ROTATION_270), eps)
    }

    @Test
    fun giroIzquierdaVectores() {
        assertArrayEquals(floatArrayOf(0f, 1f, 0f), Frame.remapAccel(floatArrayOf(1f, 0f, 0f), Frame.ROTATION_90), eps)
        assertArrayEquals(floatArrayOf(-1f, 0f, 0f), Frame.remapAccel(floatArrayOf(0f, 1f, 0f), Frame.ROTATION_90), eps)
        assertArrayEquals(floatArrayOf(0f, 1f, 0f), Frame.remapGyro(floatArrayOf(1f, 0f, 0f), Frame.ROTATION_90), eps)
        assertArrayEquals(floatArrayOf(-1f, 0f, 0f), Frame.remapGyro(floatArrayOf(0f, 1f, 0f), Frame.ROTATION_90), eps)
    }

    @Test
    fun giroDerechaVectores() {
        assertArrayEquals(floatArrayOf(0f, -1f, 0f), Frame.remapAccel(floatArrayOf(1f, 0f, 0f), Frame.ROTATION_270), eps)
        assertArrayEquals(floatArrayOf(1f, 0f, 0f), Frame.remapAccel(floatArrayOf(0f, 1f, 0f), Frame.ROTATION_270), eps)
        assertArrayEquals(floatArrayOf(0f, -1f, 0f), Frame.remapGyro(floatArrayOf(1f, 0f, 0f), Frame.ROTATION_270), eps)
        assertArrayEquals(floatArrayOf(1f, 0f, 0f), Frame.remapGyro(floatArrayOf(0f, 1f, 0f), Frame.ROTATION_270), eps)
    }

    @Test
    fun formulaCompletaDeLosVectores() {
        val v = floatArrayOf(1f, 2f, 3f)
        assertArrayEquals(floatArrayOf(-2f, 1f, 3f), Frame.remapAccel(v, Frame.ROTATION_90), eps)
        assertArrayEquals(floatArrayOf(2f, -1f, 3f), Frame.remapAccel(v, Frame.ROTATION_270), eps)
    }

    @Test
    fun quatIdentidad() {
        val id = floatArrayOf(1f, 0f, 0f, 0f)
        assertArrayEquals(floatArrayOf(h, 0f, 0f, -h), Frame.remapQuat(id, Frame.ROTATION_90), 1e-4f)
        assertArrayEquals(floatArrayOf(h, 0f, 0f, h), Frame.remapQuat(id, Frame.ROTATION_270), 1e-4f)
    }

    @Test
    fun quatProductoDeHamiltonConRALaDerecha() {
        // q = (0.5, 0.5, 0.5, 0.5) ⊗ (h, 0, 0, −h):
        // w = 0.5h + 0.5h = h; x = 0.5h − 0.5h = 0; y = 0.5h + 0.5h = h; z = −0.5h + 0.5h = 0
        val q = floatArrayOf(0.5f, 0.5f, 0.5f, 0.5f)
        assertArrayEquals(floatArrayOf(h, 0f, h, 0f), Frame.remapQuat(q, Frame.ROTATION_90), 1e-4f)
        // ⊗ (h, 0, 0, +h): w = 0.5h − 0.5h = 0; x = 0.5h + 0.5h = h; y = 0.5h − 0.5h = 0; z = 0.5h + 0.5h = h
        assertArrayEquals(floatArrayOf(0f, h, 0f, h), Frame.remapQuat(q, Frame.ROTATION_270), 1e-4f)
    }

    @Test
    fun quatConservaLaNorma() {
        val q = floatArrayOf(0.8f, 0.1f, -0.5f, 0.3f)
        val n = sqrt(q.map { it * it }.sum())
        val r = Frame.remapQuat(q, Frame.ROTATION_90)
        assertEquals(n, sqrt(r.map { it * it }.sum()), 1e-5f)
    }

    @Test
    fun otrasRotacionesNoTocanNada() {
        val v = floatArrayOf(1f, 2f, 3f)
        val q = floatArrayOf(0.8f, 0.1f, -0.5f, 0.3f)
        for (rot in intArrayOf(Frame.ROTATION_0, Frame.ROTATION_180, 7, -1)) {
            assertArrayEquals(v, Frame.remapAccel(v, rot), 0f)
            assertArrayEquals(v, Frame.remapGyro(v, rot), 0f)
            assertArrayEquals(q, Frame.remapQuat(q, rot), 0f)
        }
    }

    @Test
    fun enSitioYSinTocarLaEntrada() {
        val v = floatArrayOf(1f, 0f, 0f)
        val out = Frame.remapAccel(v, Frame.ROTATION_90)
        assertNotSame(v, out)
        assertArrayEquals(floatArrayOf(1f, 0f, 0f), v, 0f) // la entrada no se toca
        // En sitio: mismo array de salida que de entrada
        assertSame(v, Frame.remapAccel(v, Frame.ROTATION_90, v))
        assertArrayEquals(floatArrayOf(0f, 1f, 0f), v, eps)
        val q = floatArrayOf(1f, 0f, 0f, 0f)
        assertSame(q, Frame.remapQuat(q, Frame.ROTATION_270, q))
        assertArrayEquals(floatArrayOf(h, 0f, 0f, h), q, 1e-4f)
    }

    @Test
    fun valoresDeSurface() {
        // Mismos enteros que android.view.Surface.ROTATION_*
        assertEquals(0, Frame.ROTATION_0)
        assertEquals(1, Frame.ROTATION_90)
        assertEquals(2, Frame.ROTATION_180)
        assertEquals(3, Frame.ROTATION_270)
    }
}
