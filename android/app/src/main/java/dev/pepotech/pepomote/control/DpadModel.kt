package dev.pepotech.pepomote.control

import kotlin.math.abs

/**
 * Cruceta de una sola pieza (como la del Mando de Wii): el dedo se sigue
 * mientras está apoyado y la dirección sale de dónde está respecto al
 * centro, así se puede deslizar de → a ↓ sin levantarlo y las esquinas
 * entre dos brazos son diagonales. Medidas en fracción de [half] (media
 * anchura de la cruceta): los brazos miden un tercio (media anchura
 * `a = half / 3`, igual que antes) y el centro muerto es un círculo de radio
 * [DEAD]·a, bastante más pequeño que el cuadrado central de antes. Puro:
 * testeable en la JVM.
 */
object DpadModel {
    const val UP = 1
    const val DOWN = 2
    const val LEFT = 4
    const val RIGHT = 8

    /** Radio del centro muerto, en medias anchuras de brazo. */
    const val DEAD = 0.6f

    /**
     * Direcciones en pantalla para un dedo en ([dx], [dy]) desde el centro
     * (+y abajo): dentro del centro muerto, ninguna; en el cuadrado central,
     * la del eje dominante; sobre un brazo, solo la suya (aunque el dedo se
     * salga por la punta); en una esquina entre dos brazos, las dos.
     */
    fun dirs(dx: Float, dy: Float, half: Float): Int {
        val a = half / 3f
        val dead = a * DEAD
        if (dx * dx + dy * dy <= dead * dead) return 0
        val ax = abs(dx)
        val ay = abs(dy)
        val horizontal = if (dx > 0f) RIGHT else LEFT
        val vertical = if (dy > 0f) DOWN else UP
        return when {
            ax <= a && ay <= a -> if (ax > ay) horizontal else vertical
            ay <= a -> horizontal
            ax <= a -> vertical
            else -> horizontal or vertical
        }
    }

    /** Bits del mando ([ButtonState]) de esas direcciones; con [sideways], los del mando girado ([SidewaysDpad]). */
    fun buttons(dirs: Int, sideways: Boolean): Int =
        (if (dirs and UP != 0) SidewaysDpad.up(sideways) else 0) or
            (if (dirs and DOWN != 0) SidewaysDpad.down(sideways) else 0) or
            (if (dirs and LEFT != 0) SidewaysDpad.left(sideways) else 0) or
            (if (dirs and RIGHT != 0) SidewaysDpad.right(sideways) else 0)

    /** Los cuatro bits de cruceta del mando. */
    val BUTTON_BITS = intArrayOf(ButtonState.DPAD_UP, ButtonState.DPAD_DOWN, ButtonState.DPAD_LEFT, ButtonState.DPAD_RIGHT)
}
