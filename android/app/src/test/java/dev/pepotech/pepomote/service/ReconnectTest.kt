package dev.pepotech.pepomote.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ReconnectTest {

    @Test
    fun laEsperaCreceYSeQuedaEn15s() {
        assertEquals(listOf(1_000L, 2_000L, 4_000L, 8_000L, 15_000L, 15_000L, 15_000L), (1..7).map(Reconnect::delayMs))
        assertEquals("fuera de rango por abajo: como el primero", 1_000L, Reconnect.delayMs(0))
    }

    @Test
    fun seRindeALosDosMinutos() {
        val dropped = 10_000L
        assertFalse(Reconnect.giveUp(dropped, dropped + 119_999L))
        assertTrue(Reconnect.giveUp(dropped, dropped + Reconnect.GIVE_UP_MS))
        assertTrue(Reconnect.giveUp(dropped, dropped + 300_000L))
    }

    @Test
    fun elMensajeNombraAlPc() {
        assertEquals("Se perdió la conexión con SALON-PC", Reconnect.lostMessage("SALON-PC"))
    }
}
