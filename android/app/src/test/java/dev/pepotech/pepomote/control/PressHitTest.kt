package dev.pepotech.pepomote.control

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Qué botón hay bajo el dedo: los redondos se aciertan por su círculo (la
 * esquina de A no pulsa A) y, cuando dos se solapan, gana el que tiene el
 * centro más cerca.
 */
class PressHitTest {
    private val A = 1
    private val B = 2

    private fun square(bit: Int, x: Float, y: Float, size: Float = 100f) =
        PressZone(bit, x, y, x + size, y + size)

    private fun circle(bit: Int, x: Float, y: Float, size: Float = 100f) =
        PressZone(bit, x, y, x + size, y + size, circular = true)

    @Test fun elCuadradoSeAciertaHastaLaEsquina() {
        val zone = square(A, 0f, 0f)
        assertTrue(zone.contains(50f, 50f))
        assertTrue(zone.contains(1f, 1f))
        assertTrue(zone.contains(100f, 100f))
        assertFalse(zone.contains(101f, 50f))
        assertFalse(zone.contains(50f, -1f))
    }

    @Test fun elRedondoNoSeAciertaPorLasEsquinas() {
        val zone = circle(A, 0f, 0f)
        assertTrue(zone.contains(50f, 50f))
        assertTrue(zone.contains(50f, 2f))
        // la esquina del cuadrado que lo envuelve queda fuera del círculo
        assertFalse(zone.contains(2f, 2f))
        assertFalse(zone.contains(98f, 98f))
    }

    @Test fun fueraDeTodoNoHayBoton() {
        val zones = listOf(square(A, 0f, 0f), circle(B, 200f, 0f))
        assertNull(PressHit.resolve(zones, 150f, 50f))
        assertNull(PressHit.resolve(zones, 205f, 5f)) // esquina del redondo
        assertNull(PressHit.resolve(emptyList(), 50f, 50f))
    }

    @Test fun cadaDedoCaeEnSuBoton() {
        val zones = listOf(square(A, 0f, 0f), circle(B, 200f, 0f))
        assertEquals(A, PressHit.resolve(zones, 50f, 50f))
        assertEquals(B, PressHit.resolve(zones, 250f, 50f))
    }

    @Test fun enUnSolapeGanaElCentroMasCercano() {
        // dos cuadrados que se pisan entre 80 y 100
        val zones = listOf(square(A, 0f, 0f), square(B, 80f, 0f))
        assertEquals(A, PressHit.resolve(zones, 85f, 50f))
        assertEquals(B, PressHit.resolve(zones, 95f, 50f))
    }
}
