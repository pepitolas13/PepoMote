package dev.pepotech.pepomote.control

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Tabla de verdad de los dos ajustes de pulsación (la misma en las tres
 * apps): deslizando manda lo que hay bajo el dedo; sin deslizar, «mantener»
 * no suelta nunca hasta levantar y, sin mantener, salirse suelta y el dedo se
 * queda libre para siempre.
 */
class PressStepTest {
    private val A = 1
    private val B = 2

    @Test fun deslizandoElDedoCogeLoQuePisa() {
        // nace en el vacío y entra en un botón
        assertEquals(PressStep.Move.To(A), PressStep.next(slide = true, sticky = true, held = null, over = A))
        // pasa de un botón al de al lado
        assertEquals(PressStep.Move.To(B), PressStep.next(slide = true, sticky = true, held = A, over = B))
        // sale al vacío: suelta y no coge nada
        assertEquals(PressStep.Move.To(null), PressStep.next(slide = true, sticky = true, held = A, over = null))
    }

    @Test fun deslizandoSobreElMismoBotonNoHaceNada() {
        assertEquals(PressStep.Move.Keep, PressStep.next(slide = true, sticky = true, held = A, over = A))
        assertEquals(PressStep.Move.Keep, PressStep.next(slide = true, sticky = false, held = null, over = null))
    }

    @Test fun deslizandoMandaAunqueMantenerEsteApagado() {
        assertEquals(PressStep.Move.To(B), PressStep.next(slide = true, sticky = false, held = A, over = B))
        assertEquals(PressStep.Move.To(null), PressStep.next(slide = true, sticky = false, held = A, over = null))
    }

    @Test fun manteniendoNoSeSueltaNadaAlSalirse() {
        for (over in listOf(null, A, B)) {
            assertEquals(PressStep.Move.Keep, PressStep.next(slide = false, sticky = true, held = A, over = over))
        }
        assertEquals(PressStep.Move.Keep, PressStep.next(slide = false, sticky = true, held = null, over = A))
    }

    @Test fun sinMantenerSalirseSueltaYElDedoSeQuedaLibre() {
        assertEquals(PressStep.Move.To(null), PressStep.next(slide = false, sticky = false, held = A, over = null))
        assertEquals(PressStep.Move.To(null), PressStep.next(slide = false, sticky = false, held = A, over = B))
        // sigue dentro: nada que hacer
        assertEquals(PressStep.Move.Keep, PressStep.next(slide = false, sticky = false, held = A, over = A))
        // ya libre: no vuelve a coger nada
        assertEquals(PressStep.Move.Keep, PressStep.next(slide = false, sticky = false, held = null, over = B))
    }
}
