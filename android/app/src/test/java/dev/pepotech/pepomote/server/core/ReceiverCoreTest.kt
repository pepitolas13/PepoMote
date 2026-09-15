package dev.pepotech.pepomote.server.core

import dev.pepotech.pepomote.net.PmpCodec
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.buffer
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.checkHeader
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.input
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.request
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.u
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketTimeoutException
import java.util.concurrent.CopyOnWriteArrayList

class ReceiverCoreTest {
    @Test fun discoveryAndDsuVersionUseActualBoundPortsAndRestartReleasesAllSockets() {
        val core = ReceiverCore(config())
        repeat(2) {
            core.start()
            val state = core.snapshot()
            assertTrue(state.running)
            assertTrue(state.port > 0)
            assertTrue(state.dsuPort > 0)
            DatagramSocket().use { udp ->
                send(udp, state.port, "PMPDISCOVER1".toByteArray())
                val text = String(receive(udp))
                assertTrue(text.startsWith("PMPHERE1 "))
                val found = JSONObject(text.removePrefix("PMPHERE1 "))
                assertEquals(1, found.getInt("pv"))
                assertEquals(state.port, found.getInt("tcp"))
                assertEquals("Receiver test", found.getString("name"))
                send(udp, state.dsuPort, request(0x100000))
                val version = receive(udp)
                checkHeader(version, 22, 0x100000)
                assertEquals(1001, buffer(version).getShort(20).toInt())
            }
            core.close(); core.close()
            assertFalse(core.snapshot().running)
            ServerSocket(state.port).use { assertTrue(it.isBound) }
            DatagramSocket(state.port).use { assertTrue(it.isBound) }
            DatagramSocket(InetSocketAddress(loopback, state.dsuPort)).use { assertTrue(it.isBound) }
        }
    }

    @Test fun failedDsuBindDoesNotEvictOwnerAndClosesPartialPmpBindings() {
        val pmpPort = ServerSocket(0).use { it.localPort }
        DatagramSocket(InetSocketAddress(loopback, 0)).use { owner ->
            val core = ReceiverCore(config().copy(port = pmpPort, dsuPort = owner.localPort))
            try { core.start(); fail("Occupied DSU port must fail") } catch (_: java.io.IOException) { }
            assertFalse(owner.isClosed)
            assertFalse(core.snapshot().running)
            assertNotNull(core.snapshot().error)
            ServerSocket(pmpPort).use { assertTrue(it.isBound) }
            DatagramSocket(pmpPort).use { assertTrue(it.isBound) }
            core.close()
        }
    }

    @Test fun helloAdvertisesOnlySupportedAndroidCapabilitiesAndFourIndependentPlayers() = withCore { core ->
        val clients = mutableListOf<Client>()
        try {
            for (slot in 0..3) {
                val client = Client(core, hello = hello().put("name", "Phone $slot").put("nunchuk", "own"))
                clients += client
                val ok = client.ok
                assertEquals("ok", ok.getString("m"))
                assertEquals(slot, ok.getInt("slot"))
                assertEquals(slot + 1, ok.getInt("player"))
                assertEquals("android", ok.getString("platform"))
                assertEquals(listOf("dolphin", "switch"), (0 until ok.getJSONArray("modes").length()).map { ok.getJSONArray("modes").getString(it) })
                assertEquals("dolphin", ok.getString("mode"))
                assertEquals("wiimote", ok.getString("pad"))
                assertEquals("own", ok.getString("nunchuk"))
                assertFalse(ok.getBoolean("screen_only"))
                assertFalse(ok.getBoolean("text_input"))
                assertEquals(TOKEN, ok.getString("pair_token"))
            }
            assertEquals(4, clients.map { it.session }.toSet().size)
            assertFalse(clients.any { it.session == 0 })
            assertEquals(listOf(0, 1, 2, 3), core.snapshot().peers.map { it.slot })
            Client(core).use { assertEquals("busy", it.ok.getString("code")) }
        } finally { clients.forEach { it.close() } }
    }

