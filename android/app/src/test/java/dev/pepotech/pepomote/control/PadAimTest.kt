package dev.pepotech.pepomote.control

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Las dos decisiones del giro del mando universal (puras, sin Android). */
class PadAimTest {

    @Test fun elGiroSoloSeApagaConElMandoUniversalConfirmado() {
        // Sin contestar, y contestando que no, el mando universal va sin giro
        assertTrue(PadAim.motionOff(universalPad = true, operative = true, pref = AppPrefs.PAD_AIM_ASK))
        assertTrue(PadAim.motionOff(universalPad = true, operative = true, pref = AppPrefs.PAD_AIM_OFF))
        assertFalse("con giro, lo de siempre", PadAim.motionOff(universalPad = true, operative = true, pref = AppPrefs.PAD_AIM_ON))
        // Sin el eco del modo los paquetes alimentan lo de antes: no se tocan
        assertFalse(PadAim.motionOff(universalPad = true, operative = false, pref = AppPrefs.PAD_AIM_OFF))
        // Switch y RetroArch comparten pantalla y formato, y apuntan con el giro
        for (pref in listOf(AppPrefs.PAD_AIM_ASK, AppPrefs.PAD_AIM_OFF, AppPrefs.PAD_AIM_ON)) {
            assertFalse("otro modo con \"$pref\"", PadAim.motionOff(universalPad = false, operative = true, pref = pref))
        }
    }

    /**
     * Escribiendo, el móvil se menea en la mano: con el teclado abierto el
     * giro se para aunque esté encendido, y al cerrarlo vuelve. Fuera del
     * mando universal, y sin el modo confirmado, el teclado no toca nada.
     */
    @Test fun elTecladoAbiertoParaElGiroYAlCerrarloVuelve() {
        val on = AppPrefs.PAD_AIM_ON
        assertTrue(PadAim.motionOff(universalPad = true, operative = true, pref = on, keyboardOpen = true))
        assertFalse(PadAim.motionOff(universalPad = true, operative = true, pref = on, keyboardOpen = false))
        assertFalse(PadAim.motionOff(universalPad = false, operative = true, pref = on, keyboardOpen = true))
        assertFalse(PadAim.motionOff(universalPad = true, operative = false, pref = on, keyboardOpen = true))
    }

    private fun ask(
        pref: String = AppPrefs.PAD_AIM_ASK,
        universalPad: Boolean = true,
        operative: Boolean = true,
        sideChosen: Boolean = true,
        headerExpanded: Boolean = false,
        autoCollapse: Boolean = true
    ) = PadAim.shouldAsk(universalPad, operative, pref, sideChosen, headerExpanded, autoCollapse)

    @Test fun laPreguntaSaleUnaVezYDetrasDeLaDelLado() {
        assertTrue(ask())
        // Contestada (de las dos maneras), no vuelve a salir
        assertFalse(ask(pref = AppPrefs.PAD_AIM_ON))
        assertFalse(ask(pref = AppPrefs.PAD_AIM_OFF))
        // Una pregunta cada vez: el lado va primero
        assertFalse(ask(sideChosen = false))
        // Sin el eco del modo, y en cualquier otro modo, no se pregunta
        assertFalse(ask(operative = false))
        assertFalse(ask(universalPad = false))
        // La cabecera desplegada tapa el sitio: se espera a que se pliegue,
        // salvo que no vaya a plegarse sola (lector de pantalla)
        assertFalse(ask(headerExpanded = true))
        assertTrue(ask(headerExpanded = true, autoCollapse = false))
    }
}
