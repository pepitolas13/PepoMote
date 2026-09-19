package dev.pepotech.pepomote.control

import org.json.JSONObject
import org.json.JSONTokener
import java.time.Instant

data class UpdateAsset(val name: String, val url: String, val size: Long, val sha256: String)

data class UpdateManifest(
    val version: UpdateCheck.Version,
    val releaseUrl: String,
    val notes: Map<String, List<String>>,
    val asset: UpdateAsset,
    val json: String,
) {
    fun notesFor(language: String): List<String> = notes[language].orEmpty().ifEmpty { notes["en"].orEmpty() }

    companion object {
        const val MAX_BYTES = 256 * 1024
        const val MAX_PACKAGE_BYTES = 1024L * 1024 * 1024
        private val versionPattern = Regex("(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)")
        fun parse(json: String): UpdateManifest {
            require(json.toByteArray(Charsets.UTF_8).size <= MAX_BYTES) { "Manifest too large" }
            // Android's JSON parser recurses; reject hostile nesting before calling it.
            var depth = 0
            var inString = false
            var escaped = false
            for (char in json) {
                if (inString) {
                    require(!char.isISOControl()) { "Invalid JSON string" }
                    if (escaped) escaped = false
                    else if (char == '\\') escaped = true
                    else if (char == '"') inString = false
                } else when (char) {
                    '"' -> inString = true
                    '{', '[' -> { depth++; require(depth <= 32) { "Manifest nesting too deep" } }
                    '}', ']' -> { depth--; require(depth >= 0) { "Unbalanced JSON" } }
                    else -> require(char != '/' && char != '\'' && (!char.isISOControl() || char in "\t\r\n")) { "Invalid JSON" }
                }
            }
            require(depth == 0 && !inString) { "Incomplete JSON" }
            try {
                val tokener = JSONTokener(json)
                val root = tokener.nextValue() as? JSONObject ?: throw IllegalArgumentException("Expected manifest object")
                require(tokener.nextClean() == '\u0000') { "Trailing manifest data" }
                require(root.get("schema") is Int && root.getInt("schema") == 1) { "Unsupported manifest" }
                val versionText = root.strictString("version")
                require(versionPattern.matches(versionText)) { "Invalid stable version" }
                val version = requireNotNull(UpdateCheck.parse(versionText)) { "Invalid version" }
                val date = root.strictString("published_at")
                require(Regex("[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z").matches(date)) { "Publication time must be UTC" }
                require(Instant.parse(date).toString() == date) { "Invalid publication time" }
                val release = root.strictString("release_url")
                require(release == UpdateCheck.releaseUrl(version)) { "Invalid release URL" }
                val notes = root.getJSONObject("notes")
                val localized = listOf("es", "en").associateWith { language ->
                    val bullets = notes.getJSONArray(language)
                    require(bullets.length() in 1..8) { "Invalid release notes" }
                    List(bullets.length()) { index ->
                        val note = bullets.get(index)
                        require(note is String && note.isNotBlank() && note.codePointCount(0, note.length) <= 280 &&
                            note.none { it.isISOControl() }) { "Invalid release note" }
                        note
                    }
                }
                val apk = root.getJSONObject("assets").getJSONObject("android-universal")
                val name = apk.strictString("name")
                require(name == "PepoMote.apk") { "Invalid APK name" }
                val url = apk.strictString("url")
                require(url == "https://github.com/${UpdateCheck.REPO}/releases/download/v$version/$name") { "Invalid APK URL" }
                val sizeValue = apk.get("size")
                require(sizeValue is Int || sizeValue is Long) { "Invalid APK size" }
                val size = (sizeValue as Number).toLong()
                require(size in 1..MAX_PACKAGE_BYTES) { "Invalid APK size" }
                val sha = apk.strictString("sha256")
                require(sha.matches(Regex("[a-f0-9]{64}"))) { "Invalid APK checksum" }
                return UpdateManifest(version, release, localized, UpdateAsset(name, url, size, sha), json)
            } catch (e: IllegalArgumentException) {
                throw e
            } catch (e: Exception) {
                throw IllegalArgumentException("Invalid release manifest", e)
            }
        }

        private fun JSONObject.strictString(key: String): String =
            (get(key) as? String) ?: throw IllegalArgumentException("Invalid $key")
    }
}