    @Test fun codeAuthenticatesRepeatedlyInEitherFieldAndReturnsCanonicalToken() = withCore { core ->
        Client(core, hello().put("token", "123456")).use { assertEquals(TOKEN, it.ok.getString("pair_token")) }
        await { core.snapshot().peers.isEmpty() }
        Client(core, hello().apply { remove("token"); put("code", "123456") }).use { assertEquals(TOKEN, it.ok.getString("pair_token")) }
    }

    @Test fun invalidAuthVersionsAndSeparateNunchukAreRejectedClearly() = withCore { core ->
        Client(core, hello().put("pv", 2)).use { assertEquals("bad_version", it.ok.getString("code")) }
        Client(core, hello().put("token", "wrong")).use { assertEquals("bad_token", it.ok.getString("code")) }
        Client(core, hello().apply { remove("token"); put("code", "wrong") }).use { assertEquals("bad_code", it.ok.getString("code")) }
        Client(core, hello().put("role", "nunchuk")).use { assertEquals("unsupported_role", it.ok.getString("code")) }
        Client(core).use { assertEquals("ok", it.ok.getString("m")) }
    }

    @Test fun repeatedAuthenticationFailuresAreBoundedPerAddress() = withCore { core ->
        repeat(5) { Client(core, hello().put("token", "bad")).close() }
        Client(core).use { assertEquals("rate_limited", it.ok.getString("code")) }
        assertTrue(core.snapshot().peers.isEmpty())
    }

    @Test fun slotZeroOwnsRemoteModeAndUnsupportedModesEchoAuthoritativeState() = withCore { core ->
        Client(core).use { owner -> Client(core).use { guest ->
            guest.send(JSONObject().put("m", "mode").put("mode", "switch"))
            assertEquals("dolphin", guest.read("mode").getString("mode"))
            assertEquals(ReceiverMode.Dolphin, core.snapshot().mode)
            owner.send(JSONObject().put("m", "mode").put("mode", "switch"))
            assertEquals("switch", owner.read("mode").getString("mode"))
            assertEquals("switch", guest.read("mode").getString("mode"))
            owner.send(JSONObject().put("m", "pad").put("pad", "joycon_r"))
            assertEquals("pro", owner.read("pad").getString("pad"))
            owner.send(JSONObject().put("m", "mode").put("mode", "cemu"))
            assertEquals("switch", owner.read("mode").getString("mode"))
            assertTrue(owner.read("notice").getString("text").isNotBlank())
            core.setMode(ReceiverMode.Dolphin)
            assertEquals("dolphin", owner.read("mode").getString("mode"))
            owner.send(JSONObject().put("m", "screen_only").put("on", true))
            assertFalse(owner.read("screen_only").getBoolean("on"))
        } }
    }

    @Test fun tcpAndUdpPingsEchoTheirBodiesAndSensorlessButtonsReachDsu() = withCore { core ->
        Client(core).use { client -> DatagramSocket().use { udp -> DatagramSocket().use { dsu ->
            client.send(JSONObject().put("m", "ping").put("t", 987654321L))
            assertEquals(987654321L, client.read("pong").getLong("t"))
            val ping = PmpCodec.encodePing(client.session, 99L)
            send(udp, core.snapshot().port, ping)
            assertArrayEquals(PmpCodec.encodePong(client.session, 99L), receive(udp))
            subscribe(core, dsu, 0)
            send(udp, core.snapshot().port, input(client.session, buttons = 1))
            val data = receiveMatching(dsu) { it.size == 100 && u(it[49]) == 255 }
            checkHeader(data, 100, 0x100002)
            assertEquals(255, u(data[49]))
            assertArrayEquals(byteArrayOf(-128, -128, -128, -128), data.copyOfRange(40, 44))
            await { core.snapshot().peers.single().framesReceived == 1L }
        } } }
    }

