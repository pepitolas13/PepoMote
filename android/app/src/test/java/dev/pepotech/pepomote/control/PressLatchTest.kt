package dev.pepotech.pepomote.control

import org.junit.Assert.assertEquals
import org.junit.Test

/** Reloj virtual: los toques se simulan a tiempos exactos. */
class PressLatchTest {

    private class FakeScheduler : PressLatch.Scheduler {
        var t = 0L
        private val tasks = mutableListOf<Pair<Long, Runnable>>()
        override fun now() = t
        override fun postDelayed(delayMs: Long, task: Runnable) {
            tasks += (t + delayMs) to task
        }

        override fun cancel(task: Runnable) {
            tasks.removeAll { it.second === task }
        }

        fun advanceTo(end: Long) {
            while (true) {
                val next = tasks.filter { it.first <= end }.minByOrNull { it.first } ?: break
                tasks.remove(next)
                t = next.first
                next.second.run()
            }
            t = end
        }
    }

    private val clock = FakeScheduler()
    private val wire = mutableListOf<Pair<Long, Boolean>>()
    private val latch = PressLatch(clock) { _, down -> wire += clock.t to down }

    private fun down(at: Long) {
        clock.advanceTo(at); latch.set(1, true)
    }

    private fun up(at: Long) {
        clock.advanceTo(at); latch.set(1, false)
    }

    @Test
    fun elFlancoDeBajadaSaleAlInstante() {
        down(0)
        assertEquals(listOf(0L to true), wire)
    }

    @Test
    fun unToqueCortoDuraElMinimoEnElCable() {
        down(0)
        up(20)
        clock.advanceTo(500)
        assertEquals(listOf(0L to true, 70L to false), wire)
    }

    @Test
    fun mantenerPulsadoSueltaCuandoSeSuelta() {
        down(0)
        up(300)
        clock.advanceTo(500)
        assertEquals(listOf(0L to true, 300L to false), wire)
    }

    @Test
    fun dosToquesRapidosNoSeFunden() {
        down(0)
        up(20)
        down(40) // el primero aún retenido: se suelta ya y se re-pulsa a los 10 ms
        up(60)
        clock.advanceTo(500)
        assertEquals(listOf(0L to true, 40L to false, 50L to true, 120L to false), wire)
    }

    @Test
    fun soltarAntesDeQueSalgaLaRepulsacionTambienDuraElMinimo() {
        down(0)
        up(20)
        down(40)
        up(45) // el segundo down sigue en cola (sale a los 50)
        clock.advanceTo(500)
        assertEquals(listOf(0L to true, 40L to false, 50L to true, 120L to false), wire)
    }

    @Test
    fun resetCancelaLoRetenido() {
        down(0)
        up(20)
        latch.reset()
        clock.advanceTo(500)
        assertEquals(listOf(0L to true), wire) // nadie suelta: lo hace ButtonState.reset()
    }
}
