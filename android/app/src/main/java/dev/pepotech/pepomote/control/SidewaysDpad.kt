package dev.pepotech.pepomote.control

/**
 * Cruceta con el mando de lado (NES). Un juego «de lado» espera un Mando de
 * Wii girado con el extremo IR a la izquierda y aplica él mismo el giro, así
 * que lo que en pantalla apunta arriba es el RIGHT del mando, abajo el LEFT,
 * izquierda el UP y derecha el DOWN. Puro: testeable.
 */
object SidewaysDpad {
    fun up(sideways: Boolean): Int = if (sideways) ButtonState.DPAD_RIGHT else ButtonState.DPAD_UP
    fun down(sideways: Boolean): Int = if (sideways) ButtonState.DPAD_LEFT else ButtonState.DPAD_DOWN
    fun left(sideways: Boolean): Int = if (sideways) ButtonState.DPAD_UP else ButtonState.DPAD_LEFT
    fun right(sideways: Boolean): Int = if (sideways) ButtonState.DPAD_DOWN else ButtonState.DPAD_RIGHT
}
