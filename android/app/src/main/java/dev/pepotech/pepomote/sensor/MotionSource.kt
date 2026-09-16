package dev.pepotech.pepomote.sensor

import android.content.Context
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.service.LinkState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Sensor con el que el puntero mueve el cursor: giroscopio (lo de siempre) o
 * acelerómetro (apuntado por inclinación, INPUT flags bit4). Lo decide el
 * ajuste del usuario y, si no ha elegido, la clase del giroscopio
 * ([GyroDetect]): real → giroscopio; virtual o ninguno → acelerómetro. Sin
 * giroscopio ni rotation vector solo cabe el acelerómetro, diga lo que diga
 * el ajuste. Que el bit llegue al receptor depende además de que este lo
 * anuncie en su `ok` (lo cruza el servicio).
 */
object MotionSource {
    private val _tilt = MutableStateFlow(false)

    /** true = acelerómetro (inclinación). Observable por Ajustes y el mando. */
    val tilt: StateFlow<Boolean> = _tilt

    /** Lo que toca ahora según ajuste y hardware, y lo publica. */
    fun refresh(context: Context): Boolean {
        val gyroClass = GyroDetect.classify(context)
        val on = when {
            gyroClass == GyroClass.NONE -> true
            else -> when (AppPrefs.motionSource(context)) {
                AppPrefs.MOTION_ACCEL -> true
                AppPrefs.MOTION_GYRO -> false
                else -> gyroClass != GyroClass.REAL
            }
        }
        _tilt.value = on
        return on
    }

    /** Elegido a mano (Ajustes): se guarda, se publica y llega al enlace vivo. */
    fun choose(context: Context, accel: Boolean) {
        AppPrefs.setMotionSource(context, if (accel) AppPrefs.MOTION_ACCEL else AppPrefs.MOTION_GYRO)
        LinkState.setTilt?.invoke(refresh(context))
    }
}
