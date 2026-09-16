package dev.pepotech.pepomote.net

import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import kotlin.concurrent.thread

/**
 * El sondeo enseña cada receptor según contesta, sin esperar a que acabe el
 * tiempo del sondeo entero: el PC de al lado sale en la lista al momento.
 * Receptor falso en el loopback (nada de broadcast real ni del puerto 26761).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class DiscoveryTest {

    private val loopback: InetAddress = InetAddress.getLoopbackAddress()

    /** Un receptor falso que contesta `PMPHERE1` a cada sondeo que le llega. */
    private fun fakeReceiver(name: String, platform: String? = null): DatagramSocket {
        val socket = DatagramSocket(0, loopback)
        thread(isDaemon = true) {
            val buf = ByteArray(256)
            try {
                while (true) {
                    val pkt = DatagramPacket(buf, buf.size)
                    socket.receive(pkt)
                    if (!String(buf, 0, pkt.length, Charsets.US_ASCII).startsWith("PMPDISCOVER1")) continue
                    val extra = if (platform != null) ",\"platform\":\"$platform\"" else ""
                    val reply = (PmpCodec.HERE_PREFIX + "{\"pv\":1,\"name\":\"$name\",\"tcp\":26761$extra}")
                        .toByteArray(Charsets.UTF_8)
                    socket.send(DatagramPacket(reply, reply.size, pkt.address, pkt.port))
                }
            } catch (_: Exception) {
            }
        }
        return socket
    }

    @Test
    fun elPrimerReceptorSeEmiteSinEsperarAlFinalDelSondeo() = runBlocking {
        fakeReceiver("TORRE").use { pc ->
            val t0 = System.currentTimeMillis()
            val lists = mutableListOf<List<ReceiverInfo>>()
            val atMs = mutableListOf<Long>()
            Discovery.scan(1500, pc.localPort, listOf(loopback)).collect { seen ->
                lists += seen
                atMs += System.currentTimeMillis() - t0
            }
            assertEquals(listOf("TORRE"), lists.first().map { it.name })
            assertEquals(ReceiverInfo("TORRE", loopback.hostAddress, 26761), lists.first().single())
            // Se mide el hueco entre la primera emisión y el final, no el
            // tiempo absoluto: el arranque de Robolectric en la CI cae dentro
            assertTrue(
                "la primera emisión llega mucho antes del final (${atMs.first()} → ${atMs.last()} ms)",
                atMs.last() - atMs.first() >= 1000
            )
            assertTrue("el sondeo entero sigue durando lo suyo (${atMs.last()} ms)", atMs.last() >= 1400)
            assertEquals("la última emisión es la lista completa", lists.first(), lists.last())
        }
    }

    @Test
    fun sinReceptoresEmiteSoloLaListaVaciaAlAcabar() = runBlocking {
        // Un socket sin nadie detrás: nadie contesta al sondeo
        DatagramSocket(0, loopback).use { nobody ->
            val lists = Discovery.scan(300, nobody.localPort, listOf(loopback)).toList()
            assertEquals(listOf(emptyList<ReceiverInfo>()), lists)
        }
    }

    @Test
    fun elReceptorAndroidLlegaConSuPlataforma() = runBlocking {
        fakeReceiver("Móvil servidor", platform = "android").use { pc ->
            val last = Discovery.scan(400, pc.localPort, listOf(loopback)).toList().last()
            assertEquals(listOf(ReceiverInfo("Móvil servidor", loopback.hostAddress, 26761, "android")), last)
        }
    }

    @Test
    fun parseHere() {
        assertEquals(
            ReceiverInfo("TORRE", "192.168.1.20", 26800),
            Discovery.parseHere("PMPHERE1 {\"pv\":1,\"name\":\"TORRE\",\"tcp\":26800}", "192.168.1.20")
        )
        assertEquals(
            "sin nombre ni puerto: la IP y el puerto de siempre",
            ReceiverInfo("192.168.1.20", "192.168.1.20", 26761),
            Discovery.parseHere("PMPHERE1 {\"pv\":1}", "192.168.1.20")
        )
        assertNull("otra versión del protocolo", Discovery.parseHere("PMPHERE1 {\"pv\":2,\"name\":\"X\",\"tcp\":1}", "10.0.0.2"))
        assertNull("otro paquete", Discovery.parseHere("otra cosa", "10.0.0.2"))
        assertNull("JSON roto", Discovery.parseHere("PMPHERE1 {pv:", "10.0.0.2"))
    }
}
