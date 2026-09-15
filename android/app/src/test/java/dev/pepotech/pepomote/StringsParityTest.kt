package dev.pepotech.pepomote

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * values/strings.xml (español) y values-en/strings.xml (inglés) tienen las
 * mismas claves y los mismos huecos (%1$s…): ninguna pantalla se queda a
 * medias en un idioma.
 */
class StringsParityTest {

    @Test
    fun switchControlsHaveBilingualNames() {
        for (locale in listOf("values", "values-en")) {
            val labels = strings("src/main/res/$locale/strings.xml")
            for (key in listOf("channel_switch", "mode_switch", "capture", "pro_controller", "warn_needs_switch", "kb_switch_title", "kb_switch_help")) {
                assertTrue("$locale/$key", labels[key]?.isNotBlank() == true)
            }
            assertEquals("Pro Controller", labels["channel_switch_sub"])
            assertTrue("Removed controller options have no labels", labels.keys.none { it.startsWith("joycon") })
        }
    }

    private fun strings(path: String): Map<String, String> {
        val file = listOf(File(path), File("app/$path")).first { it.exists() }
        val re = Regex("<string name=\"([^\"]+)\">(.*?)</string>", RegexOption.DOT_MATCHES_ALL)
        return re.findAll(file.readText()).associate { it.groupValues[1] to it.groupValues[2] }
    }

    private fun holes(s: String): List<String> = Regex("%\\d+\\$[sd]").findAll(s).map { it.value }.sorted().toList()

    @Test
    fun mismasClavesEnLosDosIdiomas() {
        val es = strings("src/main/res/values/strings.xml")
        val en = strings("src/main/res/values-en/strings.xml")
        assertTrue("hay textos", es.size > 100)
        assertEquals("claves que faltan en inglés", emptySet<String>(), es.keys - en.keys)
        assertEquals("claves que sobran en inglés", emptySet<String>(), en.keys - es.keys)
        for ((k, v) in es) {
            assertEquals("huecos de $k", holes(v), holes(en.getValue(k)))
            assertTrue("$k vacío", v.isNotBlank() && en.getValue(k).isNotBlank())
        }
    }
}
