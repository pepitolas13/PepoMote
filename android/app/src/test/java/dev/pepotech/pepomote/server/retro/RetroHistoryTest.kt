package dev.pepotech.pepomote.server.retro

import dev.pepotech.pepomote.control.RetroLayouts
import org.junit.Assert.*
import org.junit.Test

/** Espejo de los tests de `desktop/src/retroarch/{consoles,history}.rs`. */
class RetroHistoryTest {
    @Test fun nombresDelProtocolo() {
        assertEquals(12, Console.entries.size)
        for (c in Console.entries) {
            assertEquals(c, Console.parse(c.id))
            assertTrue(c.displayName.isNotEmpty())
            assertTrue("${c.id} no está en RetroLayouts", c.id in RetroLayouts.CONSOLE_IDS)
            assertEquals(c.displayName, RetroLayouts.byId(c.id)!!.name)
        }
        assertEquals(RetroLayouts.CONSOLE_IDS.size, Console.entries.size)
        assertNull(Console.parse("snes9x"))
    }

    @Test fun porSystemid() {
        assertEquals(Console.Snes, RetroConsoles.consoleFor("super_nes", "snes9x_libretro.dll", "juego.zip", null))
        assertEquals(Console.Psx, RetroConsoles.consoleFor("playstation", "x.dll", "juego.cue", null))
        assertNull(RetroConsoles.consoleFor("dos", "dosbox_pure_libretro.dll", "juego.zip", null))
    }

    @Test fun porNombreDeNucleoSinFicha() {
        assertEquals(Console.Snes, RetroConsoles.byCoreName("mesen-s_libretro.dll"))
        assertEquals(Console.Nes, RetroConsoles.byCoreName("mesen_libretro.dll"))
        assertEquals(Console.NeoGeo, RetroConsoles.byCoreName("fbneo_neogeo_libretro.so"))
        assertEquals(Console.Arcade, RetroConsoles.byCoreName("mame2003_plus_libretro.dll"))
        assertEquals(Console.Atari2600, RetroConsoles.byCoreName("/opt/x/stella2014_libretro.dylib"))
        assertNull(RetroConsoles.byCoreName("dosbox_pure_libretro.dll"))
        assertEquals("genesis_plus_gx", RetroConsoles.coreStem("C:\\RetroArch\\cores\\genesis_plus_gx_libretro.dll"))
        assertEquals("mgba", RetroConsoles.coreStem("/data/user/0/com.retroarch.aarch64/cores/mgba_libretro_android.so"))
        assertEquals(Console.Md, RetroConsoles.consoleFor(null, "C:\\x\\picodrive_libretro.dll", "juego.zip", null))
        assertEquals(Console.NeoGeo, RetroConsoles.consoleFor("fb_alpha", "fbneo_neogeo_libretro.dll", "mslug.zip", null))
    }

    @Test fun laExtensionMandaEnNucleosMultisistema() {
        val gpgx = "genesis_plus_gx_libretro.dll"
        assertEquals(Console.Ms, RetroConsoles.consoleFor("mega_drive", gpgx, "C:\\roms\\sonic.sms", null))
        assertEquals(Console.Ms, RetroConsoles.consoleFor("mega_drive", gpgx, "sonic.GG", null))
        assertEquals(Console.Md, RetroConsoles.consoleFor("mega_drive", gpgx, "sonic.md", null))
        assertEquals(Console.Md, RetroConsoles.consoleFor("mega_drive", gpgx, "cave_story.zip", null))
        assertEquals(Console.Md, RetroConsoles.consoleFor(null, gpgx, "juego.cue", null))
        assertEquals(Console.Gb, RetroConsoles.consoleFor("game_boy_advance", "mgba_libretro.dll", "tetris.gb", null))
        assertEquals(Console.Gba, RetroConsoles.consoleFor("game_boy_advance", "mgba_libretro.dll", "tetris.gba", null))
        assertEquals(Console.Gb, RetroConsoles.consoleFor("super_nes", "x.dll", "tetris.gb", null))
        assertEquals(Console.Nes, RetroConsoles.consoleFor(null, "raro.dll", "zelda.nes", null))
        assertNull(RetroConsoles.consoleFor(null, "raro.dll", "juego.bin", null))
    }

    @Test fun miembroDeArchivoTrasAlmohadilla() {
        assertEquals("Sonic (Europe).md", RetroConsoles.contentFileName("C:\\roms\\pack.zip#Sonic (Europe).md"))
        assertEquals("md", RetroConsoles.contentExt("pack.zip#Sonic (Europe).md"))
        assertEquals("md", RetroConsoles.contentExt("/storage/emulated/0/roms/Sonic.MD"))
        assertNull(RetroConsoles.contentExt("sin_extension"))
        assertEquals(Console.Md, RetroConsoles.consoleFor("mega_drive", "genesis_plus_gx_libretro.dll", "pack.7z#rom.bin", null))
        assertEquals(Console.Gb, RetroConsoles.consoleFor(null, "x.dll", "pack.zip#Tetris.gb", null))
    }

    @Test fun dbNameUnicoComoPista() {
        assertEquals(Console.Md, RetroConsoles.byDbName("Sega - Mega Drive - Genesis.lpl"))
        assertEquals(Console.Gba, RetroConsoles.byDbName("Nintendo - Game Boy Advance.lpl"))
        assertEquals(Console.Gb, RetroConsoles.byDbName("Nintendo - Game Boy.lpl"))
        assertNull("varias: no decide", RetroConsoles.byDbName("Sega - Game Gear|Sega - Master System - Mark III"))
        assertNull(RetroConsoles.byDbName(""))
        assertEquals(Console.Ms, RetroConsoles.consoleFor("mega_drive", "genesis_plus_gx_libretro.dll", "juego.bin", "Sega - Master System - Mark III.lpl"))
    }

