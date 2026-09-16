package dev.pepotech.pepomote.net

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.net.ServerSocket
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

/** RetroArch over the real loopback control channel: capability, pads and hotkeys. */
class RetroArchControlTest {
    private data class Pad(val pad: String, val player: Int?)

    @Test fun retroArchCapabilityPadsAndHotkeys() {
        val ok = LinkedBlockingQueue<ControlClient.Ok>()
        val pads = LinkedBlockingQueue<Pad>()
        val modes = LinkedBlockingQueue<String>()
        ServerSocket(0).use { listener ->
            listener.soTimeout = 4000
            val client = ControlClient("127.0.0.1", listener.localPort, "test-token", "Test", "Test", "wiimote",
                callbacks = object : ControlClient.Callbacks {
                    override fun onOk(value: ControlClient.Ok) { ok.add(value) }
                    override fun onPadChanged(pad: String, player: Int?) { pads.add(Pad(pad, player)) }
                    override fun onModeChanged(mode: String, byPc: Boolean) { modes.add(mode) }
                    override fun onNunchukChanged(own: Boolean) = Unit
                    override fun onError(code: String, msg: String) = Unit
                    override fun onNotice(text: String) = Unit
                    override fun onClosed() = Unit
                })
            try {
                listener.accept().use { socket ->
                    socket.soTimeout = 4000
                    val reader = socket.getInputStream().bufferedReader()
                    val writer = socket.getOutputStream().bufferedWriter()
                    assertTrue(reader.readLine().contains("\"hello\""))
                    fun send(line: String) { writer.write(line); writer.newLine(); writer.flush() }
                    send("""{"m":"ok","session_id":1,"mode":"retroarch","modes":["pointer","dolphin","cemu","switch","retroarch"],"pad":"nes","player":1,"tilt":true}""")
                    val confirmed = ok.poll(3, TimeUnit.SECONDS) ?: error("no ok")
                    assertTrue(confirmed.supportsRetroArch)
                    assertTrue(confirmed.supportsSwitch)
                    assertEquals("nes", confirmed.pad)
                    // ecos de pad: los tres mandos valen; cualquier otro nombre es el RetroPad
                    send("""{"m":"pad","pad":"gun","half":null,"side":null,"player":2}""")
                    assertEquals(Pad("gun", 2), pads.poll(3, TimeUnit.SECONDS))
                    send("""{"m":"pad","pad":"pro","half":null,"side":null,"player":1}""")
                    assertEquals(Pad("retropad", 1), pads.poll(3, TimeUnit.SECONDS))
                    // una tecla rápida viaja tal cual; el eco no molesta
                    client.sendHotkey("save_state", true)
                    val line = JSONObject(reader.readLine())
                    assertEquals("hotkey", line.getString("m"))
                    assertEquals("save_state", line.getString("name"))
                    assertTrue(line.getBoolean("down"))
                    send("""{"m":"hotkey","name":"save_state","down":true,"ok":true}""")
                    client.sendHotkey("rewind", false)
                    val rewind = JSONObject(reader.readLine())
                    assertEquals("rewind", rewind.getString("name"))
                    assertFalse(rewind.getBoolean("down"))
                    // el modo llega como cualquier otro
                    send("""{"m":"mode","mode":"retroarch"}""")
                    assertEquals("retroarch", modes.poll(3, TimeUnit.SECONDS))
                    // un receptor antiguo no anuncia RetroArch
                    send("""{"m":"ok","session_id":2,"mode":"pointer","modes":["pointer","dolphin","cemu","switch"]}""")
                    val legacy = ok.poll(3, TimeUnit.SECONDS) ?: error("no legacy ok")
                    assertFalse(legacy.supportsRetroArch)
                    assertTrue(legacy.supportsSwitch)
                }
            } finally { client.close() }
        }
    }

    @Test fun androidReceiversNeverAnnounceRetroArch() {
        val ok = LinkedBlockingQueue<ControlClient.Ok>()
        ServerSocket(0).use { listener ->
            listener.soTimeout = 4000
            val client = ControlClient("127.0.0.1", listener.localPort, "t", "Test", "Test", "wiimote",
                callbacks = object : ControlClient.Callbacks {
                    override fun onOk(value: ControlClient.Ok) { ok.add(value) }
                    override fun onPadChanged(pad: String, player: Int?) = Unit
                    override fun onModeChanged(mode: String, byPc: Boolean) = Unit
                    override fun onNunchukChanged(own: Boolean) = Unit
                    override fun onError(code: String, msg: String) = Unit
                    override fun onNotice(text: String) = Unit
                    override fun onClosed() = Unit
                })
            try {
                listener.accept().use { socket ->
                    socket.soTimeout = 4000
                    val reader = socket.getInputStream().bufferedReader()
                    val writer = socket.getOutputStream().bufferedWriter()
                    reader.readLine()
                    writer.write("""{"m":"ok","session_id":1,"mode":"dolphin","platform":"android","modes":["dolphin","switch","retroarch"]}"""); writer.newLine(); writer.flush()
                    val confirmed = ok.poll(3, TimeUnit.SECONDS) ?: error("no ok")
                    assertFalse(confirmed.supportsRetroArch)
                    assertTrue(confirmed.supportsSwitch)
                }
            } finally { client.close() }
        }
    }
}