    @Test fun malformedUnknownOldDuplicateAndWrongAddressInputDoNotChangeState() = withCore { core ->
        Client(core).use { client -> DatagramSocket().use { udp ->
            val port = core.snapshot().port
            send(udp, port, input(client.session, seq = Int.MAX_VALUE, buttons = 1))
            await { core.snapshot().peers.single().framesReceived == 1L }
            send(udp, port, input(client.session, seq = Int.MAX_VALUE, buttons = 2))
            send(udp, port, input(client.session, seq = Int.MAX_VALUE - 1, buttons = 2))
            send(udp, port, input(client.session xor 0x7777, buttons = 2))
            send(udp, port, input(client.session, seq = Int.MIN_VALUE).apply { buffer(this).putFloat(40, Float.NaN) })
            DatagramSocket(InetSocketAddress(InetAddress.getByName("127.0.0.2"), 0)).use { other ->
                send(other, port, input(client.session, seq = Int.MIN_VALUE, buttons = 2))
            }
            Thread.sleep(100)
            assertEquals(1L, core.snapshot().peers.single().framesReceived)
            send(udp, port, input(client.session, seq = Int.MIN_VALUE, buttons = 2))
            await { core.snapshot().peers.single().framesReceived == 2L }
        } }
    }

    @Test fun ownNunchukCanBeToggledAndModeChangeReleasesAllButtonsImmediately() = withCore { core ->
        Client(core).use { client -> DatagramSocket().use { udp -> DatagramSocket().use { dsu ->
            subscribe(core, dsu, 0)
            val buttons = 1 or (1 shl 17) or (1 shl 18)
            send(udp, core.snapshot().port, input(client.session, flags = 2, buttons = buttons))
            val plain = receiveMatching(dsu) { it.size == 100 && u(it[49]) == 255 }
            assertEquals(0, u(plain[52]))
            assertEquals(128, u(plain[40]))
            client.send(JSONObject().put("m", "nunchuk").put("own", true))
            assertTrue(client.read("nunchuk").getBoolean("own"))
            send(udp, core.snapshot().port, input(client.session, seq = 2, flags = 2, buttons = buttons))
            val combined = receiveMatching(dsu) { it.size == 100 && u(it[53]) == 255 }
            assertEquals(255, u(combined[52]))
            assertEquals(228, u(combined[40]))
            core.setMode(ReceiverMode.Eden)
            val neutral = receiveMatching(dsu) { it.size == 100 && u(it[40]) == 127 }
            assertArrayEquals(ByteArray(4), neutral.copyOfRange(36, 40))
            assertArrayEquals(ByteArray(12), neutral.copyOfRange(44, 56))
        } } }
    }

    @Test fun staleInputReleasesButtonsBeforeReportingDisconnected() = withCore { core ->
        Client(core).use { client -> DatagramSocket().use { udp -> DatagramSocket().use { dsu ->
            subscribe(core, dsu, 0)
            send(udp, core.snapshot().port, input(client.session, buttons = 1))
            receiveMatching(dsu) { it.size == 100 && u(it[49]) == 255 }
            val start = System.nanoTime()
            val released = receiveMatching(dsu) { it.size == 100 && u(it[21]) == 2 && u(it[49]) == 0 }
            val elapsedMs = (System.nanoTime() - start) / 1_000_000
            assertTrue("release at $elapsedMs ms", elapsedMs in 150..700)
            assertEquals(0f, buffer(released).getFloat(88), 0f)
            val disconnected = receiveMatching(dsu, 1800) { it.size == 100 && u(it[21]) == 0 }
            assertEquals(0, u(disconnected[31]))
            send(dsu, core.snapshot().dsuPort, request(0x100001, byteArrayOf(1, 0, 0, 0, 0)))
            assertEquals(0, u(receiveMatching(dsu) { it.size == 32 }[21]))
        } } }
    }

