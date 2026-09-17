package dev.pepotech.pepomote.service

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Elección del mando de consola por juego y global, con tope; y la ficha `game` del receptor. */
class RetroLayoutChoiceTest {

    @Test
    fun porJuegoYGlobalConTope() {
        var c = RetroLayoutChoice()
        assertNull("de serie, automático", c.choiceFor("a.zip"))
        c = c.pick(null, "md3")
        assertEquals("sin juego: global", "md3", c.choiceFor(null))
        assertEquals("la global vale para todos", "md3", c.choiceFor("a.zip"))
        c = c.pick("a.zip", "nes")
        assertEquals("el juego manda", "nes", c.choiceFor("a.zip"))
        assertEquals("md3", c.choiceFor("b.zip"))
        c = c.pick("a.zip", null)
        assertNull("automático con juego: ese juego", c.choiceFor("a.zip"))
        assertNull("y la global", c.choiceFor(null))
        for (i in 0 until 60) c = c.pick("g$i.zip", "snes")
        assertEquals("tope", RetroLayoutChoice.CAP, c.byPath.size)
        assertNull("el más viejo fuera", c.choiceFor("g0.zip"))
        assertEquals("snes", c.choiceFor("g59.zip"))
        assertEquals("ida y vuelta", c, RetroLayoutChoice.decode(c.encode()))
        assertEquals(RetroLayoutChoice(), RetroLayoutChoice.decode(null))
        assertEquals("basura: automático", RetroLayoutChoice(), RetroLayoutChoice.decode("{no"))
    }

    @Test
    fun laFichaDelJuego() {
        val g = RetroGame.parse(JSONObject("""{"m":"game","console":"md","system":"Mega Drive","core":"Genesis Plus GX","title":"Cave Story","path":"C:\\r\\cs.zip"}"""))!!
        assertEquals("md", g.console)
        assertEquals(listOf("Mega Drive", "Genesis Plus GX", "Cave Story", "C:\\r\\cs.zip"), listOf(g.system, g.core, g.title, g.path))
        val unknown = RetroGame.parse(JSONObject("""{"m":"game","console":null,"system":"","core":"dosbox_pure","title":"game","path":"g.zip"}"""))!!
        assertNull(unknown.console)
        assertNull("sin juego", RetroGame.parse(JSONObject("""{"m":"game","console":null,"system":"","core":"","title":"","path":""}""")))
    }
}
