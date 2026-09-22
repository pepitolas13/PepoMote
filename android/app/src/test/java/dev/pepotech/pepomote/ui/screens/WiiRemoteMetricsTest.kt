package dev.pepotech.pepomote.ui.screens

import dev.pepotech.pepomote.ui.components.UiScale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Medidas del mando vertical: en un móvil normal y en tablet nada cambia; en
 * un móvil bajo todo encoge a la vez hasta que la B cabe.
 */
class WiiRemoteMetricsTest {
    private val eps = 1e-3f

    private fun fits(m: WiiRemoteMetrics) {
        assertTrue("cuerpo ${m.bodyMin} en ${m.bodyH}", m.bodyMin <= m.bodyH + 0.01f)
        assertEquals(0f, m.overflow, eps)
    }

    @Test
    fun movilNormalNoCambia() {
        // Pixel: cabecera sola (44), con chips (86) y con multimedia abierta: escala 1 exacta
        val m = WiiRemoteMetrics.forScreen(393f, 852f, 44f)
        assertEquals(1f, m.s, 0f)
        assertEquals(1f, m.grow, 0f)
        assertFalse(m.flexible)
        assertEquals(168f, m.cross, 0f)
        assertEquals(148f, m.big, 0f)
        assertEquals(52f, m.one, 0f)
        assertEquals(88f, m.trigger, 0f)
        assertEquals(18f, m.gap(18f), 0f)
        assertEquals(20, m.text(20))
        assertEquals(44, m.text(44))
        fits(m)
        assertEquals(1f, WiiRemoteMetrics.forScreen(393f, 852f, 86f).s, 0f)
        assertEquals(1f, WiiRemoteMetrics.forScreen(393f, 852f, 86f, mediaOpen = true).s, 0f)
        // El móvil más grande, igual
        assertEquals(1f, WiiRemoteMetrics.forScreen(440f, 956f, 130f, mediaOpen = true).s, 0f)
    }

    /**
     * Fuera del modo puntero no hay fila multimedia (ni su botón ni su hueco):
     * en un móvil bajo el cuerpo encoge menos, y abierta o cerrada da igual.
     */
    @Test
    fun sinFilaMultimediaElCuerpoEncogeMenos() {
        val con = WiiRemoteMetrics.forScreen(360f, 640f, 44f)
        val sin = WiiRemoteMetrics.forScreen(360f, 640f, 44f, mediaRow = false)
        assertTrue("con fila ${con.s} < 1", con.s < 1f)
        assertTrue("sin fila ${sin.s} > con fila ${con.s}", sin.s > con.s)
        assertEquals(sin.s, WiiRemoteMetrics.forScreen(360f, 640f, 44f, mediaOpen = true, mediaRow = false).s, 0f)
        assertFalse(WiiRemoteMetrics.forScreen(360f, 640f, 44f, mediaOpen = true, mediaRow = false).mediaOpen)
        fits(sin)
        // Sin fila, lo que sobra son justo su botón y su hueco
        val conS1 = WiiRemoteMetrics(1f, 900f)
        val sinS1 = WiiRemoteMetrics(1f, 900f, mediaRow = false)
        assertEquals(WiiRemoteMetrics.MEDIA_BUTTON + WiiRemoteMetrics.MEDIA_GAP, conS1.bodyMin - sinS1.bodyMin, eps)
    }

    @Test
    fun movilPequenoEncogeParaQueQuepaLaB() {
        // 360×640 en modo puntero: antes se salían ~60 dp (la B a medias)
        val m = WiiRemoteMetrics.forScreen(360f, 640f, 44f)
        assertEquals(0.899f, m.s, eps)
        assertEquals(79.1f, m.trigger, 0.1f)
        assertEquals(46.7f, m.one, 0.1f)
        assertEquals("el texto no se encoge", 20, m.text(20))
        fits(m)
        // Con la fila multimedia abierta encoge un poco más en vez de echar la B fuera
        val open = WiiRemoteMetrics.forScreen(360f, 640f, 44f, mediaOpen = true)
        assertEquals(0.834f, open.s, eps)
        fits(open)
        // Con los chips de modo (cabecera 86)
        val chips = WiiRemoteMetrics.forScreen(360f, 640f, 86f)
        assertEquals(0.828f, chips.s, eps)
        fits(chips)
        // Wii U como Mando de Wii: chips + selector «En Cemu soy» con su ayuda a tres líneas (~220)
        val selector = WiiRemoteMetrics.forScreen(360f, 640f, 220f)
        assertEquals(0.603f, selector.s, eps)
        assertEquals(31.3f, selector.one, 0.1f)
        fits(selector)
    }

    @Test
    fun porDebajoDelSueloSeRecortaElCuerpoNoLaB() {
        // Pantalla dividida: ni a 0,6 cabe; la pantalla recorta el cuerpo por abajo y la B sigue entera
        val m = WiiRemoteMetrics.forScreen(360f, 440f, 44f)
        assertEquals(WiiRemoteMetrics.FLOOR, m.s, 0f)
        assertEquals(22.4f, m.overflow, 0.1f)
        assertEquals(88f * WiiRemoteMetrics.FLOOR, m.trigger, eps)
    }

    @Test
    fun tabletCreceYNuncaEncoge() {
        // iPad 11": misma escala que UiScale, huecos flexibles, texto grande
        val grow = UiScale.remote(820f, 1180f)
        val m = WiiRemoteMetrics.forScreen(820f, 1180f, 86f)
        assertEquals(1.589f, grow, eps)
        assertEquals(grow, m.s, 0f)
        assertTrue(m.flexible)
        assertEquals(168f * grow, m.cross, eps)
        assertEquals(32, m.text(20))
        assertEquals(grow, WiiRemoteMetrics.forScreen(820f, 1180f, 86f, mediaOpen = true).s, 0f)
        // Una tablet pequeña con cabecera alta y multimedia abierta: crece igual, no encoge
        val small = WiiRemoteMetrics.forScreen(600f, 830f, 160f, mediaOpen = true)
        assertTrue(small.grow > 1f)
        assertEquals(UiScale.remote(600f, 830f), small.s, 0f)
    }
}