    @Test fun disconnectReleasesAndReusedSlotRejectsFormerSession() = withCore { core ->
        val old = Client(core)
        DatagramSocket().use { udp -> DatagramSocket().use { dsu ->
            subscribe(core, dsu, 0)
            send(udp, core.snapshot().port, input(old.session, buttons = 1))
            receiveMatching(dsu) { it.size == 100 && u(it[49]) == 255 }
            old.close()
            val release = receiveMatching(dsu) { it.size == 100 && u(it[21]) == 0 }
            assertEquals(0, u(release[49]))
            await { core.snapshot().peers.isEmpty() }
            Client(core).use { replacement ->
                assertEquals(0, replacement.ok.getInt("slot"))
                assertNotEquals(old.session, replacement.session)
                send(udp, core.snapshot().port, input(old.session, seq = 2, buttons = 1))
                Thread.sleep(80)
                assertEquals(0L, core.snapshot().peers.single().framesReceived)
            }
        } }
    }

    @Test fun dsuSubscriptionsAreIsolatedBySlotAndCanAccumulatePerSocket() = withCore { core ->
        Client(core).use { one -> Client(core).use { two ->
            DatagramSocket().use { udp -> DatagramSocket().use { first -> DatagramSocket().use { second ->
                subscribe(core, first, 0)
                send(second, core.snapshot().dsuPort, request(0x100002, byteArrayOf(2, 0, 0x50, 0x4d, 0x50, 0x31, 0, 1)))
                receiveMatching(second) { it.size == 100 && u(it[20]) == 1 }
                send(udp, core.snapshot().port, input(one.session, buttons = 1))
                send(udp, core.snapshot().port, input(two.session, buttons = 2))
                assertEquals(0, u(receiveMatching(first) { it.size == 100 && u(it[49]) == 255 }[20]))
                assertEquals(1, u(receiveMatching(second) { it.size == 100 && u(it[50]) == 255 }[20]))
                subscribe(core, first, 1)
                send(udp, core.snapshot().port, input(one.session, seq = 2, buttons = 1))
                assertEquals(0, u(receiveMatching(first) { it.size == 100 && u(it[49]) == 255 }[20]))
                assertEquals(2, core.snapshot().dsuClients)
            } } }
        } }
    }

    @Test fun allFourPlayersStreamTheirOwnButtonsAndMacAddresses() = withCore { core ->
        val controllers = (0..3).map { Client(core, hello().put("nunchuk", "own")) }
        val emulators = (0..3).map { DatagramSocket() }
        try {
            emulators.forEachIndexed { slot, socket -> subscribe(core, socket, slot) }
            DatagramSocket().use { udp ->
                controllers.forEachIndexed { slot, client ->
                    send(udp, core.snapshot().port, input(client.session, flags = 2, buttons = 1 shl slot))
                }
                val analogOffsets = listOf(49, 50, 47, 45)
                emulators.forEachIndexed { slot, socket ->
                    val wire = receiveMatching(socket) { it.size == 100 && u(it[analogOffsets[slot]]) == 255 }
                    assertEquals(slot, u(wire[20]))
                    assertEquals(slot, u(wire[29]))
                    assertEquals(228, u(wire[40]))
                    assertEquals(1, wire.copyOfRange(44, 52).count { u(it) == 255 })
                }
            }
        } finally { controllers.forEach { it.close() }; emulators.forEach { it.close() } }
    }

    @Test fun pairProbeReturnsTokenWithoutConsumingAPlayer() = withCore { core ->
        Client(core, hello().put("probe", true)).use {
            assertEquals(TOKEN, it.ok.getString("token"))
            assertEquals(TOKEN, it.ok.getString("pair_token"))
        }
        assertTrue(core.snapshot().peers.isEmpty())
        Client(core).use { assertEquals(0, it.ok.getInt("slot")) }
    }

