package dev.pepotech.pepomote.server.retro

import dev.pepotech.pepomote.server.core.ReceiverInput
import org.junit.Assert.*
import org.junit.Test

/** Espejo de los tests del planificador en `desktop/src/retroarch/link.rs`. */
class RetroSchedulerTest {
    private fun st(buttons: Int, axes: List<Int> = PadState.ZERO_AXES) = PadState(buttons, axes)

    private fun one(s: RetroScheduler): Triple<Int, Control, Int>? {
        val f = s.frame()
        assertTrue("un datagrama por jugador", f.size <= 1)
        return f.firstOrNull()
    }

    private fun packet(buttons: Int) = ReceiverInput(1, 1, 0, 0f, 0f, 0f, 0f, 0f, 0f, buttons, 0, 50, 0, 0, 0, 0, RetroMapping.FLAG_STICK_VALID)

    @Test fun unBotonPorFotogramaYLosFlancosEnOrden() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        assertNull("en reposo no hay nada que mandar", one(s))
        s.push(0, st((1 shl 8) or (1 shl 0)))
        val a = one(s)!!; val b = one(s)!!
        assertEquals(Control.button(0) to 1, a.second to a.third)
        assertEquals(Control.button(8) to 1, b.second to b.third)
        // Ya nada pendiente: refresco rotatorio de lo pulsado
        val r1 = one(s)!!; val r2 = one(s)!!
        assertEquals(Control.button(0), r1.second)
        assertEquals(Control.button(8), r2.second)
        assertEquals(1, r1.third)
    }

    @Test fun unaPulsacionBreveNoSePierde() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        s.push(0, st(1 shl 8))
        s.push(0, st(0))
        assertEquals(Control.button(8) to 1, one(s)!!.let { it.second to it.third })
        assertEquals(Control.button(8) to 0, one(s)!!.let { it.second to it.third })
        assertNull(one(s))
    }

    @Test fun losEjesSeCoalescenYVaPrimeroElQueMasCambia() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        s.push(0, st(0, listOf(1000, 0, 0, 0)))
        s.push(0, st(0, listOf(2000, -30000, 0, 0)))
        var m = one(s)!!
        assertEquals("el que más se aleja de lo enviado", Control.axis(1) to -30000, m.second to m.third)
        m = one(s)!!
        assertEquals("el valor final, no el intermedio", Control.axis(0) to 2000, m.second to m.third)
        s.push(0, st(0))
        assertEquals(Control.axis(1), one(s)!!.second)
        assertEquals(Control.axis(0), one(s)!!.second)
        assertNull(one(s))
    }

    @Test fun losBotonesVanAntesQueLosEjes() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        s.push(0, st(0, listOf(5000, 0, 0, 0)))
        s.push(0, st(1 shl 3, listOf(5000, 0, 0, 0)))
        assertEquals(Control.button(3), one(s)!!.second)
        assertEquals(Control.axis(0), one(s)!!.second)
    }

    @Test fun elRefrescoRecomponeUnMandoVaciado() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        s.push(0, st(1 shl 4, listOf(0, 0, 700, 0)))
        assertEquals(Control.button(4), one(s)!!.second)
        assertEquals(Control.axis(2), one(s)!!.second)
        val seen = mutableSetOf<Control>()
        repeat(6) { val m = one(s)!!; seen += m.second; assertTrue(m.third != 0) }
        assertEquals(2, seen.size)
    }

    @Test fun resincronizarReenviaSoloLoQueNoEstaACero() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        s.push(0, st((1 shl 0) or (1 shl 1), listOf(0, 100, 0, 0)))
        one(s); one(s); one(s)
        s.push(0, st(1 shl 1, listOf(0, 100, 0, 0)))
        one(s)
        s.resync()
        val a = one(s)!!; val b = one(s)!!
        assertEquals(Control.button(1) to 1, a.second to a.third)
        assertEquals(Control.axis(1) to 100, b.second to b.third)
        repeat(4) { assertTrue("el botón 0 (a cero) no se reenvía", one(s)!!.third != 0) }
    }

    @Test fun alIrseElMovilSeSueltaTodoYLuegoSeCalla() {
        val s = RetroScheduler()
        s.setPresent(1, true)
        s.push(1, st(1 shl 8, listOf(0, 0, 0, -20000)))
        one(s); one(s)
        s.setPresent(1, false)
        val a = one(s)!!; val b = one(s)!!
        assertEquals(1, a.first)
        assertEquals(Control.button(8) to 0, a.second to a.third)
        assertEquals(Control.axis(3) to 0, b.second to b.third)
        assertNull("ya a cero y sin móvil: silencio", one(s))
        assertFalse(s.isActive(1))
    }

    @Test fun cadaJugadorSuPuertoYSuDatagrama() {
        val s = RetroScheduler()
        s.setPresent(0, true); s.setPresent(2, true)
        s.push(0, st(1 shl 0)); s.push(2, st(1 shl 9))
        val f = s.frame()
        assertEquals(2, f.size)
        assertEquals(0, f[0].first); assertEquals(2, f[1].first)
    }

    @Test fun aporrearMasRapidoQueLosFotogramasSeCompacta() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        for (i in 0..200) s.push(0, st((i % 2) shl 8))
        assertTrue(s.queuedEvents(0) <= 64)
        while (s.queuedEvents(0) > 0) assertNotNull(one(s))
        assertEquals(0, s.known(0).buttons)
        assertNull("todo a cero: silencio", one(s))
    }

    @Test fun homeAbreElMenuUnaVezYCapturarMantieneElAvanceRapido() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        s.pushInput(0, RetroPadKind.RetroPad, packet(RetroMapping.BTN_HOME))
        s.pushInput(0, RetroPadKind.RetroPad, packet(RetroMapping.BTN_HOME))
        assertEquals(listOf("MENU_TOGGLE"), s.takePulses())
        s.pushInput(0, RetroPadKind.RetroPad, packet(0))
        s.pushInput(0, RetroPadKind.RetroPad, packet(RetroMapping.BTN_HOME))
        assertEquals(listOf("MENU_TOGGLE"), s.takePulses())
        assertTrue(s.heldCommands().isEmpty())
        s.pushInput(0, RetroPadKind.RetroPad, packet(RetroMapping.BTN_SCREEN))
        assertEquals(listOf("FAST_FORWARD_HOLD"), s.heldCommands())
        s.pushInput(0, RetroPadKind.RetroPad, packet(0))
        assertTrue(s.heldCommands().isEmpty())
        // en el Mando Wii no hay Capturar: el bit 28 no hace nada
        s.pushInput(0, RetroPadKind.Nes, packet(RetroMapping.BTN_SCREEN))
        assertTrue(s.heldCommands().isEmpty())
        assertNull("Home y Capturar no son botones del RetroPad", one(s))
    }

    @Test fun compactarEnElUltimoFlancoConservaLaLiberacion() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        s.push(0, st(1 shl RetroPad.A))
        one(s)
        repeat(65) { i -> s.push(0, st(if (i % 2 == 0) 0 else 1 shl RetroPad.A)) }
        while (s.queuedEvents(0) > 0) one(s)
        assertEquals("el último flanco suelta A aunque coincida con el límite de cola", 0, s.known(0).buttons)
        assertNull(one(s))
    }

    @Test fun teclasRapidasPorNombre() {
        val s = RetroScheduler()
        assertTrue(s.hotkey(0, "save_state", true))
        assertTrue("soltar no manda nada", s.hotkey(0, "save_state", false))
        assertFalse(s.hotkey(0, "inventada", true))
        assertEquals(listOf("SAVE_STATE"), s.takePulses())
        assertTrue(s.hotkey(1, "rewind", true))
        assertTrue(s.hotkey(0, "fast_forward", true))
        assertEquals(listOf("FAST_FORWARD_HOLD", "REWIND"), s.heldCommands().sorted())
        s.hotkey(1, "rewind", false)
        assertEquals(listOf("FAST_FORWARD_HOLD"), s.heldCommands())
        s.setPresent(0, true); s.setPresent(0, false)
        assertTrue(s.heldCommands().isEmpty())
    }

    @Test fun elModoDegradadoSoloMandaCambios() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        s.push(0, st(1 shl 0))
        assertEquals(1, s.changes().size)
        assertTrue("sin refresco", s.changes().isEmpty())
        assertEquals("el refresco normal sí", 1, s.frame().size)
    }

    @Test fun elMandoWiiDeLadoMandaBYAPorUnoYDos() {
        val s = RetroScheduler()
        s.setPresent(0, true)
        s.pushInput(0, RetroPadKind.Nes, packet(RetroMapping.BTN_ONE))
        assertEquals(Control.button(RetroPad.B), one(s)!!.second)
    }
}
