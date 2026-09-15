package dev.pepotech.pepomote.net

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.BufferedReader
import java.io.BufferedWriter
import java.net.ServerSocket

class ControlClientModePrefsTest {
    private fun BufferedReader.control(): JSONObject {
        while (true) {
            val message = JSONObject(readLine() ?: error("Control channel closed"))
            if (message.optString("m") !in setOf("ping", "pong")) return message
        }
    }

    private fun withReceiver(
        screenOnly: Boolean = true,
        onMode: (ControlClient, String) -> Unit = { _, _ -> },
        test: (ControlClient, BufferedReader, BufferedWriter) -> Unit
    ) {
        ServerSocket(0).use { listener ->
            listener.soTimeout = 4000
            lateinit var client: ControlClient
            client = ControlClient("127.0.0.1", listener.localPort, "test-token", "Test", "Test", "wiimote",
                screenOnly = screenOnly,
                callbacks = object : ControlClient.Callbacks {
                    override fun onOk(ok: ControlClient.Ok) = Unit
                    override fun onModeChanged(mode: String, byPc: Boolean) = onMode(client, mode)
                    override fun onPadChanged(pad: String, player: Int?) = Unit
                    override fun onNunchukChanged(own: Boolean) = Unit
                    override fun onError(code: String, msg: String) = Unit
                    override fun onNotice(text: String) = Unit
                    override fun onClosed() = Unit
                })
            try {
                listener.accept().use { socket ->
                    socket.soTimeout = 4000
                    test(client, socket.getInputStream().bufferedReader(), socket.getOutputStream().bufferedWriter())
                }
            } finally { client.close() }
        }
    }

    @Test fun wiiUScreenPreferencePrecedesSwitchToCemu() = withReceiver { client, reader, _ ->
        val hello = reader.control()
        assertEquals("hello", hello.getString("m"))
        assertTrue("The preference remains in the handshake", hello.getBoolean("screen_only"))
        client.sendMode("cemu", cemuScreenOnly = true)
        val screenOnly = reader.control()
        assertEquals("screen_only", screenOnly.getString("m"))
        assertTrue(screenOnly.getBoolean("on"))
        val mode = reader.control()
        assertEquals("mode", mode.getString("m"))
        assertEquals("cemu", mode.getString("mode"))
    }

    @Test fun confirmedCemuRestoresScreenOnlyBeforePad() = withReceiver(screenOnly = false,
        onMode = { client, mode -> client.restoreModePreferences(mode, "gamepad", cemuScreenOnly = true) }
    ) { _, reader, writer ->
        assertFalse(reader.control().has("screen_only"))
        writer.write("""{"m":"mode","mode":"cemu"}"""); writer.newLine(); writer.flush()
        val restored = reader.control()
        assertEquals("screen_only", restored.getString("m"))
        assertTrue(restored.getBoolean("on"))
        val pad = reader.control()
        assertEquals("pad", pad.getString("m"))
        assertEquals("gamepad", pad.getString("pad"))
    }

    @Test fun disablingFullScreenClearsTheOldReceiverPreference() = withReceiver { client, reader, _ ->
        reader.control()
        client.restoreModePreferences("cemu", "gamepad", cemuScreenOnly = false)
        val restored = reader.control()
        assertEquals("screen_only", restored.getString("m"))
        assertFalse(restored.getBoolean("on"))
        assertEquals("pad", reader.control().getString("m"))
    }

    @Test fun switchModeAndPadRequestsDoNotSendScreenCommands() = withReceiver { client, reader, _ ->
        reader.control()
        client.sendMode("switch", cemuScreenOnly = true)
        client.restoreModePreferences("switch", "joycon_r", cemuScreenOnly = true)
        client.sendText("end")
        assertEquals("mode", reader.control().getString("m"))
        val pad = reader.control()
        assertEquals("pad", pad.getString("m"))
        assertEquals("pro", pad.getString("pad"))
        assertEquals("text", reader.control().getString("m"))
    }
}
