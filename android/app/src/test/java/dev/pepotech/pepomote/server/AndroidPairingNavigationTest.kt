package dev.pepotech.pepomote.server

import dev.pepotech.pepomote.MainActivity
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.Screen
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.net.PairList
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.net.Pairing
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.PadIntent
import dev.pepotech.pepomote.service.UiLink
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
@LooperMode(LooperMode.Mode.PAUSED)
class AndroidPairingNavigationTest {
    private fun activity(action: (MainActivity) -> Unit) {
        val context = RuntimeEnvironment.getApplication()
        AppPrefs.setUpdateCheckEnabled(context, false)
        AppPrefs.setSoundsEnabled(context, false)
        AppPrefs.setOnboarded(context)
        LinkState.sendMode = null
        LinkState.pendingMode = null
        LinkState.publish(UiLink.Disconnected)
        val controller = Robolectric.buildActivity(MainActivity::class.java).create()
        try { action(controller.get()) }
        finally {
            controller.destroy()
            LinkState.sendMode = null
            LinkState.pendingMode = null
            LinkState.publish(UiLink.Disconnected)
        }
    }

    @Test fun qrWithoutExplicitConsoleChoiceDoesNotOverrideTheServerModeOrKeepNunchukRole() = activity { app ->
        app.linkRole = LinkState.ROLE_NUNCHUK
        app.onPairContent("pepomote://pair?v=1&host=192.168.1.22&port=26761&t=canonical-token&name=Android&platform=android")
        assertNull("Connecting through QR must keep the host's selected console", LinkState.pendingMode)
        assertEquals(LinkState.ROLE_WIIMOTE, app.linkRole)
        assertEquals(Screen.Controller, app.currentScreen)
        assertFalse(app.controllerDolphinOnly)
    }

    @Test fun badCodeCanBeEnteredAgainAndDoesNotRemoveTheSavedPc() = activity { app ->
        val pc = Pairing("192.168.1.10", 26761, "pc-credential", "My PC")
        val temporary = Pairing("192.168.1.22", 26761, "654321", "Android", "android")
        PairStore.save(app, pc)
        PairStore.save(app, temporary)
        assertTrue(PairList.isTemporaryCode(temporary))
        app.onLinkFailed(UiLink.Failed("bad_token", "wrong code"))
        assertEquals(Screen.Pair, app.currentScreen)
        assertEquals(listOf(pc), PairStore.all(app))
        assertEquals(app.getString(R.string.server_code_failed), app.pairReason)
    }

    @Test fun oldPcStillExplainsWhyWiiUIsUnsupported() = activity { app ->
        LinkState.publish(UiLink.Connected("Old PC", "pointer", null, 0f, supportsCemu = false))
        var sent: String? = null
        LinkState.sendMode = { sent = it }
        LinkState.requestMode(LinkState.MODE_CEMU)
        assertEquals("cemu", sent)
        assertEquals(PadIntent.WiiU, LinkState.intent.value)
        LinkState.resolveIntent(app, "pointer")
        assertEquals(PadIntent.None, LinkState.intent.value)
        assertEquals(app.getString(R.string.warn_needs_13), LinkState.notice.value?.text)
    }
}
