package dev.pepotech.pepomote.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LaunchActionTest {

    @Test
    fun cadaAccesoDirectoTieneSuAccion() {
        assertEquals(LaunchAction.Pointer, LaunchAction.parse("dev.pepotech.pepomote.action.POINTER"))
        assertEquals(LaunchAction.Dolphin, LaunchAction.parse("dev.pepotech.pepomote.action.DOLPHIN"))
        assertEquals(LaunchAction.WiiU, LaunchAction.parse("dev.pepotech.pepomote.action.WIIU"))
        assertEquals(LaunchAction.Nunchuk, LaunchAction.parse("dev.pepotech.pepomote.action.NUNCHUK"))
    }

    @Test
    fun loDemasNoEsUnAcceso() {
        assertNull(LaunchAction.parse(null))
        assertNull(LaunchAction.parse("android.intent.action.MAIN"))
        assertNull(LaunchAction.parse("android.intent.action.VIEW"))
        assertNull(LaunchAction.parse("dev.pepotech.pepomote.action.pointer"))
    }
}
