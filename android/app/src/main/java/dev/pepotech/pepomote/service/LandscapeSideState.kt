package dev.pepotech.pepomote.service

import android.content.Context
import dev.pepotech.pepomote.control.AppPrefs
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Lado elegido para un mando apaisado fijo ([LandscapeSide]), observable
 * desde la actividad (orientación), Ajustes y el propio mando: lo guardado
 * y, mientras se pregunta la primera vez, el lado provisional («Darle la
 * vuelta») que aún no se ha confirmado. Uno por mando: [NunchukSide] y
 * [GamePadSide], cada uno con su preferencia.
 */
open class LandscapeSideState(private val prefKey: String) {
    private val savedFlow = MutableStateFlow(LandscapeSide.Unset)
    private val provisionalFlow = MutableStateFlow<LandscapeSide?>(null)

    val saved: StateFlow<LandscapeSide> = savedFlow
    val provisional: StateFlow<LandscapeSide?> = provisionalFlow

    /** Al arrancar: lo guardado en Ajustes. */
    fun load(context: Context) {
        savedFlow.value = LandscapeSide.fromPref(AppPrefs.landscapeSide(context, prefKey))
    }

    /** «Así lo quiero» o Ajustes: se guarda para siempre y se acaba la prueba. */
    fun save(context: Context, side: LandscapeSide) {
        AppPrefs.setLandscapeSide(context, prefKey, side.pref)
        savedFlow.value = side
        provisionalFlow.value = null
    }

    /** «Darle la vuelta» (sin confirmar aún); null al salir del mando sin confirmar. */
    fun setProvisional(side: LandscapeSide?) {
        provisionalFlow.value = side
    }
}

/** Lado del mando + Nunchuk de Dolphin. */
object NunchukSide : LandscapeSideState("nunchukSide")

/** Lado del GamePad de Wii U (también el Pro Controller: es la misma pantalla). */
object GamePadSide : LandscapeSideState("gamePadSide")
