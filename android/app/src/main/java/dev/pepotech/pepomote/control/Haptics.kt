package dev.pepotech.pepomote.control

import android.view.View

/**
 * Háptico de un botón, la cruceta o el stick (`performHapticFeedback`).
 * Mientras vibra el juego no se dispara: el sistema ya lo ignoraba con la
 * onda infinita de antes, y con la onda finita de [GameRumble] un toque
 * propio la cortaría hasta la renovación. El motor ya está sonando, así que
 * el dedo no se pierde nada.
 */
object Haptics {
    fun tap(view: View?, constant: Int) {
        if (GameRumble.isOn) return
        view?.performHapticFeedback(constant)
    }
}
