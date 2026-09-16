package dev.pepotech.pepomote.net

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.NetworkInterface

data class ReceiverInfo(val name: String, val host: String, val tcpPort: Int,
    val platform: String = ReceiverCapabilities.DESKTOP)

/** Descubrimiento por broadcast UDP (fallback sin mDNS): PMPDISCOVER1 → PMPHERE1. */
object Discovery {
    /** Puerto UDP del receptor (PROTOCOL.md §1). */
    const val PORT = 26761

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

    /**
     * Sondea la red durante [timeoutMs] y va emitiendo la lista de receptores
     * vistos hasta el momento cada vez que aparece uno nuevo: en la LAN el
     * primero contesta en decenas de milisegundos, y la pantalla no tiene que
     * esperar al final del sondeo para enseñarlo. La última emisión, siempre
     * (aunque no haya contestado nadie), es la lista completa del sondeo.
     */
    fun scan(timeoutMs: Int = 1500): Flow<List<ReceiverInfo>> = scan(timeoutMs, PORT, null)

    /** [targets] null = los broadcast de la red; los tests sondean el loopback. */
    internal fun scan(timeoutMs: Int, port: Int, targets: List<InetAddress>?): Flow<List<ReceiverInfo>> = flow {
        val found = LinkedHashMap<String, ReceiverInfo>()
        val socket = try {
            DatagramSocket().apply {
                broadcast = true
                soTimeout = 300
            }
        } catch (_: Exception) {
            null
        }
        socket?.use { s ->
            val probe = PmpCodec.DISCOVER
            for (target in targets ?: broadcastTargets()) {
                try {
                    s.send(DatagramPacket(probe, probe.size, target, port))
                } catch (_: Exception) {
                }
            }

            val buf = ByteArray(1024)
            val deadline = System.currentTimeMillis() + timeoutMs
            while (System.currentTimeMillis() < deadline) {
                // Al salir de la pantalla se cancela el sondeo: como mucho
                // espera el soTimeout de una lectura.
                currentCoroutineContext().ensureActive()
                val pkt = DatagramPacket(buf, buf.size)
                try {
                    s.receive(pkt)
                } catch (_: Exception) {
                    continue
                }
                val host = pkt.address?.hostAddress ?: continue
                val r = parseHere(String(buf, 0, pkt.length, Charsets.UTF_8), host) ?: continue
                // Un receptor contesta a cada broadcast que le llega (el
                // limitado y el dirigido): solo el primero cuenta como nuevo.
                if (found.put(host, r) == null) emit(found.values.toList())
            }
        }
        emit(found.values.toList())
    }.flowOn(Dispatchers.IO)

    /** `PMPHERE1 {json}` → receptor, o null si no es una respuesta válida. */
    internal fun parseHere(text: String, host: String): ReceiverInfo? {
        if (!text.startsWith(PmpCodec.HERE_PREFIX)) return null
        return try {
            val json = JSONObject(text.removePrefix(PmpCodec.HERE_PREFIX))
            if (json.optInt("pv") != 1) return null
            ReceiverInfo(
                name = json.optString("name", host),
                host = host,
                tcpPort = json.optInt("tcp", PORT),
                platform = ReceiverCapabilities.platform(json.optString("platform"))
            )
        } catch (_: Exception) {
            null
        }
    }
}
