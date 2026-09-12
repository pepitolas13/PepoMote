package dev.pepotech.pepomote.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Routing del mando (addendum UX v2): función pura de enlace + intención. */
class RouteTest {

    private fun connected(
        mode: String = LinkState.MODE_POINTER,
        role: String = LinkState.ROLE_WIIMOTE,
        pad: String = LinkState.PAD_GAMEPAD,
        slot: Int = 0,
        supportsCemu: Boolean = true
    ) = UiLink.Connected(
        pcName = "PC", mode = mode, rttMs = null, sensorHz = 0f,
        slot = slot, role = role, supportsCemu = supportsCemu, pad = pad
    )

    @Test
    fun gamePadConCemuConfirmado() {
        assertEquals(PadScreen.GamePad, Route.route(connected(mode = "cemu"), PadIntent.None))
        // Jugador 2: Pro Controller, misma pantalla
        assertEquals(PadScreen.GamePad, Route.route(connected(mode = "cemu", pad = "pro", slot = 1), PadIntent.None))
        // Sea cual sea la intención
        assertEquals(PadScreen.GamePad, Route.route(connected(mode = "cemu"), PadIntent.WiiU))
    }

    @Test
    fun mandoDeWiiDentroDeWiiUEsLayoutWii() {
        assertEquals(PadScreen.Wii, Route.route(connected(mode = "cemu", pad = "wiimote"), PadIntent.None))
    }

    @Test
    fun gamePadOptimistaConIntencionPendiente() {
        // Conectando (tarjeta Wii U del inicio, aún sin ok)
        assertEquals(PadScreen.GamePad, Route.route(UiLink.Connecting, PadIntent.WiiU))
        // Conectado en otro modo, esperando el eco (chip Wii U o mode cemu tras el ok)
        assertEquals(PadScreen.GamePad, Route.route(connected(mode = "pointer"), PadIntent.WiiU))
        assertEquals(PadScreen.GamePad, Route.route(connected(mode = "dolphin"), PadIntent.WiiU))
    }

    @Test
    fun nunchukSiempreNunchuk() {
        assertEquals(PadScreen.Nunchuk, Route.route(connected(role = "nunchuk"), PadIntent.None))
        assertEquals(PadScreen.Nunchuk, Route.route(connected(mode = "cemu", role = "nunchuk", pad = "nunchuk"), PadIntent.None))
        // Un Nunchuk nunca es GamePad, aunque hubiera intención
        assertEquals(PadScreen.Nunchuk, Route.route(connected(role = "nunchuk"), PadIntent.WiiU))
    }

    @Test
    fun enCualquierOtroCasoLayoutWii() {
        assertEquals(PadScreen.Wii, Route.route(connected(mode = "pointer"), PadIntent.None))
        assertEquals(PadScreen.Wii, Route.route(connected(mode = "dolphin"), PadIntent.None))
        assertEquals(PadScreen.Wii, Route.route(UiLink.Connecting, PadIntent.None))
        assertEquals(PadScreen.Wii, Route.route(UiLink.Disconnected, PadIntent.None))
        // Sin enlace la intención no cuenta
        assertEquals(PadScreen.Wii, Route.route(UiLink.Disconnected, PadIntent.WiiU))
        assertEquals(PadScreen.Wii, Route.route(UiLink.Failed("io", "x"), PadIntent.WiiU))
    }

    @Test
    fun ecoCemuConsumeLaIntencionSinAviso() {
        val out = Route.afterModeEcho(PadIntent.WiiU, "cemu", connected(mode = "cemu"))
        assertEquals(PadIntent.None, out.intent)
        assertNull(out.warning)
    }

    @Test
    fun difusionDelPcConsumeLaIntencionSinAviso() {
        // el PC abrió Dolphin (modo automático) mientras se esperaba Wii U: ni
        // «PC antiguo» ni «solo el Jugador 1», su aviso ya lo explica
        val out = Route.afterModeEcho(PadIntent.WiiU, "dolphin", connected(mode = "dolphin", slot = 1), byPc = true)
        assertEquals(PadIntent.None, out.intent)
        assertNull(out.warning)
        // sin intención, tampoco cambia nada
        assertEquals(PadIntent.None, Route.afterModeEcho(PadIntent.None, "pointer", connected(), byPc = true).intent)
    }

    @Test
    fun ecoPointerConIntencionPendienteVuelveAWiiConAviso() {
        // PC antiguo: el ok no traía "cemu" en modes y contesta pointer a mode cemu
        val link = connected(mode = "pointer", supportsCemu = false)
        val out = Route.afterModeEcho(PadIntent.WiiU, "pointer", link)
        assertEquals(PadIntent.None, out.intent)
        assertEquals(Route.WARN_NEEDS_13, out.warning)
        assertEquals(PadScreen.Wii, Route.route(link, out.intent))
    }

    @Test
    fun ecoDeOtroModoSiendoJugador2AvisaQueSoloElJugador1CambiaElModo() {
        val link = connected(mode = "pointer", slot = 1, supportsCemu = true)
        val out = Route.afterModeEcho(PadIntent.WiiU, "pointer", link)
        assertEquals(PadIntent.None, out.intent)
        assertEquals(Route.WARN_PLAYER_1, out.warning)
        assertEquals(PadScreen.Wii, Route.route(link, out.intent))
    }

    @Test
    fun ecoSinIntencionNoCambiaNada() {
        val out = Route.afterModeEcho(PadIntent.None, "pointer", connected(mode = "pointer"))
        assertEquals(PadIntent.None, out.intent)
        assertNull(out.warning)
        // Una difusión de cemu sin intención tampoco avisa (la pantalla cambia por el modo)
        assertNull(Route.afterModeEcho(PadIntent.None, "cemu", connected(mode = "cemu")).warning)
    }

    @Test
    fun ecoAntesDelOkConIntencionAvisaDePcAntiguo() {
        val out = Route.afterModeEcho(PadIntent.WiiU, "pointer", UiLink.Connecting)
        assertEquals(PadIntent.None, out.intent)
        assertEquals(Route.WARN_NEEDS_13, out.warning)
    }

    @Test
    fun isGamePad() {
        assertEquals(true, Route.isGamePad(connected(mode = "cemu")))
        assertEquals(true, Route.isGamePad(connected(mode = "cemu", pad = "pro", slot = 2)))
        assertEquals(false, Route.isGamePad(connected(mode = "cemu", pad = "wiimote")))
        assertEquals(false, Route.isGamePad(connected(mode = "cemu", role = "nunchuk", pad = "nunchuk")))
        assertEquals(false, Route.isGamePad(connected(mode = "dolphin")))
        assertEquals(false, Route.isGamePad(UiLink.Connecting))
    }
}
