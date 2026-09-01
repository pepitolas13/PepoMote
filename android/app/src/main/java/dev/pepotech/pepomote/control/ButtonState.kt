package dev.pepotech.pepomote.control

import java.util.concurrent.atomic.AtomicInteger

/**
 * Estado de botones compartido entre la UI (escribe) y el hilo de sensores (lee).
 * Bits según PROTOCOL.md §4.2.
 */
object ButtonState {
    const val A = 1 shl 0
    const val B = 1 shl 1
    const val DPAD_UP = 1 shl 2
    const val DPAD_DOWN = 1 shl 3
    const val DPAD_LEFT = 1 shl 4
    const val DPAD_RIGHT = 1 shl 5
    const val PLUS = 1 shl 6
    const val MINUS = 1 shl 7
    const val HOME = 1 shl 8
    const val ONE = 1 shl 9
    const val TWO = 1 shl 10
    const val MEDIA_VOL_UP = 1 shl 11
    const val MEDIA_VOL_DOWN = 1 shl 12
    const val MEDIA_MUTE = 1 shl 13
    const val MEDIA_PLAY_PAUSE = 1 shl 14
    const val MEDIA_NEXT = 1 shl 15
    const val MEDIA_PREV = 1 shl 16

    private val mask = AtomicInteger(0)
    private val recenter = AtomicInteger(0)
    private val scrollAcc = AtomicInteger(0)

    fun set(bit: Int, down: Boolean) {
        mask.updateAndGet { if (down) it or bit else it and bit.inv() }
    }

    fun current(): Int = mask.get()

    fun bumpRecenter() {
        recenter.incrementAndGet()
    }

    fun recenterCount(): Int = recenter.get() and 0xFF

    fun addScroll(dyPx: Int) {
        scrollAcc.addAndGet(dyPx)
    }

    /** Vacía el acumulador de scroll (lo llama el hilo de sensores por paquete). */
    fun drainScroll(): Int = scrollAcc.getAndSet(0).coerceIn(-32768, 32767)

    fun reset() {
        mask.set(0)
        scrollAcc.set(0)
    }
}
