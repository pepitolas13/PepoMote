package dev.pepotech.pepomote.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import dev.pepotech.pepomote.R
import org.junit.Test

/** Routing del mando (addendum UX v2): función pura de enlace + intención. */
class RouteTest {

    @Test
    fun extensionRequiresReceiverCapability() {
        assertEquals(false, Route.isGamePad(connected(mode = "cemu", supportsCemu = false)))
    }

    private fun connected(
        mode: String = LinkState.MODE_POINTER,
        role: String = LinkState.ROLE_WIIMOTE,
        pad: String = LinkState.PAD_GAMEPAD,
        slot: Int = 0,
        supportsCemu: Boolean = true,
        ownNunchuk: Boolean = false,
        textInput: Boolean = true
    ) = UiLink.Connected(
        pcName = "PC", mode = mode, rttMs = null, sensorHz = 0f,
        slot = slot, role = role, supportsCemu = supportsCemu, pad = pad, ownNunchuk = ownNunchuk,
        textInput = textInput
    )

    @Test
    fun elTecladoSeEnsenaDondeElReceptorSabeTeclear() {
        // Modo puntero: el que apunta (Jugador 1 con papel de mando)
        assertEquals(true, Route.showsKeyboard(connected(mode = "pointer")))
        assertEquals(false, Route.showsKeyboard(connected(mode = "pointer", slot = 1)))
        assertEquals(false, Route.showsKeyboard(connected(mode = "pointer", role = LinkState.ROLE_NUNCHUK)))
        // Lo de siempre, igual que antes
        assertEquals(true, Route.showsKeyboard(connected(mode = "cemu")))
        assertEquals(true, Route.showsKeyboard(connected(mode = "retroarch")))
        assertEquals(true, Route.showsKeyboard(connected(mode = "switch")))
        // Dolphin no: el receptor manda el texto a la ventana con el foco,
        // pero ahí los botones van al mando emulado y no hay dónde clicar
        assertEquals(false, Route.showsKeyboard(connected(mode = "dolphin")))
        // Receptor que no sabe teclear (el servidor de Android)
        for (mode in listOf("pointer", "cemu", "retroarch", "switch")) {
            assertEquals(false, Route.showsKeyboard(connected(mode = mode, textInput = false)))
        }
        // Sin enlace confirmado, nada
        assertEquals(false, Route.showsKeyboard(UiLink.Connecting))
        assertEquals(false, Route.showsKeyboard(UiLink.Disconnected))
    }

    @Test
    fun soloSeClicaAntesDeEscribirEnModoPuntero() {
        assertEquals(true, Route.clicksBeforeKeyboard(connected(mode = "pointer"), true))
        // El ajuste apagado: se abre el teclado y el clic lo da el usuario con A
        assertEquals(false, Route.clicksBeforeKeyboard(connected(mode = "pointer"), false))
        // Fuera del puntero los botones van al mando emulado (y en la pistola
        // de RetroArch A es el clic DERECHO): nunca se clica
        for (mode in listOf("cemu", "switch", "retroarch", "dolphin")) {
            assertEquals(false, Route.clicksBeforeKeyboard(connected(mode = mode), true))
        }
        // Solo el Jugador 1 con papel de mando mueve el cursor
        assertEquals(false, Route.clicksBeforeKeyboard(connected(mode = "pointer", slot = 2), true))
        assertEquals(false, Route.clicksBeforeKeyboard(connected(mode = "pointer", role = LinkState.ROLE_NUNCHUK), true))
        assertEquals(false, Route.clicksBeforeKeyboard(UiLink.Connecting, true))
    }

    @Test
    fun laPoseSoloSeCongelaSiEsteMovilMueveElCursor() {
        assertEquals(true, Route.holdsPointerForKeyboard(connected(mode = "pointer")))
        assertEquals(false, Route.holdsPointerForKeyboard(connected(mode = "pointer", slot = 1)))
        assertEquals(false, Route.holdsPointerForKeyboard(connected(mode = "pointer", role = LinkState.ROLE_NUNCHUK)))
        for (mode in listOf("cemu", "switch", "retroarch", "dolphin")) {
            assertEquals(false, Route.holdsPointerForKeyboard(connected(mode = mode)))
        }
        assertEquals(false, Route.holdsPointerForKeyboard(UiLink.Disconnected))
    }


