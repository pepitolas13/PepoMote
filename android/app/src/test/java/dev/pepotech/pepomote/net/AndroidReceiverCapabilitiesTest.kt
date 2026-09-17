package dev.pepotech.pepomote.net

import org.junit.Assert.*
import org.junit.Test
import java.net.ServerSocket
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

class AndroidReceiverCapabilitiesTest {
    private fun handshake(platform: String?, modes: String = "\"pointer\",\"dolphin\",\"cemu\",\"switch\""): ControlClient.Ok {
        val replies = LinkedBlockingQueue<ControlClient.Ok>()
        ServerSocket(0).use { server ->
            server.soTimeout = 4000
            val client = ControlClient("127.0.0.1", server.localPort, "123456", "Phone", "Phone", "wiimote",
                object : ControlClient.Callbacks {
                    override fun onOk(ok: ControlClient.Ok) { replies.add(ok) }
                    override fun onError(code: String, msg: String) = Unit
                    override fun onModeChanged(mode: String, byPc: Boolean) = Unit
                    override fun onPadChanged(pad: String, player: Int?) = Unit
                    override fun onNunchukChanged(own: Boolean) = Unit
                    override fun onNotice(text: String) = Unit
                    override fun onClosed() = Unit
                })
            try {
                server.accept().use { socket ->
                    socket.soTimeout = 4000
                    assertTrue(socket.getInputStream().bufferedReader().readLine().contains("hello"))
                    val identity = platform?.let { "\"platform\":\"$it\"," }.orEmpty()
                    socket.getOutputStream().write(("""{"m":"ok",${identity}"session_id":17,"mode":"dolphin","modes":[$modes],"text_input":false,"pair_token":"permanent-token"}""" + "\n").toByteArray())
                    socket.getOutputStream().flush()
                    return replies.poll(3, TimeUnit.SECONDS) ?: error("No handshake response")
                }
            } finally { client.close() }
        }
    }

    @Test fun androidReceiverNeverEnablesWiiU() {
        val ok = handshake("android")
        assertFalse("el servidor Android no ofrece Wii U", ok.supportsCemu)
        assertFalse("un servidor Android anterior no anuncia RetroArch", ok.supportsRetroArch)
        assertTrue(ok.supportsSwitch)
        assertEquals(ReceiverCapabilities.ANDROID, ok.platform)
        assertFalse(ok.textInput)
        assertEquals("permanent-token", ok.pairToken)
    }

    @Test fun androidServerOffersRetroArchOnlyWhenItAnnouncesIt() {
        val ok = handshake("android", "\"dolphin\",\"switch\",\"retroarch\"")
        assertTrue(ok.supportsRetroArch)
        assertFalse(ok.supportsCemu)
        assertEquals(listOf("dolphin", "switch", "retroarch"), ReceiverCapabilities.modes("android", true, true, true))
        assertEquals(listOf("dolphin", "switch"), ReceiverCapabilities.modes("android", true, true, false))
        assertEquals("retroarch", ReceiverCapabilities.select("retroarch", "dolphin", "android", false, true, true))
        assertEquals("dolphin", ReceiverCapabilities.select("retroarch", "dolphin", "android", false, true, false))
        assertEquals("dolphin", ReceiverCapabilities.select("pointer", "dolphin", "android", false, true, true))
    }

    @Test fun existingPcReceiverKeepsItsModes() {
        val ok = handshake(null)
        assertTrue(ok.supportsCemu)
        assertTrue(ok.supportsSwitch)
        assertEquals(ReceiverCapabilities.DESKTOP, ok.platform)
    }

    @Test fun androidSelectionCannotRestoreAPcOnlyMode() {
        assertEquals(listOf("dolphin", "switch"), ReceiverCapabilities.modes("android", true, true))
        assertEquals("dolphin", ReceiverCapabilities.select("pointer", "dolphin", "android", false, true))
        assertEquals("switch", ReceiverCapabilities.select("cemu", "switch", "android", false, true))
        assertEquals("switch", ReceiverCapabilities.select("switch", "dolphin", "android", false, true))
        assertEquals("pointer", ReceiverCapabilities.select(null, "pointer", "desktop", true, true))
    }

    @Test fun pairingPlatformSurvivesSavingWithoutChangingOldPcRecords() {
        val pc = Pairing("192.168.1.3", 26761, "old-pc", "My PC")
        val android = Pairing("192.168.1.4", 26761, "new-phone", "Android TV", "android")
        assertEquals(listOf(pc, android), PairList.decode(PairList.encode(listOf(pc, android))))
        assertFalse(PairList.encode(listOf(pc)).contains("platform="))
    }
}