    private val caveStory = """{
  "version": "1.5",
  "default_core_path": "",
  "default_core_name": "",
  "items": [
    {
      "path": "/storage/emulated/0/RetroArch/roms/cave_story_v0.7.0.zip",
      "label": "",
      "core_path": "/data/user/0/com.retroarch.aarch64/cores/genesis_plus_gx_libretro_android.so",
      "core_name": "Sega - MS/GG/MD/CD (Genesis Plus GX)",
      "crc32": "00000000|crc",
      "db_name": "Sega - Mega Drive - Genesis.lpl|Sega - Master System - Mark III.lpl"
    },
    { "path": "/x/zelda.nes", "label": "Zelda", "core_path": "/x/fceumm_libretro_android.so", "core_name": "Nintendo - NES / Famicom (FCEUmm)", "db_name": "" }
  ]
}"""

    @Test fun laEntradaMasRecienteDelHistorial() {
        val top = RetroHistory.parseTop(caveStory) as HistoryTop.Entry
        assertEquals("/storage/emulated/0/RetroArch/roms/cave_story_v0.7.0.zip", top.entry.path)
        assertEquals("Sega - MS/GG/MD/CD (Genesis Plus GX)", top.entry.coreName)
        assertEquals(HistoryTop.Empty, RetroHistory.parseTop("""{"version":"1.5","items":[]}"""))
        assertEquals(HistoryTop.Empty, RetroHistory.parseTop("{}"))
        assertEquals(HistoryTop.Broken, RetroHistory.parseTop(caveStory.substring(0, 200)))
        assertEquals(HistoryTop.Broken, RetroHistory.parseTop(""))
        assertEquals(HistoryTop.Broken, RetroHistory.parseTop("basura"))
    }

    @Test fun tituloNombreCortoYFicha() {
        assertEquals("Genesis Plus GX", RetroHistory.shortCoreName("Sega - MS/GG/MD/CD (Genesis Plus GX)"))
        assertEquals("Snes9x", RetroHistory.shortCoreName("Snes9x"))
        assertEquals("Cave Story", RetroHistory.titleFor("Cave Story (USA) [!]", "x"))
        assertEquals("cave_story_v0.7.0", RetroHistory.titleFor("", "/roms/cave_story_v0.7.0.zip"))
        assertEquals("Tetris", RetroHistory.titleFor("", "pack.zip#Tetris (World).gb"))
        assertEquals("(2020)", RetroHistory.titleFor("(2020)", "x"))
        val game = RetroHistory.gameFrom((RetroHistory.parseTop(caveStory) as HistoryTop.Entry).entry)
        assertEquals(Console.Md, game.console)
        assertEquals("Mega Drive", game.system)
        assertEquals("Genesis Plus GX", game.core)
        assertEquals("cave_story_v0.7.0", game.title)
        val dos = RetroHistory.gameFrom(HistoryEntry("/x/game.zip", "Doom", "/x/dosbox_pure_libretro_android.so", "DOS (DOSBox-Pure)", ""))
        assertNull(dos.console)
        assertEquals("", dos.system)
        assertEquals("DOSBox-Pure", dos.core)
        val msg = RetroHistory.gameMessage(game)
        assertEquals("game", msg.getString("m"))
        assertEquals("md", msg.getString("console"))
        assertEquals("cave_story_v0.7.0", msg.getString("title"))
        val none = RetroHistory.gameMessage(null)
        assertTrue(none.isNull("console"))
        assertEquals("", none.getString("path"))
        assertEquals(setOf("m", "console", "system", "core", "title", "path"), none.keys().asSequence().toSet())
    }

    private class FakeDoc(var stamp: Pair<Long, Long>?, var text: String?) : HistoryDocument {
        override fun stamp() = stamp
        override fun read() = text
    }

    @Test fun elVigilanteSoloAnunciaCambios() {
        var doc: FakeDoc? = null
        val watch = RetroGameWatch { doc }
        assertFalse("sin archivo, sin juego", watch.poll(false))
        assertFalse(watch.located)
        doc = FakeDoc(1L to 100L, caveStory)
        assertTrue(watch.poll(false))
        assertTrue(watch.located)
        assertEquals(Console.Md, watch.game!!.console)
        assertFalse("misma entrada, mismo sello: nada", watch.poll(false))
        doc.stamp = 2L to 100L
        assertFalse("mismo juego reescrito: no cambia", watch.poll(false))
        doc.stamp = 3L to 50L
        doc.text = caveStory.substring(0, 150)
        assertFalse("JSON a medias: se conserva lo anterior", watch.poll(false))
        assertEquals(Console.Md, watch.game!!.console)
        doc.text = """{"items":[{"path":"/x/zelda.nes","label":"Zelda","core_path":"/x/fceumm_libretro_android.so","core_name":"Nintendo - NES / Famicom (FCEUmm)","db_name":""}]}"""
        assertTrue("el JSON completo ya se lee", watch.poll(false))
        assertEquals(Console.Nes, watch.game!!.console)
        assertEquals("Zelda", watch.game!!.title)
        // RetroArch vuelve a responder: se relee aunque el sello no cambie
        doc.text = caveStory
        assertFalse(watch.poll(false))
        assertTrue(watch.poll(true))
        assertEquals(Console.Md, watch.game!!.console)
        assertFalse(watch.poll(true))
        doc.stamp = null
        assertTrue("archivo desaparecido: sin juego", watch.poll(true))
        assertNull(watch.game)
        assertFalse(watch.located)
        watch.reset()
        assertNull(watch.game)
    }
}
