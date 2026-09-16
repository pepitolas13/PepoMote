package dev.pepotech.pepomote.ui.components

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Los textos de la cabecera plegable del GamePad: lo que dice la pastilla
 * (el modo que se quiere, o el estado del enlace) y la línea de estado de la
 * tarjeta, que ya no se recorta por ancho porque la tarjeta tiene sitio.
 */
class GamePadHeaderTextTest {

    @Test fun conectadoLaPastillaDiceElModo() {
        assertEquals(
            "Wii U",
            GamePadHeaderText.handle(connected = true, connecting = false, mode = "Wii U", connectingText = "Conectando…", disconnectedText = "Sin conexión")
        )
        assertEquals(
            "Switch",
            GamePadHeaderText.handle(connected = true, connecting = false, mode = "Switch", connectingText = "Conectando…", disconnectedText = "Sin conexión")
        )
    }

    @Test fun sinEnlaceLaPastillaDiceElEstado() {
        assertEquals(
            "Conectando…",
            GamePadHeaderText.handle(connected = false, connecting = true, mode = "Wii U", connectingText = "Conectando…", disconnectedText = "Sin conexión")
        )
        assertEquals(
            "Sin conexión",
            GamePadHeaderText.handle(connected = false, connecting = false, mode = "Wii U", connectingText = "Conectando…", disconnectedText = "Sin conexión")
        )
    }

    @Test fun reconectandoNoEsAsuntoDelTextoSinoDeLaPastilla() {
        // Reconectando, la pastilla pinta el punto latiendo y no este texto:
        // aquí, sin enlace ni conexión en marcha, queda «sin conexión»
        assertEquals(
            "Sin conexión",
            GamePadHeaderText.handle(connected = false, connecting = false, mode = "Switch", connectingText = "Conectando…", disconnectedText = "Sin conexión")
        )
    }

    @Test fun conElModoConfirmadoElEstadoLlevaMandoYRitmo() {
        assertEquals(
            "J1 · GamePad · 23 ms",
            GamePadHeaderText.status(operative = true, player = 1, padName = "GamePad", rttMs = 23.4f, activating = "Activando Wii U…")
        )
        assertEquals(
            "J2 · Pro Controller · 8 ms",
            GamePadHeaderText.status(operative = true, player = 2, padName = "Pro Controller", rttMs = 7.6f, activating = "Activando Switch…")
        )
    }

    @Test fun sinIdaYVueltaConocidoNoSeInventaNada() {
        assertEquals(
            "J1 · GamePad",
            GamePadHeaderText.status(operative = true, player = 1, padName = "GamePad", rttMs = null, activating = "Activando Wii U…")
        )
    }

    @Test fun mientrasSeEsperaElEcoSaleElAviso() {
        assertEquals(
            "Activando Wii U…",
            GamePadHeaderText.status(operative = false, player = 1, padName = "GamePad", rttMs = 12f, activating = "Activando Wii U…")
        )
        assertEquals(
            "Activando Switch…",
            GamePadHeaderText.status(operative = false, player = 1, padName = "Pro Controller", rttMs = null, activating = "Activando Switch…")
        )
    }
}
