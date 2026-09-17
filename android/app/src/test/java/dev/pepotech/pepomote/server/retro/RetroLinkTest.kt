package dev.pepotech.pepomote.server.retro

import dev.pepotech.pepomote.server.retro.FakeRetroArch.Companion.await
import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.CopyOnWriteArrayList

/** El enlace contra un RetroArch falso en UDP: dosificación, teclas, resincronización y modo degradado. */
class RetroLinkTest {
    private fun bit(b: Int) = 1 shl b

    @Test fun sincronizaYMandaUnDatagramaPorFotogramaSinColaNiGetStatus() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            val lives = CopyOnWriteArrayList<RetroLive>()
            RetroLink(ports, "PepoMote: test") { lives += it }.use { link ->
                // Un móvil ya conectado cuando RetroArch empieza a responder: aviso en su pantalla
                link.setPresent(0, true)
                link.start()
                await(message = "RetroArch responde") { link.live.reachable }
                assertEquals("1.22.2", link.live.version)
                link.push(0, PadState(bit(RetroPad.A) or bit(RetroPad.UP), listOf(0, -32767, 0, 0)))
                await(message = "A y arriba llegan al núcleo") { ra.buttons(0) == (bit(RetroPad.A) or bit(RetroPad.UP)) }
                await(message = "el eje llega") { ra.axes(0)[1] == -32767 }
                // Una pulsación breve: dos flancos, uno por fotograma
                link.push(0, PadState(bit(RetroPad.A) or bit(RetroPad.UP) or bit(RetroPad.B), listOf(0, -32767, 0, 0)))
                link.push(0, PadState(bit(RetroPad.A) or bit(RetroPad.UP), listOf(0, -32767, 0, 0)))
                link.push(0, PadState())
                await(message = "se suelta todo") { ra.buttons(0) == 0 && ra.axes(0) == listOf(0, 0, 0, 0) }
                // Teclas: mantener se repite cada fotograma; una de toque va una vez
                assertTrue(link.hotkey(0, "rewind", true))
                Thread.sleep(300)
                assertTrue(link.hotkey(0, "rewind", false))
                assertTrue(link.hotkey(0, "save_state", true))
                assertFalse(link.hotkey(0, "inventada", true))
                await(message = "SAVE_STATE llega") { ra.commands().any { it.second == "SAVE_STATE" } }
                Thread.sleep(200)
                val rewinds = ra.commands().count { it.second == "REWIND" }
                assertTrue("REWIND repetido por fotograma: $rewinds", rewinds in 8..40)
                assertEquals(1, ra.commands().count { it.second == "SAVE_STATE" })
                Thread.sleep(300)
                assertEquals("al soltar deja de repetirse", rewinds, ra.commands().count { it.second == "REWIND" })
                assertEquals("nunca dos datagramas en cola por jugador", 0, ra.doubles)
                assertEquals("GET_STATUS nunca a una 1.22.2", 0, ra.statusRequests)
                assertTrue("sondeos ≈ fotogramas/s: ${link.live.pollsPerSec}", link.live.pollsPerSec in 30f..90f)
                await(message = "SHOW_MSG al enlazar") { ra.osd().contains("PepoMote: test") }
                assertTrue(lives.any { it.reachable })
            }
        }
    }

    @Test fun getStatusSoloAVersionesPosterioresYActividad() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            ra.version = "1.23.0"
            RetroLink(ports).use { link ->
                link.start()
                await(message = "responde") { link.live.reachable }
                await(4000, "GET_STATUS tras 30 sondas") { ra.statusRequests >= 1 }
                await(message = "actividad leída") { link.live.activity == Activity.Playing("snes9x", "Test Game.sfc") }
            }
        }
    }

    @Test fun alPerderRetroArchSeResincronizaYRecomponeElMando() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            RetroLink(ports).use { link ->
                link.start()
                await(message = "responde") { link.live.reachable }
                link.setPresent(0, true)
                link.push(0, PadState(bit(RetroPad.X), listOf(1000, 0, 0, 0)))
                await(message = "X llega") { ra.buttons(0) == bit(RetroPad.X) && ra.axes(0)[0] == 1000 }
                ra.mute = true
                await(3000, "se da por perdido") { !link.live.reachable }
                assertNull(link.live.version)
                ra.clearState()
                ra.mute = false
                await(3000, "vuelve") { link.live.reachable }
                await(message = "lo pulsado se reenvía al resincronizar") { ra.buttons(0) == bit(RetroPad.X) && ra.axes(0)[0] == 1000 }
            }
        }
    }

    @Test fun cerrarElEnlaceSueltaTodosLosJugadoresAntesDeCerrarLosSockets() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            RetroLink(ports).use { link ->
                link.start()
                await { link.live.reachable }
                for (slot in 0..3) {
                    link.setPresent(slot, true)
                    link.push(slot, PadState(bit(RetroPad.A), listOf(12000, 0, 0, -15000)))
                }
                await(message = "los cuatro jugadores tienen botones y ejes pulsados") {
                    (0..3).all { ra.buttons(it) == bit(RetroPad.A) && ra.axes(it) == listOf(12000, 0, 0, -15000) }
                }
                link.close()
                await(message = "cerrar libera incluso si RetroArch conserva el estado entre fotogramas") {
                    (0..3).all { ra.buttons(it) == 0 && ra.axes(it) == PadState.ZERO_AXES }
                }
            }
        }
    }

    @Test fun volverDelSegundoPlanoNoConservaBotonesSoltadosMientrasNoRespondia() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            RetroLink(ports).use { link ->
                link.start()
                await { link.live.reachable }
                link.setPresent(0, true)
                link.push(0, PadState(bit(RetroPad.A), listOf(1000, 0, 0, 0)))
                await { ra.buttons(0) == bit(RetroPad.A) && ra.axes(0)[0] == 1000 }
                ra.mute = true
                await { !link.live.reachable }
                link.push(0, PadState())
                // Android conserva el mando al pasar al fondo: no se vacía el falso.
                ra.mute = false
                await { link.live.reachable }
                await(message = "se entregan las liberaciones pendientes al volver") {
                    ra.buttons(0) == 0 && ra.axes(0) == PadState.ZERO_AXES
                }
            }
        }
    }

    @Test fun elRefrescoRecomponeUnMandoQueSeVaciaCadaFotograma() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            ra.resetOnEmpty = true
            RetroLink(ports).use { link ->
                link.start()
                await(message = "responde") { link.live.reachable }
                link.setPresent(0, true)
                link.push(0, PadState(bit(RetroPad.B)))
                await(message = "B llega") { ra.buttons(0) == bit(RetroPad.B) }
                Thread.sleep(300)
                assertEquals("con un solo control vivo, el refresco lo manda cada fotograma", bit(RetroPad.B), ra.buttons(0))
            }
        }
    }

    @Test fun sinRespuestaConMovilesElModoDegradadoMandaACiegas() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            ra.mute = true
            RetroLink(ports).use { link ->
                link.start()
                link.setPresent(1, true)
                await(7000, "modo degradado") { link.live.degraded }
                assertFalse(link.live.reachable)
                link.push(1, PadState(bit(RetroPad.START)))
                await(message = "el cambio llega a ciegas al jugador 2") { ra.buttons(1) == bit(RetroPad.START) }
                assertEquals(0, ra.buttons(0))
                ra.mute = false
                await(3000, "al responder, fin del degradado") { link.live.reachable && !link.live.degraded }
            }
        }
    }

    @Test fun cadaJugadorSuPuertoYAlIrseSeSuelta() {
        val ports = FakeRetroArch.freePorts()
        FakeRetroArch(ports).start().use { ra ->
            RetroLink(ports).use { link ->
                link.start()
                await(message = "responde") { link.live.reachable }
                link.setPresent(0, true); link.setPresent(2, true)
                link.push(0, PadState(bit(RetroPad.L)))
                link.push(2, PadState(bit(RetroPad.R)))
                await(message = "cada uno en su puerto") { ra.buttons(0) == bit(RetroPad.L) && ra.buttons(2) == bit(RetroPad.R) }
                assertEquals(0, ra.buttons(1))
                link.setPresent(2, false)
                await(message = "el que se va suelta") { ra.buttons(2) == 0 }
                assertEquals(bit(RetroPad.L), ra.buttons(0))
            }
        }
    }
}