    @Test fun restartWithActiveControllersDoesNotRetainPeersSubscriptionsOrThreads() {
        val before = receiverThreads()
        val core = ReceiverCore(config())
        repeat(4) {
            core.start()
            val client = Client(core)
            DatagramSocket().use { dsu -> subscribe(core, dsu, 0) }
            core.close()
            client.close()
            assertTrue(core.snapshot().peers.isEmpty())
            assertEquals(0, core.snapshot().dsuClients)
            await { receiverThreads() == before }
        }
        core.close()
    }

    @Test fun idleControlSessionExpiresEvenWhenServerPingsAreNotAnswered() = withCore { core ->
        Client(core).use {
            assertEquals(1, core.snapshot().peers.size)
            await(6200) { core.snapshot().peers.isEmpty() }
        }
    }

    @Test fun inputBurstHasBoundedOutputAndRoutineCallbacksAreThrottled() {
        val callbacks = CopyOnWriteArrayList<ReceiverSnapshot>()
        ReceiverCore(config(), callbacks::add).use { core ->
            core.start()
            Client(core).use { client -> DatagramSocket().use { udp -> DatagramSocket().use { dsu ->
                subscribe(core, dsu, 0)
                val start = System.nanoTime()
                repeat(400) { seq -> send(udp, core.snapshot().port, input(client.session, seq = seq, buttons = 1)) }
                val expectedMaximum = ((System.nanoTime() - start) / 4_000_000L).toInt() + 16
                var outputs = 0
                while (true) {
                    try { receive(dsu, 70); outputs++ } catch (_: SocketTimeoutException) { break }
                }
                assertTrue("$outputs datagrams for a 400-frame burst", outputs <= expectedMaximum)
                assertTrue("${callbacks.size} callbacks for a burst", callbacks.size <= 4)
                assertTrue(core.snapshot().peers.single().framesReceived > 0)
            } } }
        }
    }

    @Test fun invalidDsuRequestsNeverSubscribeAndValidSubscriptionsExpire() = withCore { core ->
        DatagramSocket().use { dsu ->
            val bad = request(0x100002, ByteArray(8)).apply { this[8] = (this[8] + 1).toByte() }
            send(dsu, core.snapshot().dsuPort, bad)
            Thread.sleep(60)
            assertEquals(0, core.snapshot().dsuClients)
            subscribe(core, dsu, 0)
            await { core.snapshot().dsuClients == 1 }
            await(4000) { core.snapshot().dsuClients == 0 }
        }
    }

    @Test fun recenterPulseIsPerSlotAndEndsWithoutAnotherInputPacket() = withCore { core ->
        Client(core).use { client -> Client(core).use { other -> DatagramSocket().use { udp -> DatagramSocket().use { dsu -> DatagramSocket().use { second ->
            subscribe(core, dsu, 0)
            subscribe(core, second, 1)
            send(udp, core.snapshot().port, input(client.session))
            send(udp, core.snapshot().port, input(other.session))
            receiveMatching(dsu) { it.size == 100 && u(it[21]) == 2 }
            receiveMatching(second) { it.size == 100 && u(it[21]) == 2 }
            Thread.sleep(6)
            send(udp, core.snapshot().port, input(client.session, seq = 2, recenter = 1))
            send(udp, core.snapshot().port, input(other.session, seq = 2))
            receiveMatching(dsu) { it.size == 100 && u(it[39]) == 255 }
            assertEquals(0, u(receiveMatching(second) { it.size == 100 && u(it[21]) == 2 }[39]))
            val release = receiveMatching(dsu) { it.size == 100 && u(it[21]) == 2 && u(it[39]) == 0 }
            // The pulse ends before stale-input neutralization, with the last motion intact.
            assertEquals(57.29578f, buffer(release).getFloat(88), 0.0001f)
        } } } } }
    }

    @Test fun oversizedTcpLineClosesConnectionWithoutConsumingAPlayer() = withCore { core ->
        Socket(loopback, core.snapshot().port).use { socket ->
            socket.soTimeout = 2000
            socket.getOutputStream().write(ByteArray(5000) { 'a'.code.toByte() })
            val reader = BufferedReader(InputStreamReader(socket.getInputStream()))
            val line = reader.readLine()
            if (line != null) assertEquals("err", JSONObject(line).getString("m"))
        }
        Client(core).use { assertEquals(0, it.ok.getInt("slot")) }
    }

