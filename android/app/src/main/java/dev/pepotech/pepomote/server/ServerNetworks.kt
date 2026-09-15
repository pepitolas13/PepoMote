package dev.pepotech.pepomote.server

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import java.net.Inet4Address
import java.net.NetworkInterface
import java.util.Collections

internal enum class ServerNetworkKind { Local, Vpn, Cellular, Other }

internal data class ServerNetworkInfo(
    val interfaceName: String,
    val addresses: List<String>,
    val kind: ServerNetworkKind,
    val active: Boolean = false,
)

internal data class ServerInterfaceInfo(val name: String, val addresses: List<String>)
internal data class ServerNetworkAddresses(val local: List<String>, val vpn: List<String>)

internal object ServerNetworks {
    /** Read transports, not address ranges: a cellular connection can also have a 10.x address. */
    @Suppress("DEPRECATION")
    fun addresses(context: Context): ServerNetworkAddresses {
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        val networks = runCatching {
            val active = connectivity?.activeNetwork
            connectivity?.allNetworks.orEmpty().mapNotNull { network ->
                val capabilities = connectivity?.getNetworkCapabilities(network) ?: return@mapNotNull null
                val properties = connectivity.getLinkProperties(network) ?: return@mapNotNull null
                val name = properties.interfaceName ?: return@mapNotNull null
                val kind = when {
                    // VPNs may also report their underlying Wi-Fi/cellular transport.
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> ServerNetworkKind.Vpn
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> ServerNetworkKind.Cellular
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
                        capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) ||
                        capabilities.hasTransport(NetworkCapabilities.TRANSPORT_BLUETOOTH) -> ServerNetworkKind.Local
                    else -> ServerNetworkKind.Other
                }
                ServerNetworkInfo(name, properties.linkAddresses.mapNotNull { it.address.hostAddress }, kind, network == active)
            }
        }.getOrDefault(emptyList())
        // Hotspot/downstream interfaces are not always exposed as ConnectivityManager Networks.
        val interfaces = runCatching {
            Collections.list(NetworkInterface.getNetworkInterfaces()).filter { it.isUp && !it.isLoopback }
                .map { network -> ServerInterfaceInfo(network.name,
                    Collections.list(network.inetAddresses).filterIsInstance<Inet4Address>().mapNotNull { it.hostAddress }) }
        }.getOrDefault(emptyList())
        return select(networks, interfaces)
    }

    internal fun select(networks: List<ServerNetworkInfo>, interfaces: List<ServerInterfaceInfo>): ServerNetworkAddresses {
        val knownNames = networks.map { it.interfaceName }.toSet()
        val local = networks.filter { it.kind == ServerNetworkKind.Local }
            .sortedWith(compareBy<ServerNetworkInfo> { !it.active }.thenBy { it.interfaceName })
            .flatMap { it.addresses }.filter(::usableIpv4).toMutableList()
        interfaces.filter { it.name !in knownNames && localInterface.matches(it.name) }
            .sortedWith(compareBy<ServerInterfaceInfo> { if (wifiInterface.matches(it.name)) 0 else 1 }.thenBy { it.name })
            .forEach { local += it.addresses.filter(::usableIpv4) }
        val vpn = networks.filter { it.kind == ServerNetworkKind.Vpn }
            .sortedWith(compareBy<ServerNetworkInfo> { !it.active }.thenBy { it.interfaceName })
            .flatMap { it.addresses }.filter(::usableIpv4).distinct()
        return ServerNetworkAddresses(local.distinct(), vpn)
    }

    // Numeric parsing deliberately avoids DNS while refreshing the dashboard.
    private fun usableIpv4(value: String): Boolean {
        val parts = value.split('.')
        if (parts.size != 4 || parts.any { it.isEmpty() || it.length > 3 || it.any { c -> c !in '0'..'9' } }) return false
        val bytes = parts.map { it.toInt() }
        return bytes.all { it in 0..255 } && bytes[0] !in setOf(0, 127) && bytes[0] < 224 &&
            !(bytes[0] == 169 && bytes[1] == 254)
    }

    private val wifiInterface = Regex("(?:wlan|swlan|ap|softap|wifi)\\d*")
    private val localInterface = Regex("(?:wlan|swlan|ap|softap|wifi|eth|en|rndis|usb|bnep|br|bridge)\\d*")
}
