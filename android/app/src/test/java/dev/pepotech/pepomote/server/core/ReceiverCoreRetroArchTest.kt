package dev.pepotech.pepomote.server.core

import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.input
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.request
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.u
import dev.pepotech.pepomote.server.retro.Console
import dev.pepotech.pepomote.server.retro.FakeRetroArch
import dev.pepotech.pepomote.server.retro.FakeRetroArch.Companion.await
import dev.pepotech.pepomote.server.retro.RetroGameInfo
import dev.pepotech.pepomote.server.retro.RetroPad
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.Socket

/** El receptor Android en modo RetroArch contra un RetroArch falso en UDP. */
class ReceiverCoreRetroArchTest {
    private fun bit(b: Int) = 1 shl b

    @Test fun retroModeEchoesPadsLayoutsHotkeysAndGameAndRejectsTheGun() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            ra.mute = true // sin RetroArch delante el modo lo decide el jugador 1
            ReceiverCore(config(ports)).use { core ->
                core.start()
                Client(core, hello().put("pad", "nes").put("layout", "md")).use { owner ->
                    val modes = (0 until owner.ok.getJSONArray("modes").length()).map { owner.ok.getJSONArray("modes").getString(it) }
                    assertEquals(listOf("dolphin", "switch", "retroarch"), modes)
                    assertEquals("wiimote", owner.ok.getString("pad"))
                    val game = owner.read("game")
                    assertTrue("sin juego: console null", game.isNull("console"))
                    assertEquals("", game.getString("title"))
                    assertTrue("hotkey fuera de RetroArch: ok false", !owner.hotkey("save_state"))
                    owner.send(JSONObject().put("m", "mode").put("mode", "retroarch"))
                    assertEquals("retroarch", owner.read("mode").getString("mode"))
                    val pad = owner.read("pad")
                    assertEquals("el pad pedido en el hello vale en RetroArch", "nes", pad.getString("pad"))
                    assertEquals("md", pad.getString("layout"))
                    assertEquals(ReceiverMode.RetroArch, core.snapshot().mode)
                    owner.send(JSONObject().put("m", "pad").put("pad", "retropad").put("layout", "n64"))
                    val echo = owner.read("pad")
                    assertEquals("retropad", echo.getString("pad"))
                    assertEquals("n64", echo.getString("layout"))
                    assertEquals("retropad", core.snapshot().peers.single().retroPad)
                    assertEquals("n64", core.snapshot().peers.single().retroLayout)
                    owner.send(JSONObject().put("m", "pad").put("pad", "retropad").put("layout", "bogus"))
                    assertTrue(owner.read("pad").isNull("layout"))
                    owner.send(JSONObject().put("m", "pad").put("pad", "gun"))
                    assertTrue(owner.read("notice").getString("text").isNotBlank())
                    assertEquals("la pistola no existe aquí: sigue el mando anterior", "retropad", owner.read("pad").getString("pad"))
                    assertTrue(owner.hotkey("save_state"))
                    assertFalse(owner.hotkey("inventada"))
                    core.setGame(RetroGameInfo(Console.Md, "Mega Drive", "Genesis Plus GX", "Cave Story", "/roms/cave.zip"))
                    val announced = owner.read("game")
                    assertEquals("md", announced.getString("console"))
                    assertEquals("Cave Story", announced.getString("title"))
                    assertEquals("Mega Drive", core.snapshot().retro.game!!.system)
                    Client(core).use { late ->
                        assertEquals("retroarch", late.ok.getString("mode"))
                        assertEquals("retropad", late.ok.getString("pad"))
                        assertEquals("un móvil que llega después recibe el juego tras el ok", "md", late.read("game").getString("console"))
                    }
                    core.setGame(null)
                    assertTrue(owner.read("game").isNull("console"))
                    owner.send(JSONObject().put("m", "mode").put("mode", "dolphin"))
                    assertEquals("dolphin", owner.read("mode").getString("mode"))
                    assertEquals("wiimote", owner.read("pad").getString("pad"))
                }
            }
        }
    }

    @Test fun inputReachesTheFakeRetroArchOnlyInRetroModeAndIsReleasedWhenStaleOrGone() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            ReceiverCore(config(ports)).use { core ->
                core.start()
                await(message = "el enlace ve al RetroArch falso") { core.snapshot().retro.reachable }
                await(2000, "RetroArch respondiendo pone el modo RetroArch solo") { core.snapshot().mode == ReceiverMode.RetroArch }
                Client(core).use { client -> DatagramSocket().use { udp -> DatagramSocket().use { dsu ->
                    assertEquals("retroarch", client.ok.getString("mode"))
                    subscribe(core, dsu)
                    send(udp, core.snapshot().port, input(client.session, seq = 1, buttons = 1 or (1 shl 6)))
                    await(message = "A y + llegan al RetroPad del jugador 1") { ra.buttons(0) == (bit(RetroPad.A) or bit(RetroPad.START)) }
                    // El DSU solo recibe un mando neutro mientras RetroArch tiene los mandos
                    val neutral = receiveMatching(dsu) { it.size == 100 && u(it[21]) == 2 }
                    assertEquals(0, u(neutral[49]))
                    assertEquals(0, u(neutral[37]))
                    await(1500, "sin paquetes se suelta todo") { ra.buttons(0) == 0 }
                    send(udp, core.snapshot().port, input(client.session, seq = 2, buttons = 1))
                    await(message = "vuelve a pulsar") { ra.buttons(0) == bit(RetroPad.A) }
                    client.close()
                    await(message = "al irse el móvil se suelta") { ra.buttons(0) == 0 }
                    assertEquals("un datagrama por fotograma, nunca en cola", 0, ra.doubles)
                } } }
                // Home abre el menú una vez; una tecla rápida por el canal de control llega como comando
                Client(core).use { client -> DatagramSocket().use { udp ->
                    send(udp, core.snapshot().port, input(client.session, seq = 1, buttons = 1 shl 8))
                    await(message = "MENU_TOGGLE") { ra.commands().any { it.second == "MENU_TOGGLE" } }
                    assertTrue(client.hotkey("screenshot"))
                    await(message = "SCREENSHOT") { ra.commands().any { it.second == "SCREENSHOT" } }
                    assertEquals(1, ra.commands().count { it.second == "MENU_TOGGLE" })
                } }
            }
        }
    }

    @Test fun retroArchAnsweringSwitchesTheModeAndANewEmulatorSwitchesBack() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            ra.mute = true
            ReceiverCore(config(ports)).use { core ->
                core.start()
                core.setMode(ReceiverMode.Eden)
                Client(core).use { client ->
                    assertEquals("switch", client.ok.getString("mode"))
                    ra.mute = false
                    val mode = client.read("mode")
                    assertEquals("retroarch", mode.getString("mode"))
                    assertEquals("lo decidió el receptor", "pc", mode.getString("by"))
                    assertEquals("retropad", client.read("pad").getString("pad"))
                    assertTrue(client.read("notice").getString("text").isNotBlank())
                    await(message = "responde") { core.snapshot().retro.reachable }
                    // El jugador 1 puede volver a Eden con RetroArch delante: no se le lleva la contraria
                    client.send(JSONObject().put("m", "mode").put("mode", "switch"))
                    assertEquals("switch", client.read("mode").getString("mode"))
                    Thread.sleep(700)
                    assertEquals(ReceiverMode.Eden, core.snapshot().mode)
                    // RetroArch se va (detrás), se abre otro emulador: no cambia nada porque ya no estamos en RetroArch
                    ra.mute = true
                    await(3000, "se pierde") { !core.snapshot().retro.reachable }
                    ra.mute = false
                    assertEquals("retroarch", client.read("mode").getString("mode"))
                    client.read("pad")
                    ra.mute = true
                    await(3000, "se pierde otra vez") { !core.snapshot().retro.reachable }
                    Thread.sleep(2100)
                    DatagramSocket().use { dsu ->
                        subscribe(core, dsu)
                        val back = client.read("mode")
                        assertEquals("un emulador pide los mandos: vuelve al último modo", "switch", back.getString("mode"))
                        assertEquals("pc", back.getString("by"))
                        assertEquals(ReceiverMode.Eden, core.snapshot().mode)
                    }
                }
            }
        }
    }

    @Test fun unEmuladorQueSeSuscribeAntesDelTiempoDeEsperaPuedeVolverAlRenovar() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            ra.mute = true
            ReceiverCore(config(ports)).use { core ->
                core.start()
                core.setMode(ReceiverMode.Eden)
                ra.mute = false
                await { core.snapshot().mode == ReceiverMode.RetroArch }
                ra.mute = true
                DatagramSocket().use { dsu ->
                    // La primera petición llega antes de detectar que RetroArch ha pasado al fondo.
                    subscribe(core, dsu)
                    assertEquals(ReceiverMode.RetroArch, core.snapshot().mode)
                    await(5000, "las renovaciones deben completar el cambio pendiente a Eden") {
                        subscribe(core, dsu)
                        Thread.sleep(50)
                        core.snapshot().mode == ReceiverMode.Eden
                    }
                }
            }
        }
    }

    @Test fun perderLosPaquetesDeEntradaSueltaTambienElAvanceRapido() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            ReceiverCore(config(ports)).use { core ->
                core.start()
                await { core.snapshot().mode == ReceiverMode.RetroArch }
                Client(core).use { client -> DatagramSocket().use { udp ->
                    send(udp, core.snapshot().port, input(client.session, buttons = 1 shl 28))
                    await { ra.commands().any { it.second == "FAST_FORWARD_HOLD" } }
                    Thread.sleep(450)
                    val count = ra.commands().count { it.second == "FAST_FORWARD_HOLD" }
                    Thread.sleep(150)
                    assertEquals("sin INPUT durante 250 ms se sueltan también las teclas mantenidas", count,
                        ra.commands().count { it.second == "FAST_FORWARD_HOLD" })
                    assertEquals("la conexión de control sigue viva", 1, core.snapshot().peers.size)
                } }
            }
        }
    }

    private class Client(core: ReceiverCore, hello: JSONObject = hello()) : AutoCloseable {
        private val socket = Socket(loopback, core.snapshot().port).apply { soTimeout = 2500; tcpNoDelay = true }
        private val reader = BufferedReader(InputStreamReader(socket.getInputStream(), Charsets.UTF_8))
        val ok: JSONObject
        val session: Int get() = ok.optLong("session_id").toInt()
        init { send(hello); ok = read() }
        fun send(json: JSONObject) { socket.getOutputStream().write((json.toString() + "\n").toByteArray(Charsets.UTF_8)); socket.getOutputStream().flush() }
        fun hotkey(name: String): Boolean {
            send(JSONObject().put("m", "hotkey").put("name", name).put("down", true))
            return read("hotkey").getBoolean("ok")
        }
        fun read(type: String? = null): JSONObject {
            val deadline = System.nanoTime() + 4_000_000_000
            while (System.nanoTime() < deadline) {
                val line = reader.readLine() ?: throw AssertionError("Control socket closed")
                val json = JSONObject(line)
                if (json.optString("m") == "ping") { send(JSONObject().put("m", "pong").put("t", json.opt("t"))); continue }
                if (type == null || json.optString("m") == type) return json
            }
            throw AssertionError("No $type response")
        }
        override fun close() = socket.close()
    }

    private companion object {
        const val TOKEN = "00112233445566778899aabbccddeeff"
        val loopback: InetAddress = InetAddress.getByName("127.0.0.1")
        fun config(ports: dev.pepotech.pepomote.server.retro.RetroPorts) = ReceiverConfig("Receiver test", TOKEN, "123456", 0, 0, retroPorts = ports)
        fun hello() = JSONObject().put("m", "hello").put("pv", 1).put("token", TOKEN).put("name", "Test phone")
        fun send(socket: DatagramSocket, port: Int, bytes: ByteArray) = socket.send(DatagramPacket(bytes, bytes.size, loopback, port))
        fun subscribe(core: ReceiverCore, socket: DatagramSocket) {
            send(socket, core.snapshot().dsuPort, request(0x100002, byteArrayOf(1, 0, 0, 0, 0, 0, 0, 0)))
            receiveMatching(socket) { it.size == 100 }
        }
        fun receiveMatching(socket: DatagramSocket, timeoutMs: Long = 3000, match: (ByteArray) -> Boolean): ByteArray {
            val deadline = System.nanoTime() + timeoutMs * 1_000_000
            while (System.nanoTime() < deadline) {
                socket.soTimeout = ((deadline - System.nanoTime()) / 1_000_000).coerceAtLeast(1).toInt()
                val packet = DatagramPacket(ByteArray(256), 256)
                socket.receive(packet)
                val bytes = packet.data.copyOf(packet.length)
                if (match(bytes)) return bytes
            }
            throw AssertionError("Expected DSU datagram not received")
        }
    }
}
