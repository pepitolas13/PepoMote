package dev.pepotech.pepomote.control

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs

/** El pulgar en px (y hacia abajo) → stick −127..127 (y hacia arriba). */
class StickMapTest {

    private val r = 100f

    @Test
    fun centroYZonaMuertaDanCero() {
        assertEquals(0 to 0, StickMap.map(0f, 0f, r))
        assertEquals(0 to 0, StickMap.map(5f, 5f, r)) // |v| = 7 < 8 % de 100
        assertEquals(0 to 0, StickMap.map(0f, -7.9f, r))
    }

    @Test
    fun ejesLlenosYSignos() {
        assertEquals(127 to 0, StickMap.map(r, 0f, r)) // derecha
        assertEquals(-127 to 0, StickMap.map(-r, 0f, r)) // izquierda
        assertEquals(0 to 127, StickMap.map(0f, -r, r)) // dedo arriba = +y
        assertEquals(0 to -127, StickMap.map(0f, r, r)) // dedo abajo = −y
    }

    @Test
    fun fueraDelRadioSatura() {
        assertEquals(127 to 0, StickMap.map(3 * r, 0f, r))
        val (x, y) = StickMap.map(-300f, 400f, r) // 3-4-5: dirección conservada
        assertTrue(abs(x + 76) <= 1 && abs(y + 102) <= 1)
    }

    @Test
    fun sinEscalonAlSalirDelCentro() {
        val (x, _) = StickMap.map(r * StickMap.DEAD_ZONE + 0.5f, 0f, r)
        assertTrue("justo fuera de la zona muerta debe ser casi 0, fue $x", x in 1..2)
        val (mid, _) = StickMap.map(r / 2, 0f, r)
        assertTrue("a mitad de radio debe rondar la mitad, fue $mid", mid in 55..62)
    }

    @Test
    fun diagonalNoSuperaElRadio() {
        val (x, y) = StickMap.map(r, -r, r)
        assertTrue(x in 88..92 && y in 88..92) // 127·cos 45°
    }
}
