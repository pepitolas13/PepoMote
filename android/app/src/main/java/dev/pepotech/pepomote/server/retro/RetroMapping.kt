package dev.pepotech.pepomote.server.retro

import dev.pepotech.pepomote.server.core.ReceiverInput

/**
 * Del paquete INPUT del móvil (PROTOCOL.md §4.2) al RetroPad, según el mando
 * que el móvil ha elegido para RetroArch (`pad`), igual que
 * `desktop/src/retroarch/mapping.rs`:
 *
 * - `retropad`: el mando apaisado de dos sticks con etiquetas de RetroPad
 *   (posiciones de la SNES); ZL/ZR → L2/R2; − / + → Select / Start; Home →
 *   menú de RetroArch; Capturar (bit 28) → avance rápido mientras se mantiene.
 * - `nes`: el Mando Wii de lado. 1 → B y 2 → A, la A y B grandes → X e Y.
 * - `gun`: no existe en el servidor Android (no hay ratón que mover); el
 *   receptor no lo acepta y se queda con el mando anterior.
 */
enum class RetroPadKind(val wire: String) {
    RetroPad("retropad"), Nes("nes"), Gun("gun");

    companion object {
        fun parse(s: String?): RetroPadKind? = entries.firstOrNull { it.wire == s }
    }
}

object RetroMapping {
    // PROTOCOL.md §4.2 (mismos bits que ButtonState / PmpCodec)
    const val BTN_A = 1 shl 0
    const val BTN_B = 1 shl 1
    const val BTN_DPAD_UP = 1 shl 2
    const val BTN_DPAD_DOWN = 1 shl 3
    const val BTN_DPAD_LEFT = 1 shl 4
    const val BTN_DPAD_RIGHT = 1 shl 5
    const val BTN_PLUS = 1 shl 6
    const val BTN_MINUS = 1 shl 7
    const val BTN_HOME = 1 shl 8
    const val BTN_ONE = 1 shl 9
    const val BTN_TWO = 1 shl 10
    const val BTN_X = 1 shl 19
    const val BTN_Y = 1 shl 20
    const val BTN_L = 1 shl 21
    const val BTN_R = 1 shl 22
    const val BTN_ZL = 1 shl 23
    const val BTN_ZR = 1 shl 24
    const val BTN_STICK_L = 1 shl 25
    const val BTN_STICK_R = 1 shl 26
    const val BTN_SCREEN = 1 shl 28
    const val FLAG_STICK_VALID = 2
    const val FLAG_EXT = 4

    /** Bits del INPUT que NO son botones del RetroPad y el receptor trata aparte. */
    const val BIT_MENU = BTN_HOME
    const val BIT_FAST_FORWARD = BTN_SCREEN

    private fun bit(b: Int) = 1 shl b

    /** Bit del INPUT → botón del RetroPad para el mando apaisado. */
    private val RETROPAD: List<Pair<Int, Int>> = listOf(
        BTN_A to RetroPad.A, BTN_B to RetroPad.B, BTN_X to RetroPad.X, BTN_Y to RetroPad.Y,
        BTN_L to RetroPad.L, BTN_R to RetroPad.R, BTN_ZL to RetroPad.L2, BTN_ZR to RetroPad.R2,
        BTN_STICK_L to RetroPad.L3, BTN_STICK_R to RetroPad.R3,
        BTN_PLUS to RetroPad.START, BTN_MINUS to RetroPad.SELECT,
        BTN_DPAD_UP to RetroPad.UP, BTN_DPAD_DOWN to RetroPad.DOWN,
        BTN_DPAD_LEFT to RetroPad.LEFT, BTN_DPAD_RIGHT to RetroPad.RIGHT,
    )

    /** Mando Wii de lado: 1 → B, 2 → A; A y B grandes → X e Y. */
    private val WIIMOTE: List<Pair<Int, Int>> = listOf(
        BTN_ONE to RetroPad.B, BTN_TWO to RetroPad.A, BTN_A to RetroPad.X, BTN_B to RetroPad.Y,
        BTN_PLUS to RetroPad.START, BTN_MINUS to RetroPad.SELECT,
        BTN_DPAD_UP to RetroPad.UP, BTN_DPAD_DOWN to RetroPad.DOWN,
        BTN_DPAD_LEFT to RetroPad.LEFT, BTN_DPAD_RIGHT to RetroPad.RIGHT,
    )

    private fun buttons(table: List<Pair<Int, Int>>, bits: Int): Int =
        table.fold(0) { acc, (b, r) -> if (bits and b != 0) acc or bit(r) else acc }

    /** Botones y sticks de un paquete para el mando `kind`. */
    internal fun padState(kind: RetroPadKind, p: ReceiverInput): PadState = when (kind) {
        RetroPadKind.RetroPad -> {
            val sticks = p.flags and FLAG_STICK_VALID != 0
            val ext = sticks && p.flags and FLAG_EXT != 0
            PadState(
                buttons(RETROPAD, p.buttons),
                listOf(
                    if (sticks) RetroProtocol.axisFromStick(p.stickX, false) else 0,
                    if (sticks) RetroProtocol.axisFromStick(p.stickY, true) else 0,
                    if (ext) RetroProtocol.axisFromStick(p.stickRX, false) else 0,
                    if (ext) RetroProtocol.axisFromStick(p.stickRY, true) else 0,
                ),
            )
        }
        RetroPadKind.Nes, RetroPadKind.Gun -> PadState(buttons(WIIMOTE, p.buttons))
    }
}
