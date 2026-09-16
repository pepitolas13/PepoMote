package dev.pepotech.pepomote.service

import dev.pepotech.pepomote.net.ReceiverCapabilities
import org.junit.Assert.*
import org.junit.Test

class RetroArchRouteTest {
    private fun link(pad: String = "retropad", mode: String = "retroarch") =
        UiLink.Connected("PC", mode, null, 0f, supportsCemu = true, supportsSwitch = true, supportsRetroArch = true, pad = pad)

    @Test fun retroarchIsOfferedOnlyByPcReceiversThatAnnounceIt() {
        assertEquals(listOf("pointer", "dolphin", "cemu", "switch", "retroarch"), Route.availableModes(link()))
        assertEquals(listOf("pointer", "dolphin", "cemu", "switch"), Route.availableModes(link().copy(supportsRetroArch = false)))
        assertEquals(listOf("dolphin", "switch"), ReceiverCapabilities.modes(ReceiverCapabilities.ANDROID, true, true, true))
        assertEquals("dolphin", ReceiverCapabilities.select("retroarch", "dolphin", ReceiverCapabilities.ANDROID, false, true, true))
        assertEquals("retroarch", ReceiverCapabilities.select("retroarch", "pointer", ReceiverCapabilities.DESKTOP, false, false, true))
        assertEquals(emptyList<String>(), Route.availableModes(link().copy(slot = 1)))
    }

    @Test fun onlyTheRetroPadIsTheTwoStickScreen() {
        assertTrue(Route.isGamePad(link()))
        assertEquals(PadScreen.GamePad, Route.route(link(), PadIntent.None))
        assertTrue(Route.forcesLandscape(link(), PadIntent.None))
        // mando de NES y pistola: los layouts de Wii de siempre
        assertFalse(Route.isGamePad(link("nes")))
        assertEquals(PadScreen.Wii, Route.route(link("nes"), PadIntent.None))
        assertTrue("el mando de NES va fijo en apaisado", Route.forcesLandscape(link("nes"), PadIntent.None))
        assertTrue(Route.retroNes(link("nes")))
        assertFalse(Route.isGamePad(link("gun")))
        assertEquals(PadScreen.Wii, Route.route(link("gun"), PadIntent.None))
        assertFalse("la pistola se sostiene derecha", Route.forcesLandscape(link("gun"), PadIntent.None))
        assertTrue(Route.retroGun(link("gun")))
        // un receptor sin RetroArch nunca da mando de RetroArch
        assertFalse(Route.isGamePad(link().copy(supportsRetroArch = false)))
        assertFalse(Route.isRetroArch(link().copy(supportsRetroArch = false)))
        assertFalse(Route.isRetroArch(link().copy(role = "nunchuk")))
        assertFalse(Route.isRetroArch(link(mode = "switch")))
    }

    @Test fun theDpadIsNeverSidewaysInRetroArch() {
        // el receptor mapea la cruceta tal como se ve: nada del giro de Dolphin
        assertFalse(Route.sidewaysDpad(link("nes")))
        assertFalse(Route.sidewaysDpad(link("gun")))
        assertTrue(Route.sidewaysDpad(link(mode = "dolphin")))
    }

    @Test fun pendingIntentOpensTheRetroPadOptimistically() {
        assertEquals("retroarch", Route.displayMode(link(mode = "pointer"), PadIntent.RetroArch))
        assertEquals(PadScreen.GamePad, Route.route(UiLink.Connecting, PadIntent.RetroArch))
        assertEquals(PadScreen.GamePad, Route.route(link(mode = "pointer"), PadIntent.RetroArch))
        assertEquals(PadScreen.Wii, Route.route(UiLink.Disconnected, PadIntent.RetroArch))
    }

    @Test fun rejectedRetroArchRequestExplainsWhy() {
        val old = link(mode = "pointer").copy(supportsRetroArch = false)
        val rejected = Route.afterModeEcho(PadIntent.RetroArch, "pointer", old)
        assertEquals(PadIntent.None, rejected.intent)
        assertEquals(Route.WARN_NEEDS_RETROARCH, rejected.warning)
        assertEquals(Route.WARN_PLAYER_1, Route.afterModeEcho(PadIntent.RetroArch, "pointer", link(mode = "pointer").copy(slot = 1)).warning)
        assertNull(Route.afterModeEcho(PadIntent.RetroArch, "retroarch", link()).warning)
        assertNull(Route.afterModeEcho(PadIntent.RetroArch, "dolphin", link(), byPc = true).warning)
        // las demás intenciones siguen como estaban
        assertEquals(Route.WARN_NEEDS_SWITCH, Route.afterModeEcho(PadIntent.Switch, "pointer", old.copy(supportsSwitch = false)).warning)
    }

    @Test fun padPreferencesKeepOnlyTheThreeRetroArchPads() {
        assertEquals("retroPad", PadPreference.key("retroarch"))
        assertNotEquals(PadPreference.key("retroarch"), PadPreference.key("switch"))
        for (pad in listOf("retropad", "nes", "gun")) {
            assertEquals(pad, PadPreference.normalize("retroarch", pad))
            assertEquals(pad, PadPreference.effective("retroarch", pad))
        }
        for (pad in listOf(null, "pro", "wiimote", "gamepad", "joycons", "")) {
            assertEquals("retropad", PadPreference.normalize("retroarch", pad))
        }
        assertEquals("retropad", PadPreference.effective("retroarch", "wiimote"))
        // fuera de RetroArch sus nombres no valen
        assertEquals("gamepad", PadPreference.normalize("cemu", "nes"))
        assertEquals("pro", PadPreference.normalize("switch", "gun"))
    }

    @Test fun launchShortcutOpensRetroArch() {
        assertEquals(LaunchAction.RetroArch, LaunchAction.parse("dev.pepotech.pepomote.action.RETROARCH"))
        assertEquals(LaunchAction.Nunchuk, LaunchAction.parse("dev.pepotech.pepomote.action.NUNCHUK"))
    }
}
