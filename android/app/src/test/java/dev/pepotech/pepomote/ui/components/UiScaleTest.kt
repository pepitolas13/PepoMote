package dev.pepotech.pepomote.ui.components

import org.junit.Assert.assertEquals
import org.junit.Test

/** Escala de la interfaz en tablets: mismos vectores que iOS (UiScaleTests). */
class UiScaleTest {
    private val eps = 1e-3f

    @Test
    fun mandoVerticalNoEncogeYCreceConLaPantalla() {
        assertEquals(1f, UiScale.remote(375f, 667f), eps)
        assertEquals(1f, UiScale.remote(393f, 852f), eps)
        assertEquals("el móvil más grande sigue en 1", 1f, UiScale.remote(440f, 956f), eps)
        assertEquals(1.514f, UiScale.remote(744f, 1133f), eps)
        assertEquals(1.589f, UiScale.remote(820f, 1180f), eps)
        assertEquals(1.899f, UiScale.remote(1032f, 1376f), eps)
        assertEquals(2f, UiScale.remote(2000f, 3000f), eps)
        assertEquals(1f, UiScale.remote(0f, 0f), eps)
    }

    @Test
    fun mandoApaisado() {
        assertEquals(1f, UiScale.landscape(852f, 393f), eps)
        assertEquals(1f, UiScale.landscape(956f, 440f), eps)
        assertEquals(1.619f, UiScale.landscape(1133f, 744f), eps)
        assertEquals(1.686f, UiScale.landscape(1180f, 820f), eps)
        assertEquals(1.966f, UiScale.landscape(1376f, 1032f), eps)
        assertEquals(2f, UiScale.landscape(5000f, 3000f), eps)
    }

    @Test
    fun gamePad() {
        assertEquals(1f, UiScale.gamePad(852f, 393f), eps)
        assertEquals(1.185f, UiScale.gamePad(1133f, 744f), eps)
        assertEquals(1.234f, UiScale.gamePad(1180f, 820f), eps)
        assertEquals(1.439f, UiScale.gamePad(1376f, 1032f), eps)
        assertEquals(2f, UiScale.gamePad(3000f, 1500f), eps)
    }

    @Test
    fun medidasDelMandoVertical() {
        // iPad 13": la A pasa de 148 a 281 dp y la columna (520·grow) casi llena la pantalla
        val grow = UiScale.remote(1032f, 1376f)
        assertEquals(281f, 148f * grow, 1f)
        assertEquals(987f, 520f * grow, 1f)
        // NES apaisado en tablet 13"
        assertEquals(190f * 1.966f, 190f * UiScale.landscape(1376f, 1032f), 0.2f)
    }
}
