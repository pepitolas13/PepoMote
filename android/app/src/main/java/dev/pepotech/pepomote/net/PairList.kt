package dev.pepotech.pepomote.net

import java.net.URLDecoder
import java.net.URLEncoder

/**
 * Varios PCs guardados: funciones puras sobre la lista (sin Android),
 * testeadas en PairListTest. Un token es un PC; su nombre y su IP pueden
 * cambiar (renombrado, DHCP) y se actualizan sin perder el token.
 */
object PairList {
    private fun enc(s: String): String = URLEncoder.encode(s, "UTF-8")
    private fun dec(s: String): String = URLDecoder.decode(s, "UTF-8")

    /** Una línea por PC: `t=…&host=…&port=…&name=…`, valores URL-encoded. */
    fun encode(list: List<Pairing>): String =
        list.joinToString("\n") { p -> "t=${enc(p.token)}&host=${enc(p.host)}&port=${p.port}&name=${enc(p.pcName)}" }

    fun decode(text: String?): List<Pairing> =
        text.orEmpty().lines().filter { it.isNotBlank() }.mapNotNull { line ->
            val f = line.split("&").mapNotNull { kv ->
                val i = kv.indexOf('=')
                if (i <= 0) null else kv.substring(0, i) to dec(kv.substring(i + 1))
            }.toMap()
            val token = f["t"] ?: return@mapNotNull null
            val host = f["host"] ?: return@mapNotNull null
            Pairing(host, f["port"]?.toIntOrNull() ?: 26761, token, f["name"] ?: "PC")
        }

    /** Añade o actualiza (por token) conservando el orden. */
    fun upsert(list: List<Pairing>, p: Pairing): List<Pairing> =
        if (list.any { it.token == p.token }) list.map { if (it.token == p.token) p else it } else list + p

    fun forget(list: List<Pairing>, token: String): List<Pairing> = list.filter { it.token != token }

    /** PC actual tras olvidar `forgotten`: el mismo si sigue en la lista; si no, el primero que quede. */
    fun nextCurrent(list: List<Pairing>, current: String?, forgotten: String): String? =
        if (current != forgotten && list.any { it.token == current }) current else list.firstOrNull()?.token

    /** Un PC guardado se ve en la red (por nombre o por IP). */
    fun isOnline(p: Pairing, receivers: List<ReceiverInfo>): Boolean =
        receivers.any { it.name == p.pcName || it.host == p.host }

    /** Receptores de la red que no son ninguno de los guardados. */
    fun unknown(receivers: List<ReceiverInfo>, list: List<Pairing>): List<ReceiverInfo> =
        receivers.filter { r -> list.none { it.pcName == r.name || it.host == r.host } }

    /**
     * IP y puerto de los PCs guardados que la red enseña en otro sitio (DHCP,
     * otra Wi-Fi), por nombre. Un sitio ya ocupado por OTRO PC guardado (otro
     * token) no se adopta: dos PCs con el mismo nombre no se pisan.
     */
    fun refreshFrom(list: List<Pairing>, receivers: List<ReceiverInfo>): List<Pairing> =
        list.map { p -> relocateTarget(p, list, receivers) }

    /** `p` con la IP/puerto donde la red dice que está ahora su nombre (o tal cual). */
    fun relocateTarget(p: Pairing, list: List<Pairing>, receivers: List<ReceiverInfo>): Pairing {
        val found = receivers.firstOrNull { it.name == p.pcName && (it.host != p.host || it.tcpPort != p.port) }
            ?: return p
        val claimed = list.any { it.token != p.token && it.host == found.host && it.port == found.tcpPort }
        return if (claimed) p else p.copy(host = found.host, port = found.tcpPort)
    }
}