    @Test fun callbacksCanReadStateAndCloseReceiverWithoutDeadlock() {
        val states = CopyOnWriteArrayList<ReceiverSnapshot>()
        lateinit var core: ReceiverCore
        core = ReceiverCore(config()) { state ->
            states += state
            assertEquals(state.running, core.snapshot().running)
            if (state.peers.isNotEmpty()) core.close()
        }
        core.start()
        try {
            runCatching { Client(core).close() }
            await { !core.snapshot().running }
            assertTrue(states.any { it.running })
            assertTrue(states.any { !it.running })
        } finally { core.close() }
    }

    private class Client(core: ReceiverCore, hello: JSONObject = hello()) : AutoCloseable {
        private val socket = Socket(loopback, core.snapshot().port).apply { soTimeout = 2200; tcpNoDelay = true }
        private val reader = BufferedReader(InputStreamReader(socket.getInputStream(), Charsets.UTF_8))
        val ok: JSONObject
        val session: Int get() = ok.optLong("session_id").toInt()
        init { send(hello); ok = read() }
        fun send(json: JSONObject) { socket.getOutputStream().write((json.toString() + "\n").toByteArray(Charsets.UTF_8)); socket.getOutputStream().flush() }
        fun read(type: String? = null): JSONObject {
            val deadline = System.nanoTime() + 2_000_000_000
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

    companion object {
        private const val TOKEN = "00112233445566778899aabbccddeeff"
        private val loopback = InetAddress.getByName("127.0.0.1")
        private fun config() = ReceiverConfig("Receiver test", TOKEN, "123456", 0, 0)
        private fun hello() = JSONObject().put("m", "hello").put("pv", 1).put("token", TOKEN).put("name", "Test phone")
        private fun withCore(block: (ReceiverCore) -> Unit) { ReceiverCore(config()).use { it.start(); block(it) } }
        private fun send(socket: DatagramSocket, port: Int, bytes: ByteArray) = socket.send(DatagramPacket(bytes, bytes.size, loopback, port))
        private fun receive(socket: DatagramSocket, timeoutMs: Int = 1800): ByteArray {
            socket.soTimeout = timeoutMs
            val packet = DatagramPacket(ByteArray(2048), 2048)
            socket.receive(packet)
            return packet.data.copyOf(packet.length)
        }
        private fun receiveMatching(socket: DatagramSocket, timeoutMs: Int = 1800, predicate: (ByteArray) -> Boolean): ByteArray {
            val deadline = System.nanoTime() + timeoutMs * 1_000_000L
            while (System.nanoTime() < deadline) {
                val bytes = receive(socket, ((deadline - System.nanoTime()) / 1_000_000).coerceAtLeast(1).toInt())
                if (predicate(bytes)) return bytes
            }
            throw SocketTimeoutException("Expected DSU packet not received")
        }
        private fun subscribe(core: ReceiverCore, socket: DatagramSocket, slot: Int) {
            send(socket, core.snapshot().dsuPort, request(0x100002, byteArrayOf(1, slot.toByte(), 0, 0, 0, 0, 0, 0)))
            receiveMatching(socket) { it.size == 100 && u(it[20]) == slot }
        }
        private fun await(timeoutMs: Long = 1800, predicate: () -> Boolean) {
            val deadline = System.nanoTime() + timeoutMs * 1_000_000
            while (System.nanoTime() < deadline) { if (predicate()) return; Thread.sleep(10) }
            assertTrue("Timed out waiting for receiver state", predicate())
        }
        private fun receiverThreads() = Thread.getAllStackTraces().keys
            .filter { it.isAlive && it.name.startsWith("pepomote-receiver-") }.map { it.id }.toSet()
    }
}
