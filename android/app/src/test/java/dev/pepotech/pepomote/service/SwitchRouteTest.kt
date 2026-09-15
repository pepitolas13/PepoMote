package dev.pepotech.pepomote.service

import org.junit.Assert.*
import org.junit.Test

class SwitchRouteTest {
    @Test fun controllerOwnerCanReachEverySupportedMode() {
        val left = link().copy(supportsCemu = true)
        assertEquals(listOf("pointer", "dolphin", "cemu", "switch"), Route.availableModes(left))
        assertEquals(listOf("pointer", "dolphin", "switch"), Route.availableModes(left.copy(supportsCemu = false)))
        assertEquals(emptyList<String>(), Route.availableModes(left.copy(slot = 1)))
        assertEquals(emptyList<String>(), Route.availableModes(left.copy(role = "nunchuk")))
    }

    private fun link(pad: String = "pro") =
        UiLink.Connected("PC", "switch", null, 0f, supportsSwitch = true, pad = pad)

    @Test fun oldReceiversNeverGetExtendedInput() {
        assertFalse(Route.isGamePad(link().copy(supportsSwitch = false)))
        assertFalse(Route.isGamePad(link().copy(role = "nunchuk")))
        assertTrue(Route.isGamePad(link("wiimote")))
        assertTrue(Route.isGamePad(link()))
    }

    @Test fun pendingModeKeepsItsOwnIdentity() {
        assertEquals("switch", Route.displayMode(link().copy(mode = "cemu"), PadIntent.Switch))
        assertEquals("cemu", Route.displayMode(link(), PadIntent.WiiU))
        assertEquals(PadScreen.GamePad, Route.route(UiLink.Connecting, PadIntent.Switch))
        assertEquals(PadScreen.GamePad, Route.route(UiLink.Reconnecting("PC", 2), PadIntent.Switch))
        assertEquals(PadScreen.Wii, Route.route(UiLink.Disconnected, PadIntent.Switch))
    }

    @Test fun staleJoyConAssignmentsAlwaysUseTheFullProController() {
        for (pad in listOf("pro", "joycons", "joycon_r", "joycon_side", "wiimote", "unknown")) {
            assertEquals(PadScreen.GamePad, Route.route(link(pad), PadIntent.None))
            assertTrue(Route.forcesLandscape(link(pad), PadIntent.None))
        }
    }

    @Test fun switchOffersOnlyProController() {
        assertEquals(listOf(PadScreen.GamePad, PadScreen.Wii, PadScreen.Nunchuk), PadScreen.entries)
        assertTrue(Route.forcesLandscape(link(), PadIntent.None))
    }

    @Test fun failedSwitchRequestCanRecoverToTheConfirmedMode() {
        val old = link().copy(mode = "pointer", supportsSwitch = false)
        val rejected = Route.afterModeEcho(PadIntent.Switch, "pointer", old)
        assertEquals(PadIntent.None, rejected.intent)
        assertEquals(Route.WARN_NEEDS_SWITCH, rejected.warning)
        assertEquals(PadScreen.Wii, Route.route(old, rejected.intent))
        assertEquals(Route.WARN_PLAYER_1, Route.afterModeEcho(PadIntent.Switch, "pointer", link().copy(slot = 1)).warning)
        assertNull(Route.afterModeEcho(PadIntent.Switch, "switch", link()).warning)
        assertNull(Route.afterModeEcho(PadIntent.Switch, "dolphin", link(), byPc = true).warning)
    }

    @Test fun preferencesCannotLeakBetweenConsoleModes() {
        assertNotEquals(PadPreference.key("cemu"), PadPreference.key("switch"))
        assertEquals("gamepad", PadPreference.normalize("cemu", "joycons"))
        assertEquals("pro", PadPreference.normalize("switch", "wiimote"))
        assertEquals("pro", PadPreference.normalize("switch", null))
        for (pad in listOf("joycons", "joycon_side", "joycon_r", "gamepad", "pro")) {
            assertEquals("pro", PadPreference.normalize("switch", pad))
        }
        assertNull(PadPreference.key("pointer"))
    }
}
