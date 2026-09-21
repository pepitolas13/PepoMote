package dev.pepotech.pepomote.service

import android.app.Activity
import android.content.res.Configuration
import android.hardware.SensorManager
import android.os.Build
import android.os.SystemClock
import android.provider.Settings
import android.view.OrientationEventListener
import android.view.Surface
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Propuesta de girar la pantalla (ver [RotationProposal]): con el giro
 * automático del sistema BLOQUEADO, escucha el giro físico del móvil
 * (`OrientationEventListener`, que funciona aunque el sistema no gire) y,
 * cuando lleva un momento mirando hacia otro lado que la pantalla, publica
 * [proposal] para que el mando enseñe el botón «Girar la pantalla». Aceptarla
 * ([accept]) pide esa orientación a la actividad ([accepted]) hasta la
 * siguiente propuesta, hasta salir del mando o hasta que se active el giro
 * automático del sistema (entonces vuelve a mandar él: si no, el mando se
 * quedaría clavado en la orientación aceptada aunque el móvil ya girase
 * solo); nunca gira sola. Con el giro automático activo no propone nada: ya
 * gira el sistema. Solo se escucha con [active] (el mando en pantalla y
 * orientación libre: el GamePad y el mando + Nunchuk van fijos en apaisado).
 */
object RotationSuggester {
    private val machine = RotationProposalMachine()

    private val _proposal = MutableStateFlow<Int?>(null)

    /** Propuesta en pantalla (`ActivityInfo.SCREEN_ORIENTATION_*`), o null. */
    val proposal: StateFlow<Int?> = _proposal

    private val _accepted = MutableStateFlow<Int?>(null)

    /** Orientación aceptada, o null = la que diga el sistema. */
    val accepted: StateFlow<Int?> = _accepted

    private var activity: Activity? = null
    private var listener: OrientationEventListener? = null

    /**
     * Con el mando en pantalla y orientación libre. Al apagarse, la
     * orientación vuelve a ser la del sistema («al salir del mando, como
     * estaba») y el sensor deja de escucharse.
     */
    @Volatile
    var active: Boolean = false
        set(value) {
            field = value
            if (!value) {
                machine.clear()
                _proposal.value = null
                _accepted.value = null
            }
            syncListener()
        }

    /** La actividad está en pantalla (onResume). */
    fun start(activity: Activity) {
        this.activity = activity
        syncListener()
    }

    /** La actividad deja de estar en pantalla (onPause): sensor fuera, propuesta fuera. */
    fun stop() {
        listener?.disable()
        listener = null
        activity = null
        machine.clear()
        _proposal.value = null
    }

    /** El usuario toca la propuesta. */
    fun accept() {
        val p = machine.accept() ?: return
        _proposal.value = null
        _accepted.value = p
    }

    private fun syncListener() {
        val a = activity
        if (a == null || !active) {
            listener?.disable()
            listener = null
            return
        }
        if (listener != null) return
        val l = object : OrientationEventListener(a, SensorManager.SENSOR_DELAY_NORMAL) {
            override fun onOrientationChanged(orientation: Int) = onDegrees(orientation)
        }
        if (l.canDetectOrientation()) {
            l.enable()
            listener = l
        }
    }

    private fun onDegrees(degrees: Int) {
        val a = activity ?: return
        val autoRotate = systemAutoRotate(a)
        // Giro automático activado después de aceptar una propuesta: la
        // orientación aceptada deja de mandar y decide el sistema
        if (autoRotate && _accepted.value != null) _accepted.value = null
        val enabled = active && !autoRotate
        val rotation = displayRotation(a)
        val p = machine.onDegrees(degrees, rotation, naturalPortrait(a, rotation), SystemClock.elapsedRealtime(), enabled)
        if (_proposal.value != p) _proposal.value = p
    }

    /** El giro automático del sistema está activo (entonces ya gira él solo). */
    private fun systemAutoRotate(a: Activity): Boolean = try {
        Settings.System.getInt(a.contentResolver, Settings.System.ACCELEROMETER_ROTATION, 0) == 1
    } catch (_: RuntimeException) {
        false
    }

    private fun displayRotation(a: Activity): Int = try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            a.display?.rotation ?: Surface.ROTATION_0
        } else {
            @Suppress("DEPRECATION")
            a.windowManager.defaultDisplay.rotation
        }
    } catch (_: RuntimeException) {
        Surface.ROTATION_0
    }

    /** Vertical de fábrica (un móvil): derecho y en vertical, o girado y en apaisado. */
    private fun naturalPortrait(a: Activity, rotation: Int): Boolean {
        val landscapeNow = a.resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
        val upright = rotation == Surface.ROTATION_0 || rotation == Surface.ROTATION_180
        return upright != landscapeNow
    }
}
