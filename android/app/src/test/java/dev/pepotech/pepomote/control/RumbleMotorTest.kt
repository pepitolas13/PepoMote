package dev.pepotech.pepomote.control

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * La política del motor de la vibración: cada orden al `Vibrator` es finita
 * (caduca sola) y se renueva mientras el receptor siga confirmando; un Stop
 * solo cancela tras un hueco de silencio, para fundir los pulsos de Wii en
 * vez de bombardear al sistema con `vibrate()`/`cancel()`. Reloj de mentira.
 */
class RumbleMotorTest {
    private fun vibrate(level: Float) = MotorOrder.Vibrate(level)

    @Test
    fun startVibraAlInstanteYStopEsperaElHueco() {
        val m = RumbleMotor()
        assertEquals(vibrate(1f), m.set(1f, 0))
        assertTrue(m.isOn)
        assertNull("parar no cancela en el acto", m.set(0f, 10))
        assertTrue("sigue encendido hasta el hueco", m.isOn)
        assertNull("hueco de 40 ms: aún no", m.tick(50))
        assertEquals("hueco de 90 ms: cancela", MotorOrder.Cancel, m.tick(100))
        assertFalse(m.isOn)
        assertEquals("reintento 1", MotorOrder.Cancel, m.tick(200))
        assertEquals("reintento 2", MotorOrder.Cancel, m.tick(300))
        assertNull(m.tick(400))
        assertFalse("sin nada pendiente", m.busy)
    }

    @Test
    fun laComprobacionDelHuecoTambienValeEntreTics() {
        val m = RumbleMotor()
        m.set(1f, 0)
        m.set(0f, 500)
        // La comprobación programada a +80 ms del Stop
        assertEquals(MotorOrder.Cancel, m.tick(580))
    }

    @Test
    fun unTrenDePulsosNoBombardeaAlMotor() {
        val m = RumbleMotor()
        val orders = mutableListOf<Pair<Long, MotorOrder>>()
        fun note(t: Long, o: MotorOrder?) { if (o != null) orders.add(t to o) }
        var nextTick = 100L
        // 30 Hz: 16 ms encendido, 17 apagado, durante un segundo
        for (i in 0 until 30) {
            val on = i * 33L
            val off = on + 16
            while (nextTick <= on) { note(nextTick, m.tick(nextTick)); nextTick += 100 }
            note(on, m.set(1f, on))
            while (nextTick <= off) { note(nextTick, m.tick(nextTick)); nextTick += 100 }
            note(off, m.set(0f, off))
        }
        assertEquals("una sola orden durante el tren: $orders", listOf(0L to vibrate(1f)), orders)
        assertTrue(m.isOn)
        // Acabado el tren (último Stop en 973 ms): el hueco se cumple y cancela
        assertEquals(MotorOrder.Cancel, m.tick(1100))
        assertFalse(m.isOn)
    }

    @Test
    fun unRumbleLargoSeRenuevaCadaDosSegundosSinCancelar() {
        val m = RumbleMotor()
        assertEquals(vibrate(0.65f), m.set(0.65f, 0))
        val renewals = mutableListOf<Long>()
        var t = 100L
        while (t <= 7000) {
            when (val o = m.tick(t)) {
                is MotorOrder.Vibrate -> { assertEquals(0.65f, o.level, 1e-6f); renewals.add(t) }
                MotorOrder.Cancel -> throw AssertionError("cancelado en $t")
                null -> Unit
            }
            t += 100
        }
        assertEquals(listOf(2000L, 4000L, 6000L), renewals)
        assertTrue(m.busy)
        assertTrue(m.isOn)
    }

    @Test
    fun unStartConElMotorEncendidoNoReemite() {
        val m = RumbleMotor()
        assertEquals(vibrate(1f), m.set(1f, 0))
        m.set(0f, 500)
        assertNull("el pulso siguiente entra dentro de la orden viva", m.set(1f, 520))
        assertNull(m.tick(600))
        assertTrue(m.isOn)
        assertTrue(m.busy)
    }

    @Test
    fun losCambiosSeAgrupanEnElTicConHisteresis() {
        val m = RumbleMotor()
        assertEquals(vibrate(1f), m.set(1f, 0))
        assertNull(m.set(0.95f, 10))
        assertNull(m.set(0.5f, 20))
        assertEquals("el último nivel, en el tic", vibrate(0.5f), m.tick(100))
        assertNull(m.set(0.55f, 150))
        assertNull("un cambio menor que la histéresis no reemite", m.tick(200))
    }

    @Test
    fun elChipDeAjustesAplicaAlInstante() {
        val m = RumbleMotor()
        assertEquals(vibrate(1f), m.set(1f, 0))
        assertEquals(vibrate(0.35f), m.set(0.35f, 10, immediate = true))
        assertNull("mismo nivel: nada", m.set(0.35f, 15, immediate = true))
        assertEquals("Apagada cancela ya", MotorOrder.Cancel, m.set(0f, 20, immediate = true))
        assertFalse(m.isOn)
        assertEquals(MotorOrder.Cancel, m.tick(120))
        assertEquals(MotorOrder.Cancel, m.tick(220))
        assertNull(m.tick(320))
        assertFalse(m.busy)
    }

    @Test
    fun resetCancelaYReintenta() {
        val m = RumbleMotor()
        assertEquals("sobre nuevo también cancela: no cuesta nada", MotorOrder.Cancel, m.reset())
        m.tick(100); m.tick(200); m.tick(300)
        assertFalse(m.busy)
        assertEquals(vibrate(1f), m.set(1f, 400))
        assertEquals(MotorOrder.Cancel, m.reset())
        assertFalse(m.isOn)
        assertTrue(m.busy)
        assertEquals(MotorOrder.Cancel, m.tick(510))
        assertEquals(MotorOrder.Cancel, m.tick(610))
        assertNull(m.tick(710))
        assertFalse(m.busy)
    }

    @Test
    fun unStartDuranteLosReintentosVuelveAVibrarYNingunReintentoLoMata() {
        val m = RumbleMotor()
        m.set(1f, 0)
        m.set(0f, 10)
        assertEquals(MotorOrder.Cancel, m.tick(100))
        assertEquals(vibrate(1f), m.set(1f, 150))
        assertNull("sin reintento que lo mate", m.tick(200))
        assertNull(m.tick(300))
        assertTrue(m.isOn)
    }

    @Test
    fun sinNadaQueHacerNoEstaOcupado() {
        val m = RumbleMotor()
        assertFalse(m.busy)
        assertNull(m.set(0f, 0))
        assertFalse("un Stop sin motor no deja nada pendiente", m.busy)
        assertNull(m.tick(100))
    }
}
