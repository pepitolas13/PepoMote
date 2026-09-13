package dev.pepotech.pepomote.ui.components

import org.junit.Assert.assertEquals
import org.junit.Test

/** Escala de la interfaz en tablets: mismos vectores que iOS (UiScaleTests). */
class UiScaleTest {
    private val eps = 1e-3f

    @Test
    fun verticalNoEncogeYCreceEnTablet() {
        val bw = UiScale.PHONE_PORTRAIT_W
        val bh = UiScale.PHONE_PORTRAIT_H
        assertEquals(1f, UiScale.factor(375f, 667f, bw, bh, 1.5f), eps)
        assertEquals(1f, UiScale.factor(393f, 852f, bw, bh, 1.5f), eps)
        assertEquals(1f, UiScale.factor(440f, 956f, bw, bh, 1.5f), eps)
        assertEquals(1.185f, UiScale.factor(744f, 1133f, bw, bh, 1.5f), eps)
        assertEquals(1.234f, UiScale.factor(820f, 1180f, bw, bh, 1.5f), eps)
        assertEquals(1.439f, UiScale.factor(1032f, 1376f, bw, bh, 1.5f), eps)
        assertEquals(1.5f, UiScale.factor(2000f, 3000f, bw, bh, 1.5f), eps)
        assertEquals(1f, UiScale.factor(0f, 0f, bw, bh, 1.5f), eps)
    }

    @Test
    fun apaisado() {
        val bw = UiScale.PHONE_LANDSCAPE_W
        val bh = UiScale.PHONE_LANDSCAPE_H
        assertEquals(1f, UiScale.factor(852f, 393f, bw, bh, 1.6f), eps)
        assertEquals(1.234f, UiScale.factor(1180f, 820f, bw, bh, 1.6f), eps)
        assertEquals(1.439f, UiScale.factor(1376f, 1032f, bw, bh, 1.6f), eps)
        assertEquals(1.6f, UiScale.factor(5000f, 3000f, bw, bh, 1.6f), eps)
    }

    @Test
    fun soloAncho() {
        assertEquals(1f, UiScale.factorWidth(844f, 956f, 1.6f), eps)
        assertEquals(1f, UiScale.factorWidth(956f, 956f, 1.6f), eps)
        assertEquals(1.234f, UiScale.factorWidth(1180f, 956f, 1.6f), eps)
        assertEquals(1.439f, UiScale.factorWidth(1376f, 956f, 1.6f), eps)
        assertEquals(1.6f, UiScale.factorWidth(3000f, 956f, 1.6f), eps)
    }
}
