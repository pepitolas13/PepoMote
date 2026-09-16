package dev.pepotech.pepomote.control

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * La máquina de dedos de la capa de pulsación: cada dedo lleva lo suyo, un
 * dedo descartado (su `down` se lo comió el stick o la cruceta) no pulsa
 * nada aunque pase por encima de un botón, y al acabar no queda nada
 * apretado.
 *
 * Trazado de la prueba: A en 0..100 y B en 100..200, los dos a lo alto de
 * 0..100; a partir de 200, el vacío.
 */
class SlideTrackerTest {
    private val A = 1
    private val B = 2
    private val zones = listOf(
        PressZone(A, 0f, 0f, 100f, 100f),
        PressZone(B, 100f, 0f, 200f, 100f)
    )
    private val tracker = SlideTracker { x, y -> PressHit.resolve(zones, x, y) }

    private fun down(id: Long, x: Float, slide: Boolean = true) =
        tracker.down(id, x, 50f, slide = slide, sticky = true)

    private fun move(id: Long, x: Float, slide: Boolean = true) =
        tracker.move(id, x, 50f, slide = slide, sticky = true)

    @Test fun deslizandoDeAaBySaliendoAlVacio() {
        assertEquals(listOf(PressEvent.Press(A)), down(1L, 50f))
        assertEquals(emptyList<PressEvent>(), move(1L, 60f))
        assertEquals(listOf(PressEvent.Release(A), PressEvent.Press(B)), move(1L, 150f))
        assertEquals(listOf(PressEvent.Release(B)), move(1L, 250f))
        assertEquals(emptyList<PressEvent>(), tracker.up(1L))
    }

    @Test fun unDedoQueNaceEnElVacioPulsaAlEntrar() {
        assertEquals(emptyList<PressEvent>(), down(1L, 250f))
        assertEquals(listOf(PressEvent.Press(B)), move(1L, 150f))
        assertEquals(listOf(PressEvent.Release(B)), tracker.up(1L))
    }

    @Test fun dosDedosCadaUnoConLoSuyo() {
        assertEquals(listOf(PressEvent.Press(A)), down(1L, 50f))
        assertEquals(listOf(PressEvent.Press(B)), down(2L, 150f))
        // el segundo dedo vuelve al vacío: suelta lo suyo y deja A en paz
        assertEquals(listOf(PressEvent.Release(B)), move(2L, 250f))
        assertEquals(emptyList<PressEvent>(), tracker.up(2L))
        assertEquals(listOf(PressEvent.Release(A)), tracker.up(1L))
    }

    @Test fun unBotonQueLlevaOtroDedoNoSeLeQuita() {
        assertEquals(listOf(PressEvent.Press(A)), down(1L, 50f))
        // el segundo entra en A: ya está pulsado y es del primero
        assertEquals(emptyList<PressEvent>(), down(2L, 60f))
        // al levantar el segundo, A sigue pulsado por el primero
        assertEquals(emptyList<PressEvent>(), tracker.up(2L))
        assertEquals(listOf(PressEvent.Release(A)), tracker.up(1L))
    }

    @Test fun elDedoDescartadoNoPulsaNada() {
        // su `down` se lo comió el stick, la cruceta, un chip…
        assertEquals(emptyList<PressEvent>(), tracker.skip(1L))
        assertEquals(emptyList<PressEvent>(), move(1L, 50f))
        assertEquals(emptyList<PressEvent>(), move(1L, 150f))
        assertEquals(emptyList<PressEvent>(), tracker.up(1L))
    }

    @Test fun siOtroControlSeQuedaElDedoAMediasSueltaLoQueLlevaba() {
        // la tira de scroll no se come el `down`, solo el arrastre
        assertEquals(listOf(PressEvent.Press(A)), down(1L, 50f))
        assertEquals(listOf(PressEvent.Release(A)), tracker.skip(1L))
        assertEquals(emptyList<PressEvent>(), move(1L, 150f))
        assertEquals(emptyList<PressEvent>(), tracker.up(1L))
    }

    @Test fun sinDeslizarLaCapaNoPulsaNada() {
        // sin el ajuste, cada botón se apaña con su propio gesto
        assertEquals(emptyList<PressEvent>(), down(1L, 50f, slide = false))
        assertEquals(emptyList<PressEvent>(), move(1L, 150f, slide = false))
        assertEquals(emptyList<PressEvent>(), tracker.up(1L))
    }

    @Test fun alAcabarNoQuedaNadaApretado() {
        down(1L, 50f)
        down(2L, 150f)
        assertEquals(
            listOf(PressEvent.Release(A), PressEvent.Release(B)),
            tracker.releaseAll().sortedBy { (it as PressEvent.Release).bit }
        )
        // y ya no queda ningún dedo vivo
        assertEquals(emptyList<PressEvent>(), tracker.up(1L))
        assertEquals(emptyList<PressEvent>(), tracker.releaseAll())
    }
}
