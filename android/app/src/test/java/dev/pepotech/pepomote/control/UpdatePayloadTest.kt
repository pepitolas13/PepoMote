package dev.pepotech.pepomote.control

import dev.pepotech.pepomote.net.UpdatePayload
import dev.pepotech.pepomote.net.UpdateTransportPolicy
import kotlinx.coroutines.CancellationException
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.security.MessageDigest

class UpdatePayloadTest {
    private val bytes = "verified package".toByteArray()
    private val asset = UpdateAsset("PepoMote.apk", "", bytes.size.toLong(),
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) })

    @Test fun copiesOnlyExactSizeAndDigest() {
        val output = ByteArrayOutputStream()
        var progress = 0L
        UpdatePayload.copyVerified(bytes.inputStream(), output, asset, {}, { progress = it })
        assertArrayEquals(bytes, output.toByteArray())
        assertEquals(asset.size, progress)
        for (invalid in listOf(bytes.copyOf(bytes.size - 1), bytes + byteArrayOf(0), "modified package".toByteArray())) {
            assertThrows(IllegalArgumentException::class.java) {
                UpdatePayload.copyVerified(invalid.inputStream(), ByteArrayOutputStream(), asset, {}, {})
            }
        }
    }
    @Test fun cancellationStopsBeforeWriting() {
        val output = ByteArrayOutputStream()
        assertThrows(CancellationException::class.java) {
            UpdatePayload.copyVerified(bytes.inputStream(), output, asset, { throw CancellationException() }, {})
        }
        assertEquals(0, output.size())
    }
    @Test fun redirectsStayOnHttpsGitHubReleaseInfrastructure() {
        assertTrue(UpdateTransportPolicy.allowed("https://release-assets.githubusercontent.com/a?token=abc", "PepoMote.apk"))
        assertTrue(UpdateTransportPolicy.allowed("https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.apk", "PepoMote.apk"))
        for (bad in listOf("http://release-assets.githubusercontent.com/a", "https://evil.example/a",
            "https://github.com/evil/repo/releases/download/v1.11.0/PepoMote.apk",
            "https://user@release-assets.githubusercontent.com/a", "https://release-assets.githubusercontent.com:444/a",
            "https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/evil.apk")) {
            assertFalse(bad, UpdateTransportPolicy.allowed(bad, "PepoMote.apk"))
        }
    }
}
