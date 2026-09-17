package dev.pepotech.pepomote.control

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * La tabla de plantillas de consola es una copia a mano de `pmp::retro`: se
 * compara campo a campo con `protocol/retro-layouts.json` (generado desde
 * Rust), bits incluidos, y se comprueban sus invariantes y la elección.
 */
class RetroLayoutsTest {

    private fun json(): JSONObject {
        val file = listOf("../protocol/retro-layouts.json", "../../protocol/retro-layouts.json", "protocol/retro-layouts.json")
            .map(::File).first { it.exists() }
        return JSONObject(file.readText())
    }

    @Test
    fun laTablaCoincideConElJsonDeRust() {
        val j = json()
        assertEquals(1, j.getInt("version"))
        val bits = j.getJSONObject("bits")
        for ((name, value) in RetroLayouts.BIT_NAMES) {
            assertEquals("bit $name", bits.getInt(name), value)
        }
        assertEquals(bits.length(), RetroLayouts.BIT_NAMES.size)
        val consoles = j.getJSONArray("console_ids")
        assertEquals(RetroLayouts.CONSOLE_IDS, (0 until consoles.length()).map { consoles.getString(it) })
        val layouts = j.getJSONArray("layouts")
        assertEquals("mismas plantillas, mismo orden", RetroLayouts.ALL.size, layouts.length())
        for (i in 0 until layouts.length()) {
            val l = layouts.getJSONObject(i)
            val k = RetroLayouts.ALL[i]
            val id = l.getString("id")
            assertEquals(id, k.id)
            assertEquals(id, l.getString("name"), k.name)
            assertEquals(id, l.getString("shape"), k.shape.id)
            assertEquals(id, l.getInt("stagger"), k.stagger)
            val face = l.getJSONArray("face")
            assertEquals(id, face.length(), k.face.size)
            for (f in 0 until face.length()) {
                val fj = face.getJSONObject(f)
                val fk = k.face[f]
                assertEquals("$id/${fk.slot}", fj.getString("slot"), fk.slot)
                assertEquals("$id/${fk.slot}", fj.getString("label"), fk.label)
                assertEquals("$id/${fk.slot} bit", fj.getString("bit"), RetroLayouts.bitName(fk.bit))
                assertEquals("$id/${fk.slot} color", fj.getString("color"), fk.color.id)
                assertEquals("$id/${fk.slot} primary", fj.getBoolean("primary"), fk.primary)
                assertEquals("$id/${fk.slot} size", fj.getDouble("size"), fk.size.toDouble(), 1e-6)
            }
            assertEquals(id, l.getBoolean("left_stick"), k.leftStick)
            assertEquals(id, l.getString("right_stick"), k.rightStick.id)
            assertEquals(id, l.getBoolean("stick_clicks"), k.stickClicks)
            val sh = l.getJSONObject("shoulders")
            fun opt(key: String) = if (sh.isNull(key)) null else sh.getString(key)
            assertEquals(id, opt("l"), k.shoulders.l)
            assertEquals(id, opt("r"), k.shoulders.r)
            assertEquals(id, opt("l2"), k.shoulders.l2)
            assertEquals(id, opt("r2"), k.shoulders.r2)
            val center = l.getJSONArray("center")
            assertEquals(id, center.length(), k.center.size)
            for (c in 0 until center.length()) {
                assertEquals(id, center.getJSONObject(c).getString("label"), k.center[c].label)
                assertEquals(id, center.getJSONObject(c).getString("bit"), RetroLayouts.bitName(k.center[c].bit))
            }
        }
    }

    @Test
    fun cadaPlantillaEsCoherente() {
        val ids = RetroLayouts.ALL.map { it.id }
        assertEquals(ids, ids.distinct())
        assertEquals("retropad", RetroLayouts.ALL.first().id)
        assertTrue(RetroLayouts.CONSOLE_IDS.all { RetroLayouts.byId(it) != null })
        assertTrue(!RetroLayouts.isConsole("retropad") && !RetroLayouts.isConsole("md3"))
        for (l in RetroLayouts.ALL) {
            assertEquals(l.id, l.shape.slots.size, l.face.size)
            assertEquals(l.id, l.shape.slots.toSet(), l.face.map { it.slot }.toSet())
            assertEquals("${l.id}: bits repetidos", l.face.size, l.face.map { it.bit }.distinct().size)
            assertTrue("${l.id}: más de un primario", l.face.count { it.primary } <= 1)
            assertTrue("${l.id}: clics sin sticks", !l.stickClicks || (l.leftStick && l.rightStick == RightStick.ANALOG))
            assertTrue("${l.id}: stagger solo en dos", l.shape == RetroShape.TWO || l.stagger == 0)
            assertTrue("${l.id}: centro", l.center.size in 1..2 && l.center.all { it.bit == ButtonState.PLUS || it.bit == ButtonState.MINUS })
            assertTrue(l.face.all { RetroLayouts.bitName(it.bit) != null })
        }
    }

    @Test
    fun laPlantillaEfectiva() {
        assertEquals("retropad", RetroLayouts.effective(null, null))
        assertEquals("md", RetroLayouts.effective("md", null))
        assertEquals("md", RetroLayouts.effective("md", "auto"))
        assertEquals("nes", RetroLayouts.effective("md", "nes"))
        assertEquals("md3", RetroLayouts.effective("md", "md3"))
        assertEquals("retropad", RetroLayouts.effective("md", "retropad"))
        assertEquals("retropad", RetroLayouts.effective("dos", null))
        assertEquals("retropad", RetroLayouts.effective("md3", null))
        assertNull(RetroLayouts.byId("bogus"))
    }
}
