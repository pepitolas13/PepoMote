package dev.pepotech.pepomote.net

import org.junit.Assert.*
import org.junit.Test
import java.net.ServerSocket
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

/** Real loopback control channel, including JSON parsing and line framing. */
class ControlClientTest {
    private data class Pad(val pad: String, val player: Int?)

    @Test fun fourDigitDesktopCodeUsesCodeFieldAndRemembersThePermanentToken() {
        val replies = LinkedBlockingQueue<ControlClient.Ok>()
        ServerSocket(0).use { listener ->
            listener.soTimeout = 4000
            val client = ControlClient("127.0.0.1", listener.localPort, "0042", "Phone", "Phone", "wiimote",
                callbacks = object : ControlClient.Callbacks {
                    override fun onOk(ok: ControlClient.Ok) { replies.add(ok) }
                    override fun onError(code: String, msg: String) = Unit
                    override fun onModeChanged(mode: String, byPc: Boolean) = Unit
                    override fun onPadChanged(pad: String, player: Int?) = Unit
                    override fun onNunchukChanged(own: Boolean) = Unit
                    override fun onNotice(text: String) = Unit
                    override fun onClosed() = Unit
                })
            try {
                listener.accept().use { socket ->
                    socket.soTimeout = 4000
                    val hello = org.json.JSONObject(socket.getInputStream().bufferedReader().readLine())
                    assertEquals("0042", hello.optString("code"))
                    // Android 1.8 accepted its short code through token; retain that compatibility.
                    assertEquals("0042", hello.optString("token"))
                    socket.getOutputStream().write(("""{"m":"ok","session_id":17,"mode":"pointer","token":"permanent-desktop-token"}""" + "\n").toByteArray())
                    val ok = replies.poll(3, TimeUnit.SECONDS) ?: error("No desktop confirmation")
                    assertEquals("permanent-desktop-token", ok.pairToken)
                    assertEquals(ReceiverCapabilities.DESKTOP, ok.platform)
                }
            } finally { client.close() }
        }
    }

    @Test fun staleSwitchAssignmentsAreNormalizedToProAndIgnored() {
        val ok = LinkedBlockingQueue<ControlClient.Ok>()
        val pads = LinkedBlockingQueue<Pad>()
        ServerSocket(0).use { listener ->
            listener.soTimeout = 4000
            val client = ControlClient("127.0.0.1", listener.localPort, "test-token", "Test", "Test", "wiimote",
                callbacks = object : ControlClient.Callbacks {
                    override fun onOk(value: ControlClient.Ok) { ok.add(value) }
                    override fun onPadChanged(pad: String, player: Int?) { pads.add(Pad(pad, player)) }
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
                    assertTrue(reader.readLine().contains("\"hello\""))
                    fun send(line: String) { writer.write(line); writer.newLine(); writer.flush() }
                    send("""{"m":"ok","session_id":1,"mode":"switch","modes":["pointer","cemu","switch"],"pad":"joycons","half":"left","side":null,"player":1,"tilt":true}""")
                    val confirmed = ok.poll(3, TimeUnit.SECONDS) ?: error("no ok")
                    assertTrue(confirmed.supportsSwitch)
                    assertTrue(confirmed.supportsCemu)
                    assertTrue("el receptor anuncia la inclinación", confirmed.supportsTilt)
                    assertEquals("pro", confirmed.pad)
                    send("""{"m":"pad","pad":"joycon_r","half":"right","side":null,"player":1}""")
                    assertEquals(Pad("pro", 1), pads.poll(3, TimeUnit.SECONDS))
                    send("""{"m":"pad","pad":"joycon_side","half":null,"side":"left","player":2}""")
                    assertEquals(Pad("pro", 2), pads.poll(3, TimeUnit.SECONDS))
                    send("""{"m":"pad","pad":"pro","half":"unknown","side":"unknown"}""")
                    assertEquals(Pad("pro", null), pads.poll(3, TimeUnit.SECONDS))
                    send("""{"m":"ok","session_id":2,"mode":"pointer"}""")
                    val legacy = ok.poll(3, TimeUnit.SECONDS) ?: error("no legacy ok")
                    assertFalse(legacy.supportsSwitch)
                    assertFalse(legacy.supportsCemu)
                    assertFalse("un receptor sin «tilt» en el ok nunca recibe el bit4", legacy.supportsTilt)
                    client.restoreModePreferences("switch", "joycon_side", false)
                    var outgoing = reader.readLine()
                    while (outgoing.contains("\"ping\"")) outgoing = reader.readLine()
                    val request = org.json.JSONObject(outgoing)
                    assertEquals("pro", request.getString("pad"))
                    assertFalse(request.has("half"))
                    assertFalse(request.has("side"))
                }
            } finally { client.close() }
        }
    }
}
