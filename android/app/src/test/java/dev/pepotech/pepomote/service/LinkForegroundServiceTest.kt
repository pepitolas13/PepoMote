package dev.pepotech.pepomote.service

import android.content.Context
import android.content.Intent
import android.os.Looper
import android.os.Vibrator
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.control.GameRumble
import dev.pepotech.pepomote.control.RumbleMotor
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.net.Pairing
import dev.pepotech.pepomote.sensor.MotionEngine
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowVibrator
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.ServerSocket
import java.net.SocketAddress
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.time.Duration
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class LinkForegroundServiceTest {
    private fun awaitState(condition: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(4)
        while (!condition() && System.nanoTime() < deadline) Thread.sleep(5)
        assertTrue("Control callback completed", condition())
    }

    @Test fun repeatedHandshakeCannotStartAnotherEngineAndReconnectReplacesIt() {
        val context: Context = RuntimeEnvironment.getApplication()
        AppPrefs.setSoundsEnabled(context, false)
        LinkForegroundService.setAppVisible(true)
        ServerSocket(0).use { receiver ->
            receiver.soTimeout = 4000
            DatagramSocket(0).use { udp ->
                PairStore.save(context, Pairing("127.0.0.1", receiver.localPort, "test-token", "Test PC"))
                val service = Robolectric.buildService(LinkForegroundService::class.java).create()
                var original: MotionEngine? = null
                try {
                    service.get().onStartCommand(Intent(), 0, 1)
                    receiver.accept().use { socket ->
                        socket.soTimeout = 4000
                        assertTrue(socket.getInputStream().bufferedReader().readLine().contains("\"hello\""))
                        val writer = socket.getOutputStream().bufferedWriter()
                        fun send(line: String) { writer.write(line); writer.newLine(); writer.flush() }
                        send("""{"m":"ok","session_id":1,"udp_port":${udp.localPort},"mode":"pointer"}""")
                        awaitState { LinkState.flow.value is UiLink.Connected }
                        original = LinkState.motion
                        assertNotNull(original)
                        send("""{"m":"ok","session_id":1,"udp_port":${udp.localPort},"mode":"pointer"}""")
                        send("""{"m":"pad","pad":"gamepad","player":2}""")
                        awaitState { (LinkState.flow.value as? UiLink.Connected)?.player == 2 }
                        assertSame("A repeated ok must not orphan a sending engine", original, LinkState.motion)
                        service.get().onStartCommand(Intent(), 0, 2)
                    }
                    receiver.accept().use { socket ->
                        socket.soTimeout = 4000
                        assertTrue(socket.getInputStream().bufferedReader().readLine().contains("\"hello\""))
                        val writer = socket.getOutputStream().bufferedWriter()
                        writer.write("""{"m":"ok","session_id":2,"udp_port":${udp.localPort},"mode":"pointer"}""")
                        writer.newLine(); writer.flush()
                        awaitState { LinkState.flow.value is UiLink.Connected }
                        assertNotSame(original, LinkState.motion)
                    }
                } finally {
                    service.destroy()
                    original?.stop()
                    LinkForegroundService.setAppVisible(false)
                }
                assertNull(LinkState.motion)
                assertEquals(UiLink.Disconnected, LinkState.flow.value)
            }
        }
    }

    /** Como [awaitState], pero dejando correr lo encolado en el hilo principal (los RUMBLE se postean ahí). */
    private fun awaitMain(condition: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(4)
        while (!condition() && System.nanoTime() < deadline) {
            shadowOf(Looper.getMainLooper()).idle()
            Thread.sleep(5)
        }
        assertTrue("main thread condition", condition())
    }

    /** Como [awaitMain], avanzando además el reloj 20 ms por vuelta (para lo programado con retraso: el hueco de silencio). */
    private fun awaitMainFor(condition: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(4)
        while (!condition() && System.nanoTime() < deadline) {
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(20))
            Thread.sleep(5)
        }
        assertTrue("main thread condition (timed)", condition())
    }

    /** Un RUMBLE de 20 bytes (PROTOCOL.md §4.5) del receptor falso al móvil. */
    private fun sendRumble(udp: DatagramSocket, to: SocketAddress, session: Int, seq: Int, strong: Int, ttl: Int) {
        val b = ByteBuffer.allocate(20).order(ByteOrder.LITTLE_ENDIAN)
        b.put("PMP1".toByteArray(Charsets.US_ASCII)).put(4).put(0).putShort(0)
        b.putInt(session).putInt(seq).put(strong.toByte()).put(0).putShort(ttl.toShort())
        udp.send(DatagramPacket(b.array(), 20, to))
    }

    /**
     * La vibración de los juegos de extremo a extremo: un RUMBLE del receptor
     * enciende el motor con una onda FINITA (nunca un bucle sin fin), sin
     * refresco el TTL la para sola, un cero la para tras el hueco de
     * silencio, y con la app fuera de pantalla no vibra nada.
     */
    @Test fun elRumbleDelReceptorVibraConOndaFinitaYParaSoloSinRefrescoOConCero() {
        val context: Context = RuntimeEnvironment.getApplication()
        AppPrefs.setSoundsEnabled(context, false)
        AppPrefs.setGameRumble(context, "high")
        ShadowVibrator.reset()
        val vib = shadowOf(context.getSystemService(Vibrator::class.java))
        vib.setHasAmplitudeControl(true) // antes de crear el servicio: attach lo cachea
        LinkForegroundService.setAppVisible(true)
        ServerSocket(0).use { receiver ->
            receiver.soTimeout = 4000
            DatagramSocket(0).use { udp ->
                udp.soTimeout = 4000
                PairStore.save(context, Pairing("127.0.0.1", receiver.localPort, "test-token", "Test PC"))
                val service = Robolectric.buildService(LinkForegroundService::class.java).create()
                try {
                    service.get().onStartCommand(Intent(), 0, 1)
                    receiver.accept().use { socket ->
                        socket.soTimeout = 4000
                        assertTrue(socket.getInputStream().bufferedReader().readLine().contains("\"hello\""))
                        val writer = socket.getOutputStream().bufferedWriter()
                        writer.write("""{"m":"ok","session_id":7,"udp_port":${udp.localPort},"mode":"dolphin"}""")
                        writer.newLine(); writer.flush()
                        awaitState { LinkState.flow.value is UiLink.Connected }
                        // El primer datagrama del móvil (su PING) dice desde qué puerto habla
                        val first = DatagramPacket(ByteArray(256), 256)
                        udp.receive(first)
                        val phone = first.socketAddress

                        // 1. Enciende con una onda finita
                        sendRumble(udp, phone, 7, 1, 255, 400)
                        awaitMain { vib.isVibrating }
                        assertEquals(listOf(750L, 750L, 750L, 750L), vib.pattern.toList())
                        assertEquals("sin bucle", -1, vib.repeat)
                        assertTrue(GameRumble.isOn)
                        // Otra sesión no manda aquí
                        sendRumble(udp, phone, 8, 2, 0, 0)

                        // 2. Sin refresco: el TTL (400 ms) vence y el motor cancela tras el hueco
                        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(700))
                        assertTrue("TTL vencido: cancelado", vib.isCancelled)
                        assertFalse(GameRumble.isOn)

                        // 3. Un cero explícito para tras el hueco de silencio
                        ShadowVibrator.reset()
                        sendRumble(udp, phone, 7, 2, 200, 400)
                        awaitMain { vib.isVibrating }
                        sendRumble(udp, phone, 7, 3, 0, 0)
                        awaitMainFor { vib.isCancelled }
                        assertTrue("cero: cancelado", vib.isCancelled)

                        // 4. Fuera de pantalla: para ya y los RUMBLE se ignoran
                        ShadowVibrator.reset()
                        sendRumble(udp, phone, 7, 4, 255, 400)
                        awaitMain { vib.isVibrating }
                        LinkForegroundService.setAppVisible(false)
                        assertTrue("al irse al fondo: cancelado", vib.isCancelled)
                        ShadowVibrator.reset()
                        sendRumble(udp, phone, 7, 5, 255, 400)
                        Thread.sleep(200)
                        shadowOf(Looper.getMainLooper()).idle()
                        assertFalse("fuera de pantalla no vibra", vib.isVibrating)
                        // De vuelta, el siguiente refresco rearranca (la máquina olvidó el seq)
                        LinkForegroundService.setAppVisible(true)
                        sendRumble(udp, phone, 7, 6, 255, 400)
                        awaitMain { vib.isVibrating }
                    }
                } finally {
                    service.destroy()
                    LinkForegroundService.setAppVisible(false)
                }
                assertEquals(UiLink.Disconnected, LinkState.flow.value)
                assertFalse("al destruir el servicio el motor queda apagado", GameRumble.isOn)
            }
        }
    }
}
