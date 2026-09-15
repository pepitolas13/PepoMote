package dev.pepotech.pepomote.service

import android.content.Context
import android.content.Intent
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.net.Pairing
import dev.pepotech.pepomote.sensor.MotionEngine
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.net.DatagramSocket
import java.net.ServerSocket
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
}
