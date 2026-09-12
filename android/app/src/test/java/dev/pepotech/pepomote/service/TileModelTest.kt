package dev.pepotech.pepomote.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TileModelTest {

    private val connected = UiLink.Connected("SALON-PC", LinkState.MODE_POINTER, null, 0f)

    @Test
    fun elTileDiceElEstado() {
        assertEquals(TileModel(true, "Conectado a SALON-PC"), TileModel.of(connected, true))
        assertEquals(TileModel(true, "Conectando…"), TileModel.of(UiLink.Connecting, true))
        assertEquals(TileModel(true, "Reconectando…"), TileModel.of(UiLink.Reconnecting("SALON-PC", 3), true))
        assertEquals(TileModel(false, "Sin conexión"), TileModel.of(UiLink.Disconnected, true))
        assertEquals(TileModel(false, "Sin conexión"), TileModel.of(UiLink.Failed("io", "x"), true))
        assertEquals(TileModel(false, "Sin PC"), TileModel.of(UiLink.Disconnected, false))
    }

    @Test
    fun unToqueConectaDesconectaOAbreLaApp() {
        assertEquals(TileAction.Stop, tileAction(connected, true))
        assertEquals(TileAction.Stop, tileAction(UiLink.Connecting, true))
        assertEquals(TileAction.Stop, tileAction(UiLink.Reconnecting("PC", 1), true))
        assertEquals(TileAction.Start, tileAction(UiLink.Disconnected, true))
        assertEquals(TileAction.Start, tileAction(UiLink.Failed("io", "x"), true))
        assertEquals(TileAction.OpenApp, tileAction(UiLink.Disconnected, false))
        assertTrue(connected.alive)
        assertFalse(UiLink.Failed("io", "x").alive)
    }
}
