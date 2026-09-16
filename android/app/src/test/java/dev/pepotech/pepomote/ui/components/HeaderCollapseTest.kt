package dev.pepotech.pepomote.ui.components

import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Cabecera plegable de los mandos apaisados: cuándo se pliega sola y cuándo
 * vuelve sola. La tarjeta tapa B/Z/C, así que se quita de en medio enseguida,
 * pero nunca cuando el usuario no puede verla desaparecer (lector de pantalla)
 * ni cuando hace falta «Salir» (sin enlace).
 */
class HeaderCollapseTest {

    @Test fun cuatroSegundosDeCortesia() {
        assertEquals(4000L, HeaderCollapse.AUTO_MS)
    }

    @Test fun conElEnlaceVivoSePliegaSola() {
        assertTrue(HeaderCollapse.autoCollapses(connected = true, screenReader = false))
    }

    @Test fun sinEnlaceSeQuedaAbierta() {
        assertFalse(HeaderCollapse.autoCollapses(connected = false, screenReader = false))
        assertFalse(HeaderCollapse.autoCollapses(connected = false, screenReader = true))
    }

    @Test fun conLectorDePantallaNoSePliegaSola() {
        assertFalse(HeaderCollapse.autoCollapses(connected = true, screenReader = true))
    }

    @Test fun soloSeDespliegaSolaAlCaerseElEnlace() {
        assertTrue(HeaderCollapse.expandsOn(UiLink.Disconnected))
        // reconectando no: el servicio lo arregla solo y el mando sigue en la mano
        assertFalse(HeaderCollapse.expandsOn(UiLink.Reconnecting("PC de Dani", 2)))
        assertFalse(HeaderCollapse.expandsOn(UiLink.Connecting))
        assertFalse(HeaderCollapse.expandsOn(UiLink.Failed("busy", "ocupado")))
        assertFalse(HeaderCollapse.expandsOn(connected()))
        // un cambio de modo del PC tampoco la despliega
        assertFalse(HeaderCollapse.expandsOn(connected(mode = LinkState.MODE_CEMU)))
    }

    private fun connected(mode: String = LinkState.MODE_DOLPHIN): UiLink.Connected =
        UiLink.Connected(pcName = "PC de Dani", mode = mode, rttMs = 12f, sensorHz = 60f)
}
