package dev.pepotech.pepomote.net

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress

data class ReceiverInfo(val name: String, val host: String, val tcpPort: Int)

/** Descubrimiento por broadcast UDP (fallback sin mDNS): PMPDISCOVER1 → PMPHERE1. */
object Discovery {
    suspend fun scan(timeoutMs: Int = 1500): List<ReceiverInfo> = withContext(Dispatchers.IO) {
        val found = LinkedHashMap<String, ReceiverInfo>()
        try {
            DatagramSocket().use { socket ->
                socket.broadcast = true
                socket.soTimeout = 300
                val probe = PmpCodec.DISCOVER
                val target = InetAddress.getByName("255.255.255.255")
                socket.send(DatagramPacket(probe, probe.size, target, 26761))

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
