package dev.pepotech.pepomote.net

import android.os.SystemClock
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress

/**
 * Socket UDP caliente: envía INPUT, responde a los PING del receptor
 * y mide RTT con sus propios PING (1 Hz).
 */
class UdpSender(
    host: String,
    port: Int,
    private val sessionId: Int,
    private val onRtt: (Float) -> Unit
) {
    private val socket = DatagramSocket().apply {
        // DSCP EF (46): la Wi-Fi (WMM) mete estos datagramas en la cola de
        // voz, por delante del tráfico normal → menos jitter con la red
        // cargada. Si el sistema lo ignora o lo prohíbe, no pasa nada.
        try {
            trafficClass = 0xB8
        } catch (_: Exception) {
        }
        connect(InetSocketAddress(InetAddress.getByName(host), port))
    }

    @Volatile
    private var running = true

    private val listener = Thread({
        val buf = ByteArray(256)
        val pkt = DatagramPacket(buf, buf.size)
        while (running) {
            try {
                socket.receive(pkt)
                when (PmpCodec.packetType(buf, pkt.length)) {
                    PmpCodec.TYPE_PING -> {
                        // Eco: mismo cuerpo, tipo PONG
                        send(PmpCodec.encodePong(PmpCodec.pingSession(buf), PmpCodec.pingT(buf)))
                    }

                    PmpCodec.TYPE_PONG -> {
                        if (PmpCodec.pingSession(buf) == sessionId) {
                            val rttUs = nowUs() - PmpCodec.pingT(buf)
                            if (rttUs in 0..5_000_000) onRtt(rttUs / 1000f)
                        }
                    }

                    else -> Unit
                }
            } catch (_: Exception) {
                // socket cerrado o error puntual
            }
        }
    }, "pepomote-udp-listener").apply { isDaemon = true; start() }

    private val pinger = Thread({
        while (running) {
            try {
                send(PmpCodec.encodePing(sessionId, nowUs()))
                Thread.sleep(1000)
            } catch (_: InterruptedException) {
                break
            } catch (_: Exception) {
            }
        }
    }, "pepomote-udp-ping").apply { isDaemon = true; start() }

    fun send(data: ByteArray) {
        try {
            socket.send(DatagramPacket(data, data.size))
        } catch (_: Exception) {
        }
    }

    fun close() {
        running = false
        pinger.interrupt()
        socket.close()
        listener.join(500)
    }

    private fun nowUs() = SystemClock.elapsedRealtimeNanos() / 1000
}
