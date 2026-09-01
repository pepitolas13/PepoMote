package dev.pepotech.pepomote.control

/**
 * Garantiza que cada pulsación dure al menos [minPressMs] EN EL CABLE.
 *
 * Un toque de botón físico dura 20-40 ms: 5-10 paquetes a 250 Hz. Una ráfaga
 * de pérdida Wi-Fi de ese tamaño (habitual) se lo traga entero, y el juego
 * (60 Hz, a veces con antirrebote de 2-3 frames) también puede no verlo. Con
 * 70 ms van ~18 paquetes y el toque llega siempre. El flanco de BAJADA (el que
 * marca la latencia) sale al instante; solo se retrasa el de subida, y solo en
 * toques más cortos que el mínimo. Mantener pulsado no cambia nada.
 *
 * Dos toques muy seguidos no se funden: si el segundo llega con el primero aún
 * retenido, se suelta al momento y se re-pulsa tras [minGapMs] (dos paquetes),
 * para que el receptor vea los dos flancos.
 *
 * Sin dependencias de Android: el reloj y el temporizador se inyectan.
 */
class PressLatch(
    private val scheduler: Scheduler,
    private val minPressMs: Long = 70,
    private val minGapMs: Long = 10,
    private val onWire: (bit: Int, down: Boolean) -> Unit
) {
    /** Reloj + temporizador (Handler del hilo principal en la app; virtual en tests). */
    interface Scheduler {
        fun now(): Long
        fun postDelayed(delayMs: Long, task: Runnable)
        fun cancel(task: Runnable)
    }

    private class Bit {
        var downAt = 0L
        var release: Runnable? = null
        var press: Runnable? = null
        var releaseWhenPressed = false
    }

    private val bits = HashMap<Int, Bit>()

    @Synchronized
    fun set(bit: Int, down: Boolean) {
        val b = bits.getOrPut(bit) { Bit() }
        if (down) physicalDown(bit, b) else physicalUp(bit, b)
    }

    private fun physicalDown(bit: Int, b: Bit) {
        b.releaseWhenPressed = false
        if (b.press != null) return // re-pulsación ya en cola: se mantendrá
        val held = b.release
        if (held == null) {
            press(bit, b)
            return
        }
        // El toque anterior sigue retenido: soltar YA y re-pulsar tras un hueco
        scheduler.cancel(held)
        b.release = null
        onWire(bit, false)
        lateinit var task: Runnable
        task = Runnable {
            synchronized(this) {
                if (b.press === task) {
                    b.press = null
                    press(bit, b)
                }
            }
        }
        b.press = task
        scheduler.postDelayed(minGapMs, task)
    }

    private fun physicalUp(bit: Int, b: Bit) {
        if (b.press != null) {
            // El down aún está en cola: se soltará al ejecutarse
            b.releaseWhenPressed = true
            return
        }
        val remaining = b.downAt + minPressMs - scheduler.now()
        if (remaining <= 0) {
            b.release?.let(scheduler::cancel)
            b.release = null
            onWire(bit, false)
        } else {
            scheduleRelease(bit, b, remaining)
        }
    }

    private fun press(bit: Int, b: Bit) {
        b.downAt = scheduler.now()
        onWire(bit, true)
        if (b.releaseWhenPressed) {
            b.releaseWhenPressed = false
            scheduleRelease(bit, b, minPressMs)
        }
    }

    private fun scheduleRelease(bit: Int, b: Bit, delayMs: Long) {
        b.release?.let(scheduler::cancel)
        lateinit var task: Runnable
        task = Runnable {
            synchronized(this) {
                if (b.release === task) {
                    b.release = null
                    onWire(bit, false)
                }
            }
        }
        b.release = task
        scheduler.postDelayed(delayMs, task)
    }

    /** Cancela todo lo pendiente (nueva conexión): nada queda retenido. */
    @Synchronized
    fun reset() {
        for (b in bits.values) {
            b.release?.let(scheduler::cancel)
            b.press?.let(scheduler::cancel)
        }
        bits.clear()
    }
}
