package dev.pepotech.pepomote.control

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import java.util.concurrent.atomic.AtomicInteger

/**
 * Estado de botones compartido entre la UI (escribe) y el hilo de sensores (lee).
 * Bits según PROTOCOL.md §4.2. Las pulsaciones pasan por [PressLatch]: un
 * toque, por corto que sea, dura lo suficiente en el cable para llegar.
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

    /** Nunchuk (móvil de la otra mano). */
    const val C = 1 shl 17
    const val Z = 1 shl 18

    /** Wii U GamePad / Pro Controller (modo Cemu). */
    const val X = 1 shl 19
    const val Y = 1 shl 20
    const val L = 1 shl 21
    const val R = 1 shl 22
    const val ZL = 1 shl 23
    const val ZR = 1 shl 24

    /** Click del stick izquierdo / derecho (L3 / R3). */
    const val STICK_L = 1 shl 25
    const val STICK_R = 1 shl 26

    /** Soplar al micrófono del GamePad. */
    const val MIC = 1 shl 27

    /** Cambiar la vista TV ↔ pantalla del GamePad (función de Cemu). */
    const val SCREEN = 1 shl 28

    /**
     * Dedo en la pantalla táctil del GamePad: instantánea inmutable, así el
     * hilo de sensores lee x, y y `down` coherentes entre sí de una sola vez.
     * x, y = fracción de la pantalla en 0..65535, origen arriba-izquierda.
     */
    class Touch(val x: Int, val y: Int, val down: Boolean)

    private val NO_TOUCH = Touch(0, 0, false)

    private val mask = AtomicInteger(0)
    private val recenter = AtomicInteger(0)
    private val scrollAcc = AtomicInteger(0)

    /** Stick del Nunchuk o izquierdo del GamePad, −127..127, +x derecha, +y arriba. */
    private val stickXv = AtomicInteger(0)
    private val stickYv = AtomicInteger(0)

    /** Stick derecho del GamePad, misma convención. */
    private val stick2Xv = AtomicInteger(0)
    private val stick2Yv = AtomicInteger(0)

    @Volatile
    private var touchV: Touch = NO_TOUCH

    private val handler = Handler(Looper.getMainLooper())
    private val latch = PressLatch(object : PressLatch.Scheduler {
        override fun now(): Long = SystemClock.uptimeMillis()
        override fun postDelayed(delayMs: Long, task: Runnable) {
            handler.postDelayed(task, delayMs)
        }

        override fun cancel(task: Runnable) = handler.removeCallbacks(task)
    }) { bit, down ->
        mask.updateAndGet { if (down) it or bit else it and bit.inv() }
    }

    /** Pulsación física/táctil: el flanco de bajada sale al instante. */
    fun set(bit: Int, down: Boolean) = latch.set(bit, down)

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

    /** Posición del stick (el stick va sin latch: es continuo, no un flanco). */
    fun setStick(x: Int, y: Int) {
        stickXv.set(x.coerceIn(-127, 127))
        stickYv.set(y.coerceIn(-127, 127))
    }

    fun stickX(): Int = stickXv.get()
    fun stickY(): Int = stickYv.get()

    /** Stick derecho del GamePad (solo lo escribe la pantalla GamePad). */
    fun setStick2(x: Int, y: Int) {
        stick2Xv.set(x.coerceIn(-127, 127))
        stick2Yv.set(y.coerceIn(-127, 127))
    }

    fun stickRX(): Int = stick2Xv.get()
    fun stickRY(): Int = stick2Yv.get()

    /**
     * Pantalla táctil del GamePad: `x`, `y` en 0..65535 (se recortan) y
     * `down` = hay dedo. Al soltar se conservan las últimas coordenadas
     * (al receptor le da igual: sin FLAG_TOUCH no las mira).
     */
    fun setTouch(x: Int, y: Int, down: Boolean) {
        touchV = Touch(x.coerceIn(0, 65535), y.coerceIn(0, 65535), down)
    }

    /** Instantánea atómica del táctil. */
    fun touch(): Touch = touchV

    /** Todo suelto: botones, scroll, ambos sticks y táctil. */
    fun reset() {
        latch.reset()
        mask.set(0)
        scrollAcc.set(0)
        stickXv.set(0)
        stickYv.set(0)
        stick2Xv.set(0)
        stick2Yv.set(0)
        touchV = NO_TOUCH
    }
}
