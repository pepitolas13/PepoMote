package dev.pepotech.pepomote.net

import dev.pepotech.pepomote.control.UpdateAsset
import dev.pepotech.pepomote.control.UpdateCheck
import dev.pepotech.pepomote.control.UpdateManifest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.security.MessageDigest

/** Explicit redirects prevent HTTPS downgrades and arbitrary download hosts. */
object UpdateTransportPolicy {
    fun allowed(url: String, assetName: String): Boolean = runCatching {
        val uri = URI(url)
        uri.scheme == "https" && uri.userInfo == null && uri.port in listOf(-1, 443) && uri.fragment == null &&
            when (uri.host) {
                "release-assets.githubusercontent.com", "objects.githubusercontent.com" -> true
                "github.com" -> uri.query == null && (uri.path == "/${UpdateCheck.REPO}/releases/latest/download/update.json" && assetName == "update.json" ||
                    Regex("/${UpdateCheck.REPO}/releases/download/v[0-9]+\\.[0-9]+\\.[0-9]+/${Regex.escape(assetName)}").matches(uri.path))
                else -> false
            }
    }.getOrDefault(false)
}

object UpdatePayload {
    fun copyVerified(input: InputStream, output: OutputStream, asset: UpdateAsset, checkActive: () -> Unit, progress: (Long) -> Unit) {
        require(asset.size in 1..UpdateManifest.MAX_PACKAGE_BYTES)
        val digest = MessageDigest.getInstance("SHA-256")
        val buffer = ByteArray(64 * 1024)
        var total = 0L
        while (true) {
            checkActive()
            val count = input.read(buffer)
            checkActive()
            if (count < 0) break
            total += count
            require(total <= asset.size) { "Package exceeds its published size" }
            output.write(buffer, 0, count)
            digest.update(buffer, 0, count)
            progress(total)
        }
        require(total == asset.size) { "Package is incomplete" }
        require(hex(digest.digest()) == asset.sha256) { "Package checksum mismatch" }
    }

    fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it.toInt() and 0xff) }
}

object UpdateClient {
    suspend fun latest(): UpdateManifest = withContext(Dispatchers.IO) {
        val coroutine = currentCoroutineContext()
        val started = System.nanoTime()
        val check = { coroutine.ensureActive(); require(System.nanoTime() - started < 60_000_000_000L) { "Update check timed out" } }
        val connection = open(UpdateCheck.MANIFEST_URL, "update.json", check)
        try {
            require(connection.contentLengthLong <= UpdateManifest.MAX_BYTES) { "Manifest too large" }
            val output = ByteArrayOutputStream()
            connection.inputStream.use { input ->
                val buffer = ByteArray(8192)
                while (true) {
                    check()
                    val count = input.read(buffer)
                    if (count < 0) break
                    require(output.size() + count <= UpdateManifest.MAX_BYTES) { "Manifest too large" }
                    output.write(buffer, 0, count)
                }
            }
            check()
            UpdateManifest.parse(output.toString("UTF-8"))
        } finally { connection.disconnect() }
    }

    /** The temporary file is private and is removed for every unsuccessful attempt. */
    suspend fun download(release: UpdateManifest, destination: File, progress: (Long) -> Unit) = withContext(Dispatchers.IO) {
        val coroutine = currentCoroutineContext()
        val started = System.nanoTime()
        val check = { coroutine.ensureActive(); require(System.nanoTime() - started < 900_000_000_000L) { "Download timed out" } }
        val partial = File(destination.parentFile, "${destination.name}.part")
        try {
            val connection = open(release.asset.url, release.asset.name, check)
            try {
                require(connection.contentLengthLong < 0 || connection.contentLengthLong == release.asset.size) { "Unexpected package size" }
                connection.inputStream.use { input ->
                    partial.outputStream().use { output -> UpdatePayload.copyVerified(input, output, release.asset, check, progress) }
                }
            } finally { connection.disconnect() }
            check()
            require(partial.renameTo(destination)) { "Cannot prepare the downloaded package" }
        } finally { partial.delete() }
    }

    private fun open(url: String, assetName: String, check: () -> Unit): HttpURLConnection {
        var target = url
        repeat(6) {
            check()
            require(UpdateTransportPolicy.allowed(target, assetName)) { "Untrusted update redirect" }
            val connection = (URL(target).openConnection() as HttpURLConnection).apply {
                instanceFollowRedirects = false
                connectTimeout = 10_000
                readTimeout = 15_000
                setRequestProperty("User-Agent", UpdateCheck.USER_AGENT)
                setRequestProperty("Accept-Encoding", "identity")
            }
            try {
                val code = connection.responseCode
                if (code == HttpURLConnection.HTTP_OK) return connection
                require(code in listOf(301, 302, 303, 307, 308)) { "Update unavailable (HTTP $code)" }
                val location = requireNotNull(connection.getHeaderField("Location")) { "Missing update redirect" }
                target = URL(URL(target), location).toString()
            } catch (error: Exception) {
                connection.disconnect()
                throw error
            }
            connection.disconnect()
        }
        throw IllegalArgumentException("Too many update redirects")
    }
}
