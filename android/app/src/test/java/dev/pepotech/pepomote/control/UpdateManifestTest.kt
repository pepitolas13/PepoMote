package dev.pepotech.pepomote.control

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class UpdateManifestTest {
    private fun manifest() = JSONObject("""{
      "schema":1,"version":"1.11.0","published_at":"2026-09-19T12:00:00Z",
      "notes":{"es":["Mejor conexión"],"en":["Better connection"]},
      "release_url":"https://github.com/pepitolas13/PepoMote/releases/tag/v1.11.0",
      "assets":{"android-universal":{"name":"PepoMote.apk","url":"https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.apk","size":1234,"sha256":"${"a".repeat(64)}"}}
    }""")
    private fun rejects(edit: (JSONObject) -> Unit) {
        val json = manifest().apply(edit)
        assertThrows(IllegalArgumentException::class.java) { UpdateManifest.parse(json.toString()) }
    }
    @Test fun acceptsCompleteReleaseWithLocalizedNotes() {
        val release = UpdateManifest.parse(manifest().toString())
        assertEquals(UpdateCheck.Version(1,11,0), release.version)
        assertEquals(listOf("Mejor conexión"), release.notesFor("es"))
        assertEquals(listOf("Better connection"), release.notesFor("fr"))
        assertEquals(1234L, release.asset.size)
    }
    @Test fun rejectsWrongRepositoryTagAssetAndNonHttps() {
        for (bad in listOf("http://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.apk",
            "https://github.com/evil/PepoMote/releases/download/v1.11.0/PepoMote.apk",
            "https://github.com/pepitolas13/PepoMote/releases/download/v1.10.0/PepoMote.apk",
            "https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/PepoMote.apk?x=1")) {
            rejects { it.getJSONObject("assets").getJSONObject("android-universal").put("url", bad) }
        }
        rejects { it.put("release_url", "https://example.com/v1.11.0") }
    }
    @Test fun rejectsInvalidSchemaBoundsAndUnstableVersion() {
        rejects { it.put("schema", 2) }
        rejects { it.put("schema", "1") }
        for (v in listOf("1.11", "v1.11.0", "1.11.0-beta", "01.11.0", "١.١١.٠")) rejects { it.put("version", v) }
        rejects { it.put("published_at", "today") }
        rejects { it.getJSONObject("assets").getJSONObject("android-universal").put("size", -1) }
        rejects { it.getJSONObject("assets").getJSONObject("android-universal").put("size", 1.5) }
        rejects { it.getJSONObject("assets").getJSONObject("android-universal").put("sha256", "oops") }
        rejects { it.getJSONObject("notes").put("es", listOf("x".repeat(281))) }
        rejects { it.getJSONObject("notes").put("en", List(9) { "note" }) }
        rejects { it.getJSONObject("notes").put("es", listOf("line\nline")) }
        assertThrows(IllegalArgumentException::class.java) { UpdateManifest.parse(" ".repeat(262145)) }
    }
    @Test fun rejectsTrailingPayloadAndExtremeNestingBeforeParsing() {
        assertThrows(IllegalArgumentException::class.java) { UpdateManifest.parse(manifest().toString() + " ignored") }
        assertThrows(IllegalArgumentException::class.java) { UpdateManifest.parse("[".repeat(2000) + "0" + "]".repeat(2000)) }
    }
    @Test fun accepts280UnicodeCodePointsRatherThanOnly280Utf16Units() {
        val json = manifest().apply { getJSONObject("notes").put("es", listOf("😀".repeat(280))) }
        assertEquals("😀".repeat(280), UpdateManifest.parse(json.toString()).notesFor("es").single())
        rejects { it.getJSONObject("notes").put("es", listOf("😀".repeat(281))) }
    }
}
