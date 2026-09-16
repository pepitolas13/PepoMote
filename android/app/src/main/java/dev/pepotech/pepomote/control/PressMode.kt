package dev.pepotech.pepomote.control

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/**
 * Los dos ajustes de pulsación, vivos para la interfaz (como
 * [UiSounds.enabled] con los sonidos): los lee cada botón y la capa de
 * pulsación sin volver a tocar las preferencias en cada dedo. Los setters de
 * [AppPrefs] los mantienen al día; [init] los carga al arrancar.
 */
object PressMode {
    /** «Pulsar deslizando» (de serie, no). */
    var slide by mutableStateOf(false)

    /** «Mantener al salir del botón» (de serie, sí). Manda [slide] si está encendido. */
    var sticky by mutableStateOf(true)

    fun init(context: Context) {
        slide = AppPrefs.slidePress(context)
        sticky = AppPrefs.stickyPress(context)
    }
}
