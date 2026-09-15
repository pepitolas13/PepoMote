package dev.pepotech.pepomote.server

import org.junit.Assert.*
import org.junit.Test

class ServerNetworksTest {
    @Test fun cellularPrivateAddressIsNotAdvertisedBesideWifi() {
        val result = ServerNetworks.select(listOf(
            ServerNetworkInfo("rmnet_data0", listOf("10.45.20.7"), ServerNetworkKind.Cellular),
            ServerNetworkInfo("wlan0", listOf("192.168.1.20"), ServerNetworkKind.Local, true)
        ), listOf(ServerInterfaceInfo("rmnet_data0", listOf("10.45.20.7")),
            ServerInterfaceInfo("wlan0", listOf("192.168.1.20"))))
        assertEquals(listOf("192.168.1.20"), result.local)
        assertTrue(result.vpn.isEmpty())
    }

    @Test fun vpnUsesItsOwnQrAndNeverReplacesTheLocalAddress() {
        val result = ServerNetworks.select(listOf(
            ServerNetworkInfo("tun0", listOf("100.100.10.2", "fd7a:115c:a1e0::1"), ServerNetworkKind.Vpn, true),
            ServerNetworkInfo("wlan0", listOf("192.168.1.20"), ServerNetworkKind.Local)
        ), listOf(ServerInterfaceInfo("tun0", listOf("100.100.10.2")), ServerInterfaceInfo("wlan0", listOf("192.168.1.20"))))
        assertEquals(listOf("192.168.1.20"), result.local)
        assertEquals(listOf("100.100.10.2"), result.vpn)
    }

    @Test fun aPrivateVpnDoesNotLeakIntoLocalChoices() {
        val result = ServerNetworks.select(listOf(
            ServerNetworkInfo("tun0", listOf("10.8.0.2"), ServerNetworkKind.Vpn)
        ), listOf(ServerInterfaceInfo("tun0", listOf("10.8.0.2"))))
        assertTrue(result.local.isEmpty())
        assertEquals(listOf("10.8.0.2"), result.vpn)
    }

    @Test fun hotspotRemainsAvailableWhenAndroidOnlyReportsTheCellularUplink() {
        val result = ServerNetworks.select(listOf(
            ServerNetworkInfo("rmnet0", listOf("10.8.0.2"), ServerNetworkKind.Cellular, true)
        ), listOf(ServerInterfaceInfo("ap0", listOf("192.168.43.1")),
            ServerInterfaceInfo("rmnet0", listOf("10.8.0.2"))))
        assertEquals(listOf("192.168.43.1"), result.local)
    }

    @Test fun activeWifiWinsWithoutDependingOnInterfaceEnumerationOrder() {
        val result = ServerNetworks.select(listOf(
            ServerNetworkInfo("eth0", listOf("192.168.0.2"), ServerNetworkKind.Local),
            ServerNetworkInfo("wlan1", listOf("192.168.1.2"), ServerNetworkKind.Local, true)
        ), listOf(ServerInterfaceInfo("eth0", listOf("192.168.0.2")),
            ServerInterfaceInfo("wlan0", listOf("192.168.2.2")),
            ServerInterfaceInfo("wlan1", listOf("192.168.1.2"))))
        assertEquals("192.168.1.2", result.local.first())
    }

    @Test fun excludesUnusableAndUnidentifiedInterfacesInsteadOfGuessingFromPrivateRange() {
        val result = ServerNetworks.select(emptyList(), listOf(
            ServerInterfaceInfo("rmnet0", listOf("100.70.1.3", "10.0.0.2")),
            ServerInterfaceInfo("p2p0", listOf("192.168.49.1")),
            ServerInterfaceInfo("tun0", listOf("10.8.0.2")),
            ServerInterfaceInfo("wlan0", listOf("::1", "127.0.0.1", "0.0.0.0", "224.0.0.1", "169.254.1.3"))))
        assertTrue(result.local.isEmpty())
        assertTrue(result.vpn.isEmpty())
    }

    @Test fun trustedLocalNetworkCanUsePubliclyAssignedAddress() {
        val result = ServerNetworks.select(listOf(
            ServerNetworkInfo("eth0", listOf("203.0.113.2"), ServerNetworkKind.Local)
        ), emptyList())
        assertEquals(listOf("203.0.113.2"), result.local)
    }

    @Test fun cellularTransportOverridesWifiLookingInterfaceName() {
        val result = ServerNetworks.select(listOf(
            ServerNetworkInfo("wlan0", listOf("10.0.0.2"), ServerNetworkKind.Cellular)
        ), listOf(ServerInterfaceInfo("wlan0", listOf("10.0.0.2"))))
        assertTrue(result.local.isEmpty())
    }
}
