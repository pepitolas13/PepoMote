package dev.pepotech.pepomote.control

import org.junit.Assert.assertEquals
import org.junit.Test

/** La cruceta de lado manda los botones de un mando girado con el IR a la izquierda. */
class SidewaysDpadTest {
    @Test
    fun deLado() {
        assertEquals(ButtonState.DPAD_RIGHT, SidewaysDpad.up(true))
        assertEquals(ButtonState.DPAD_LEFT, SidewaysDpad.down(true))
        assertEquals(ButtonState.DPAD_UP, SidewaysDpad.left(true))
        assertEquals(ButtonState.DPAD_DOWN, SidewaysDpad.right(true))
        // los cuatro brazos siguen siendo los cuatro botones, sin repetir
        val bits = setOf(SidewaysDpad.up(true), SidewaysDpad.down(true), SidewaysDpad.left(true), SidewaysDpad.right(true))
        assertEquals(setOf(ButtonState.DPAD_UP, ButtonState.DPAD_DOWN, ButtonState.DPAD_LEFT, ButtonState.DPAD_RIGHT), bits)
    }

    @Test
    fun vertical() {
        assertEquals(ButtonState.DPAD_UP, SidewaysDpad.up(false))
        assertEquals(ButtonState.DPAD_DOWN, SidewaysDpad.down(false))
        assertEquals(ButtonState.DPAD_LEFT, SidewaysDpad.left(false))
        assertEquals(ButtonState.DPAD_RIGHT, SidewaysDpad.right(false))
    }
}
