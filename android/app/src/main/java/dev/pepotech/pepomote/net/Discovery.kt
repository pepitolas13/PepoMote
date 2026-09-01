package dev.pepotech.pepomote.net

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.NetworkInterface

data class ReceiverInfo(val name: String, val host: String, val tcpPort: Int)

/** Descubrimiento por broadcast UDP (fallback sin mDNS): PMPDISCOVER1 → PMPHERE1. */
object Discovery {
    /**
     * Destinos del sondeo: el broadcast limitado (255.255.255.255) y el
     * DIRIGIDO de cada interfaz (p. ej. 192.168.1.255). Bastantes móviles y
     * routers descartan el limitado; el dirigido llega. PROTOCOL.md §1.
     */
    private fun broadcastTargets(): List<InetAddress> {
        val targets = LinkedHashSet<InetAddress>()
        targets += InetAddress.getByName("255.255.255.255")
        try {
            NetworkInterface.getNetworkInterfaces()?.toList()?.forEach { nic ->
                if (!nic.isUp || nic.isLoopback) return@forEach
                nic.interfaceAddresses.forEach { ia -> ia.broadcast?.let { targets += it } }
            }
        } catch (_: Exception) {
        }
        return targets.toList()
    }

    suspend fun scan(timeoutMs: Int = 1500): List<ReceiverInfo> = withContext(Dispatchers.IO) {
        val found = LinkedHashMap<String, ReceiverInfo>()
        try {
            DatagramSocket().use { socket ->
                socket.broadcast = true
                socket.soTimeout = 300
                val probe = PmpCodec.DISCOVER
                for (target in broadcastTargets()) {
                    try {
                        socket.send(DatagramPacket(probe, probe.size, target, 26761))
                    } catch (_: Exception) {
                    }
                }

                val buf = ByteArray(1024)
                val deadline = System.currentTimeMillis() + timeoutMs
                while (System.currentTimeMillis() < deadline) {
                    val pkt = DatagramPacket(buf, buf.size)
                    try {
                        socket.receive(pkt)
                    } catch (_: Exception) {
                        continue
                    }
                    val text = String(buf, 0, pkt.length, Charsets.UTF_8)
                    if (!text.startsWith(PmpCodec.HERE_PREFIX)) continue
                    try {
                        val json = JSONObject(text.removePrefix(PmpCodec.HERE_PREFIX))
                        if (json.optInt("pv") != 1) continue
                        val host = pkt.address.hostAddress ?: continue
                        found[host] = ReceiverInfo(
                            name = json.optString("name", host),
                            host = host,
                            tcpPort = json.optInt("tcp", 26761)
                        )
                    } catch (_: Exception) {
                    }
                }
            }
        } catch (_: Exception) {
        }
        found.values.toList()
    }
}
