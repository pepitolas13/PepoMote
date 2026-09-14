package dev.pepotech.pepomote.service

import android.content.Context
import dev.pepotech.pepomote.control.AppPrefs
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Lado elegido para el mando + Nunchuk ([LandscapeSide]), observable desde
 * la actividad (orientación), Ajustes y el propio mando: lo guardado y,
 * mientras se pregunta la primera vez, el lado provisional («Darle la
 * vuelta») que aún no se ha confirmado.
 */
object NunchukSide {
    private val savedFlow = MutableStateFlow(LandscapeSide.Unset)
    private val provisionalFlow = MutableStateFlow<LandscapeSide?>(null)

    val saved: StateFlow<LandscapeSide> = savedFlow
    val provisional: StateFlow<LandscapeSide?> = provisionalFlow

    /** Al arrancar: lo guardado en Ajustes. */
    fun load(context: Context) {
        savedFlow.value = LandscapeSide.fromPref(AppPrefs.nunchukSide(context))
    }

    /** «Así lo quiero» o Ajustes: se guarda para siempre y se acaba la prueba. */
    fun save(context: Context, side: LandscapeSide) {
        AppPrefs.setNunchukSide(context, side.pref)
        savedFlow.value = side
        provisionalFlow.value = null
    }

    /** «Darle la vuelta» (sin confirmar aún); null al salir del mando sin confirmar. */
    fun setProvisional(side: LandscapeSide?) {
        provisionalFlow.value = side
    }
}
