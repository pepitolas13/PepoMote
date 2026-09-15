package dev.pepotech.pepomote.server

import android.content.Context
import android.content.Intent
import android.hardware.Sensor
import android.hardware.SensorManager
import android.os.Looper
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.net.Pairing
import dev.pepotech.pepomote.net.ReceiverCapabilities
import dev.pepotech.pepomote.sensor.Frame
import dev.pepotech.pepomote.sensor.MotionEngine
import dev.pepotech.pepomote.sensor.SenderKind
import dev.pepotech.pepomote.server.core.ReceiverConfig
import dev.pepotech.pepomote.server.core.ReceiverCore
import dev.pepotech.pepomote.server.core.ReceiverMode
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.buffer
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.checkHeader
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.request
import dev.pepotech.pepomote.server.core.ReceiverProtocolTest.Companion.u
import dev.pepotech.pepomote.service.LinkForegroundService
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.PadIntent
import dev.pepotech.pepomote.service.Route
import dev.pepotech.pepomote.service.ScreenLink
import dev.pepotech.pepomote.service.UiLink
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import org.robolectric.shadows.ShadowLooper
import org.robolectric.shadows.ShadowSystemClock
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.SocketTimeoutException
import java.time.Duration

/** Real client service, TCP authentication, sensorless PMP sender, receiver, and DSU UDP. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
@LooperMode(LooperMode.Mode.PAUSED)
class AndroidServerClientE2eTest {
    private val context: Context get() = RuntimeEnvironment.getApplication()
    private val clients = mutableListOf<ClientSession>()
    private val savedPc = Pairing("192.168.10.9", 26761, "existing-desktop-token", "My saved PC")

    private data class ClientSession(
        val service: ServiceController<LinkForegroundService>,
        val engine: MotionEngine,
        val sensorLooper: ShadowLooper,
    )

    @Before fun prepareSensorlessClientAndExistingPc() {
        val sensors = shadowOf(context.getSystemService(Context.SENSOR_SERVICE) as SensorManager)
        sensors.getSensorList(Sensor.TYPE_ALL).toList().forEach(sensors::removeSensor)
        assertTrue(sensors.getSensorList(Sensor.TYPE_ALL).isEmpty())
        context.getSharedPreferences("pairing", Context.MODE_PRIVATE).edit().clear().commit()
        PairStore.save(context, savedPc)
        AppPrefs.setSoundsEnabled(context, false)
        AppPrefs.setOwnNunchuk(context, false)
        // Existing Wii U choices must survive using the Android receiver.
        AppPrefs.setPad(context, LinkState.MODE_CEMU, "wiimote")
        AppPrefs.setGamePadFullScreen(context, true)
        LinkState.pendingMode = null
        LinkState.clearIntent()
        LinkState.publish(UiLink.Disconnected)
        ButtonState.reset()
        ScreenLink.unbind()
        LinkForegroundService.setAppVisible(true)
    }

    @After fun stopEveryClient() {
        clients.toList().forEach(::stopClient)
        ButtonState.reset()
        LinkState.pendingMode = null
        LinkState.clearIntent()
        LinkForegroundService.setAppVisible(false)
    }

    @Test fun temporaryCodePairsSensorlessWiiAndAuthoritativeEdenControlsReachDsu() {
        ReceiverCore(config("0042")).use { receiver ->
            receiver.start()
            PairStore.save(context, temporaryPair(receiver, "0042"))
            // A pending desktop request cannot restore Cemu on the Android server.
            LinkState.requestMode(LinkState.MODE_CEMU)
            val client = startClient()
            val connected = LinkState.flow.value as UiLink.Connected
            assertEquals(ReceiverCapabilities.ANDROID, connected.platform)
            assertEquals(LinkState.MODE_DOLPHIN, connected.mode)
            assertEquals("wiimote", connected.pad)
            assertEquals(0, connected.slot)
            assertFalse(connected.supportsCemu)
            assertTrue(connected.supportsSwitch)
            assertFalse(connected.textInput)
            assertEquals(false, connected.screenOnly)
            await("Pending desktop mode is discarded") { LinkState.pendingMode == null && LinkState.intent.value == PadIntent.None }
            assertEquals(TOKEN, PairStore.load(context)?.token)
            assertEquals(ReceiverCapabilities.ANDROID, PairStore.load(context)?.platform)
            assertFalse(PairStore.all(context).any { it.token == "0042" })
            assertNull(ScreenLink.client.value)

            DatagramSocket().use { dsu ->
                subscribe(receiver, dsu)
                ButtonState.set(ButtonState.A, true)
                advanceInput(client, 20)
                val wii = receiveDsu(dsu) { u(it[49]) == 255 }
                checkHeader(wii, 100, 0x100002)
                assertEquals(0x40, u(wii[37]))
                assertArrayEquals(byteArrayOf(0, -1, 0, 0), wii.copyOfRange(48, 52))
                assertArrayEquals(byteArrayOf(-128, -128, -128, -128), wii.copyOfRange(40, 44))
                assertEquals(0f, buffer(wii).getFloat(88), 0f)
                assertTrue(receiver.snapshot().peers.single().framesReceived > 0)

                // The local host changes mode; the client must follow its authoritative echo.
                receiver.setMode(ReceiverMode.Eden)
                await("Client follows Eden mode") {
                    (LinkState.flow.value as? UiLink.Connected)?.let { it.mode == LinkState.MODE_SWITCH && it.pad == "pro" } == true
                }
                assertTrue(Route.isGamePad(LinkState.flow.value))
                val neutral = receiveDsu(dsu) { u(it[40]) == 127 && u(it[37]) == 0 }
                assertArrayEquals(ByteArray(4), neutral.copyOfRange(36, 40))

                // This is the format/rotation change performed by GamePadScreen after confirmation.
                client.engine.kind = SenderKind.SWITCH
                client.engine.rotation = Frame.ROTATION_90
                ButtonState.reset()
                ButtonState.set(ButtonState.A or ButtonState.ZL or ButtonState.HOME or ButtonState.SCREEN or ButtonState.STICK_R or ButtonState.MIC, true)
                ButtonState.setStick(100, -80)
                ButtonState.setStick2(-64, 32)
                ButtonState.setTouch(12000, 34000, true)
                advanceInput(client, 20)
                val eden = receiveDsu(dsu) { u(it[37]) == 0x21 && u(it[40]) == 227 }
                checkHeader(eden, 100, 0x100002)
                assertEquals(4, u(eden[36])) // Right stick click.
                assertEquals(255, u(eden[50])) // Switch A is Circle.
                assertEquals(0, u(eden[49]))
                assertEquals(255, u(eden[55])) // ZL includes its analog trigger.
                assertEquals(255, u(eden[38])) // Home.
                assertEquals(255, u(eden[39])) // Capture.
                assertArrayEquals(byteArrayOf(227.toByte(), 47, 63, 159.toByte()), eden.copyOfRange(40, 44))
                assertArrayEquals(ByteArray(12), eden.copyOfRange(56, 68))
                assertEquals(0f, buffer(eden).getFloat(88), 0f)
                assertNull(ScreenLink.client.value)

                stopClient(client)
                val released = receiveDsu(dsu) { u(it[21]) == 0 }
                assertEquals(0, u(released[31]))
                assertArrayEquals(ByteArray(4), released.copyOfRange(36, 40))
                assertArrayEquals(ByteArray(12), released.copyOfRange(44, 56))
                await("Receiver frees the player after service teardown") { receiver.snapshot().peers.isEmpty() }
            }
            assertNull(LinkState.motion)
            assertEquals(UiLink.Disconnected, LinkState.flow.value)
            assertSavedPcAndPreferencesUnchanged()
        }
    }

    @Test fun canonicalPairingReconnectsAfterReceiverRestartChangesItsDisplayedCode() {
        var receiverPort = 0
        ReceiverCore(config("123456")).use { receiver ->
            receiver.start()
            receiverPort = receiver.snapshot().port
            PairStore.save(context, temporaryPair(receiver, "123456"))
            val client = startClient()
            assertEquals(TOKEN, PairStore.load(context)?.token)
            stopClient(client)
            await("First authenticated session is released") { receiver.snapshot().peers.isEmpty() }
        }
        ReceiverCore(config("654321").copy(port = receiverPort)).use { restarted ->
            restarted.setMode(ReceiverMode.Eden)
            restarted.start()
            // PairStore still contains the same permanent token, not either displayed code.
            val client = startClient()
            val connected = LinkState.flow.value as UiLink.Connected
            assertEquals(LinkState.MODE_SWITCH, connected.mode)
            assertEquals("pro", connected.pad)
            assertEquals(TOKEN, PairStore.load(context)?.token)
            assertEquals(ReceiverMode.Eden, restarted.snapshot().mode)
            assertEquals(1, restarted.snapshot().peers.size)
            stopClient(client)
            await("Restarted receiver also releases its client") { restarted.snapshot().peers.isEmpty() }
            assertSavedPcAndPreferencesUnchanged()
        }
    }

    private fun startClient(): ClientSession {
        val existingLoopers = ShadowLooper.getAllLoopers().toSet()
        val service = Robolectric.buildService(LinkForegroundService::class.java).create()
        try {
            service.get().onStartCommand(Intent(), 0, 1)
            await("Real ControlClient completes PMP authentication: ${LinkState.flow.value}") { LinkState.flow.value is UiLink.Connected }
            val engine = requireNotNull(LinkState.motion)
            val looper = ShadowLooper.getAllLoopers().single { it !in existingLoopers && it.thread.name == "pepomote-sensors" }
            return ClientSession(service, engine, shadowOf(looper).apply { pause() }).also(clients::add)
        } catch (e: Throwable) {
            service.destroy()
            throw e
        }
    }

    private fun stopClient(client: ClientSession) {
        if (clients.remove(client)) client.service.destroy()
        client.engine.stop()
    }

    private fun advanceInput(client: ClientSession, ms: Int) {
        repeat(ms) {
            ShadowSystemClock.advanceBy(Duration.ofMillis(1))
            shadowOf(Looper.getMainLooper()).idle()
            client.sensorLooper.idle()
        }
    }

    private fun assertSavedPcAndPreferencesUnchanged() {
        assertEquals(savedPc, PairStore.all(context).single { it.token == savedPc.token })
        assertEquals(2, PairStore.all(context).size)
        assertEquals("wiimote", AppPrefs.pad(context, LinkState.MODE_CEMU))
        assertTrue(AppPrefs.gamePadFullScreen(context))
    }

    private fun subscribe(receiver: ReceiverCore, socket: DatagramSocket) {
        val bytes = request(0x100002, byteArrayOf(1, 0, 0, 0, 0, 0, 0, 0))
        socket.send(DatagramPacket(bytes, bytes.size, InetAddress.getByName("127.0.0.1"), receiver.snapshot().dsuPort))
        receiveDsu(socket) { true }
    }

    private fun receiveDsu(socket: DatagramSocket, match: (ByteArray) -> Boolean): ByteArray {
        val deadline = System.nanoTime() + 3_000_000_000L
        while (System.nanoTime() < deadline) {
            socket.soTimeout = ((deadline - System.nanoTime()) / 1_000_000).coerceAtLeast(1).toInt()
            val packet = DatagramPacket(ByteArray(256), 256)
            socket.receive(packet)
            val bytes = packet.data.copyOf(packet.length)
            if (bytes.size == 100 && u(bytes[20]) == 0 && match(bytes)) return bytes
        }
        throw SocketTimeoutException("Expected DSU controller state was not received")
    }

    private fun await(message: String, condition: () -> Boolean) {
        val deadline = System.nanoTime() + 4_000_000_000L
        do {
            shadowOf(Looper.getMainLooper()).idle()
            if (condition()) return
            Thread.sleep(5)
        } while (System.nanoTime() < deadline)
        assertTrue("$message; final link = ${LinkState.flow.value}", condition())
    }

    private fun config(code: String) = ReceiverConfig("Android receiver under test", TOKEN, code, 0, 0)
    private fun temporaryPair(receiver: ReceiverCore, code: String) = Pairing(
        "127.0.0.1", receiver.snapshot().port, code, "Android receiver under test", ReceiverCapabilities.ANDROID)

    private companion object { const val TOKEN = "permanent-android-receiver-e2e-token" }
}
