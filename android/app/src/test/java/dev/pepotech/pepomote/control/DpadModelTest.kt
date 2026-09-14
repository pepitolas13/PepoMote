package dev.pepotech.pepomote.control

import org.junit.Assert.assertEquals
import org.junit.Test

/** Cruceta de una pieza: dónde está el dedo → direcciones; y su mapeo a botones. */
class DpadModelTest {
    private val half = 90f // brazos de media anchura 30, centro muerto de radio 18

    private fun d(x: Float, y: Float) = DpadModel.dirs(x, y, half)

    @Test
    fun centroMuertoPequeno() {
        assertEquals(0, d(0f, 0f))
        assertEquals(0, d(10f, 10f)) // a 14 del centro
        assertEquals(0, d(-17f, 0f))
        // Justo fuera del círculo: ya es una dirección (antes, todo el cuadrado central de 60 era muerto)
        assertEquals(DpadModel.RIGHT, d(19f, 0f))
        assertEquals(DpadModel.DOWN, d(12f, 14f)) // a 18,4: en el cuadrado central manda el eje dominante
        assertEquals(DpadModel.RIGHT, d(15f, 12f))
        assertEquals(DpadModel.LEFT, d(-25f, 0f))
        assertEquals(DpadModel.UP, d(0f, -25f))
    }

    @Test
    fun sobreUnBrazoSoloSuDireccion() {
        assertEquals(DpadModel.UP, d(0f, -60f))
        assertEquals(DpadModel.RIGHT, d(60f, 0f))
        assertEquals("el dedo descentrado en el brazo no añade la diagonal", DpadModel.RIGHT, d(60f, 20f))
        assertEquals("hasta el borde del brazo", DpadModel.RIGHT, d(60f, -30f))
        assertEquals(DpadModel.DOWN, d(20f, 60f))
        assertEquals(DpadModel.LEFT, d(-60f, -29f))
        // Fuera de la cruceta por la punta: sigue siendo esa dirección (el pulgar resbala)
        assertEquals(DpadModel.RIGHT, d(200f, 5f))
        assertEquals(DpadModel.UP, d(-10f, -400f))
    }

    @Test
    fun lasEsquinasSonDiagonales() {
        assertEquals(DpadModel.DOWN or DpadModel.RIGHT, d(50f, 50f))
        assertEquals(DpadModel.DOWN or DpadModel.LEFT, d(-40f, 35f))
        assertEquals(DpadModel.UP or DpadModel.LEFT, d(-35f, -40f))
        assertEquals("justo pasada la esquina del cuadrado central", DpadModel.UP or DpadModel.RIGHT, d(31f, -31f))
        assertEquals("también fuera de la cruceta", DpadModel.DOWN or DpadModel.RIGHT, d(200f, 200f))
    }

    @Test
    fun botonesDelMandoYDeLado() {
        assertEquals(ButtonState.DPAD_UP, DpadModel.buttons(DpadModel.UP, sideways = false))
        assertEquals(ButtonState.DPAD_DOWN or ButtonState.DPAD_LEFT, DpadModel.buttons(DpadModel.DOWN or DpadModel.LEFT, sideways = false))
        assertEquals(0, DpadModel.buttons(0, sideways = true))
        // De lado (IR a la izquierda): arriba en pantalla es el RIGHT del mando…
        assertEquals(ButtonState.DPAD_RIGHT, DpadModel.buttons(DpadModel.UP, sideways = true))
        assertEquals(ButtonState.DPAD_RIGHT or ButtonState.DPAD_DOWN, DpadModel.buttons(DpadModel.UP or DpadModel.RIGHT, sideways = true))
        assertEquals(
            ButtonState.DPAD_UP or ButtonState.DPAD_DOWN or ButtonState.DPAD_LEFT or ButtonState.DPAD_RIGHT,
            DpadModel.buttons(DpadModel.UP or DpadModel.DOWN or DpadModel.LEFT or DpadModel.RIGHT, sideways = true)
        )
    }
}
