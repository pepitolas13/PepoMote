package dev.pepotech.pepomote.server

import android.app.Notification
import android.content.Context
import android.content.Intent
import android.os.Looper
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.net.Pairing
import dev.pepotech.pepomote.net.ReceiverCapabilities
import dev.pepotech.pepomote.server.core.ReceiverConfig
import dev.pepotech.pepomote.server.core.ReceiverCore
import dev.pepotech.pepomote.server.core.ReceiverSnapshot
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import java.net.DatagramSocket
import java.net.InetSocketAddress
import java.net.ServerSocket

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26, 35])
@LooperMode(LooperMode.Mode.PAUSED)
class ServerLifecycleTest {
    class IsolatedService : ServerForegroundService() {
        override fun createReceiver(config: ReceiverConfig, onState: (ReceiverSnapshot) -> Unit) =
            ReceiverCore(config.copy(port = 0, dsuPort = 0), onState)
    }

    private fun await(condition: () -> Boolean) {
        val until = System.nanoTime() + 5_000_000_000
        do {
            shadowOf(Looper.getMainLooper()).idle()
            if (condition()) return
            Thread.sleep(10)
        } while (System.nanoTime() < until)
        assertTrue("Receiver lifecycle completed", condition())
    }

    @Test fun repeatedStartHasOneSessionAndDestroyReleasesBothProtocols() {
        val service = Robolectric.buildService(IsolatedService::class.java).create()
        var tcpPort = 0
        var dsuPort = 0
        try {
            service.get().onStartCommand(Intent(), 0, 1)
            await { ServerState.flow.value.running }
            val first = ServerState.flow.value
            tcpPort = first.receiver!!.port
            dsuPort = first.receiver.dsuPort
            val notification = shadowOf(service.get()).lastForegroundNotification
            assertNotNull("Foreground notification exists before a game opens", notification)
            assertTrue(notification.flags and Notification.FLAG_ONGOING_EVENT != 0)
            service.get().onStartCommand(Intent(), 0, 2)
            assertEquals(first.config, ServerState.flow.value.config)
            assertEquals(tcpPort, ServerState.flow.value.receiver!!.port)
            // Removing the task does not turn off the receiver under a foreground emulator.
            service.get().onTaskRemoved(Intent())
            assertTrue(ServerState.flow.value.running)
        } finally { service.destroy() }
        await {
            runCatching {
                ServerSocket().use { it.bind(InetSocketAddress(tcpPort)) }
                DatagramSocket(null).use { it.bind(InetSocketAddress(tcpPort)) }
                DatagramSocket(null).use { it.bind(InetSocketAddress("127.0.0.1", dsuPort)) }
            }.isSuccess
        }
        assertFalse(ServerState.flow.value.active)
    }

    @Test fun qrRoundTripsAndServerIdentityPreservesSavedPc() {
        val context: Context = RuntimeEnvironment.getApplication()
        val pc = Pairing("192.168.10.9", 26761, "saved-desktop-token", "My PC")
        PairStore.save(context, pc)
        val first = ServerIdentity.config(context)
        val second = ServerIdentity.config(context)
        assertEquals(first.token, second.token)
        assertEquals(43, first.token.length)
        assertTrue(first.pairCode.matches(Regex("[0-9]{6}")))
        val parsed = PairStore.parsePairUrl(ServerIdentity.pairUrl(first, "192.168.10.3"))!!
        assertEquals(ReceiverCapabilities.ANDROID, parsed.platform)
        assertEquals(first.token, parsed.token)
        assertEquals(first.name, parsed.pcName)
        assertEquals("192.168.10.3", parsed.host)
        assertEquals(pc, PairStore.load(context))
    }

    @Test fun immediateStopCannotReappearOrHoldPortsInNextInstance() {
        repeat(3) {
            val service = Robolectric.buildService(IsolatedService::class.java).create()
            service.get().onStartCommand(Intent(), 0, it + 1)
            service.destroy()
        }
        val final = Robolectric.buildService(IsolatedService::class.java).create()
        try {
            final.get().onStartCommand(Intent(), 0, 10)
            await { ServerState.flow.value.running }
            assertNull(ServerState.flow.value.error)
        } finally { final.destroy() }
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse(ServerState.flow.value.active)
    }
}