    @Test
    fun mandoDeLadoGiraLaCrucetaYLosSensores() {
        val r90 = dev.pepotech.pepomote.sensor.Frame.ROTATION_90
        val r270 = dev.pepotech.pepomote.sensor.Frame.ROTATION_270
        val r180 = dev.pepotech.pepomote.sensor.Frame.ROTATION_180
        val r0 = dev.pepotech.pepomote.sensor.Frame.ROTATION_0
        // Dolphin y Wii U como Mando de Wii: mando girado (cruceta girada, sensores normalizados al IR a la izquierda)
        for (link in listOf(connected(mode = "dolphin"), connected(mode = "cemu", pad = "wiimote"))) {
            assertEquals(true, Route.sidewaysDpad(link))
            assertEquals(r0, Route.sidewaysRotation(link, r90))
            assertEquals(r180, Route.sidewaysRotation(link, r270))
        }
        // Puntero: flechas del PC y el móvil apunta con su borde largo (como el GamePad)
        assertEquals(false, Route.sidewaysDpad(connected(mode = "pointer")))
        assertEquals(r90, Route.sidewaysRotation(connected(mode = "pointer"), r90))
        assertEquals(r270, Route.sidewaysRotation(connected(mode = "pointer"), r270))
        // Sin enlace confirmado: como puntero
        assertEquals(false, Route.sidewaysDpad(UiLink.Connecting))
        assertEquals(r90, Route.sidewaysRotation(UiLink.Connecting, r90))
    }

    @Test
    fun apaisadoFijoConElGamePadYConElNunchukPropio() {
        // GamePad (confirmado o pedido): apaisado fijo, como siempre
        assertEquals(true, Route.forcesLandscape(connected(mode = "cemu"), PadIntent.None))
        assertEquals(true, Route.forcesLandscape(UiLink.Connecting, PadIntent.WiiU))
        // Dolphin con el Nunchuk propio confirmado: el mando se gira solo
        assertEquals(true, Route.forcesLandscape(connected(mode = "dolphin", ownNunchuk = true), PadIntent.None))
        // Dolphin sin Nunchuk, puntero, Nunchuk (rol), Wii U como Mando de Wii: sigue al móvil
        assertEquals(false, Route.forcesLandscape(connected(mode = "dolphin"), PadIntent.None))
        assertEquals(false, Route.forcesLandscape(connected(mode = "pointer", ownNunchuk = true), PadIntent.None))
        assertEquals(false, Route.forcesLandscape(connected(mode = "dolphin", role = "nunchuk", ownNunchuk = true), PadIntent.None))
        assertEquals(false, Route.forcesLandscape(connected(mode = "cemu", pad = "wiimote", ownNunchuk = true), PadIntent.None))
        assertEquals(false, Route.forcesLandscape(UiLink.Disconnected, PadIntent.None))
    }

    @Test
    fun apaisadoConNunchukSoloEnDolphinConfirmado() {
        assertEquals(true, Route.wiiLandscapeNunchuk(connected(mode = "dolphin", ownNunchuk = true)))
        // cualquier jugador, no solo el 1
        assertEquals(true, Route.wiiLandscapeNunchuk(connected(mode = "dolphin", ownNunchuk = true, slot = 2)))
        // sin confirmación del receptor (antiguo, o ajuste apagado): NES de siempre
        assertEquals(false, Route.wiiLandscapeNunchuk(connected(mode = "dolphin")))
        // en puntero y en Wii U no hay Nunchuk propio
        assertEquals(false, Route.wiiLandscapeNunchuk(connected(mode = "pointer", ownNunchuk = true)))
        assertEquals(false, Route.wiiLandscapeNunchuk(connected(mode = "cemu", pad = "wiimote", ownNunchuk = true)))
        // un Nunchuk (rol) nunca
        assertEquals(false, Route.wiiLandscapeNunchuk(connected(mode = "dolphin", role = "nunchuk", ownNunchuk = true)))
        assertEquals(false, Route.wiiLandscapeNunchuk(UiLink.Connecting))
        // la ruta principal no cambia: sigue siendo el layout Wii
        assertEquals(PadScreen.Wii, Route.route(connected(mode = "dolphin", ownNunchuk = true), PadIntent.None))
    }

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
    fun reconectandoElGamePadSeQuedaSiHayIntencion() {
        val re = UiLink.Reconnecting("PC", 2)
        assertEquals(PadScreen.GamePad, Route.route(re, PadIntent.WiiU))
        assertEquals(PadScreen.Wii, Route.route(re, PadIntent.None))
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
