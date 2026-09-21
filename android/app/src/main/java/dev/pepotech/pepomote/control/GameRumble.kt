package dev.pepotech.pepomote.control

import android.content.Context
import android.os.Build
import android.os.SystemClock
import android.os.VibrationAttributes
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.util.Log

/**
 * Motor de la vibración que piden los juegos (RUMBLE del receptor,
 * PROTOCOL.md §4.5), aparte de la háptica de los botones ([Haptics]). Es el
 * gemelo de `GameRumble.swift`. Solo en el hilo principal: el servicio salta
 * a él antes de llamar aquí, y Ajustes ya vive ahí.
 *
 * Qué decide y qué no: cuándo vibrar, renovar o cancelar lo decide
 * [RumbleMotor] (puro y probado); aquí solo se traduce cada orden a una
 * [VibrationEffect] y se manda al [Vibrator]:
 *  - Con control de amplitud, una onda FINITA de [RumbleMotor.LEASE_MS] a la
 *    amplitud que toca, en [SEGMENTS] segmentos (Android 12 toma una finita
 *    de tres o menos por un háptico de toque y la deja en manos del ajuste
 *    «vibración al tocar»).
 *  - Sin control de amplitud, la intensidad se finge con el reparto
 *    encendido/apagado de un ciclo de [CYCLE_MS], también finita.
 * Nunca una onda que se repita sin fin: si el `cancel()` se pierde por el
 * camino (es lo que le pasaba a un móvil que no paraba de vibrar), el motor
 * calla solo al acabar la orden. Mientras el juego siga pidiendo, el reloj
 * del servicio la renueva.
 *
 * En Android 13+ la orden va como `USAGE_PHYSICAL_EMULATION`: los hápticos
 * de los botones (toque) no la cortan, las notificaciones y las alarmas sí
 * (hasta la 1.12 una onda infinita sin uso las tapaba), y su intensidad no
 * depende del ajuste de vibración al tocar.
 */
object GameRumble {
    private const val TAG = "PepoMote"

    /** Ciclo del PWM sin control de amplitud: corto, para que el motor no llegue a pararse del todo. */
    private const val CYCLE_MS = 60L

    /** Segmentos de la onda con amplitud (≥ 4: ver la cabecera). */
    private const val SEGMENTS = 4

    private var vibrator: Vibrator? = null
    private var amplitudes = false
    private var attached = false
    private val motor = RumbleMotor()

    /** Último nivel crudo del receptor (0 = parado): con él se reaplica al cambiar el ajuste. */
    private var raw = 0f

    /**
     * Escala del ajuste «Vibración en los juegos». La cambia [AppPrefs.setGameRumble]
     * en vivo y se aplica al instante: Apagada cancela ya, aunque el juego siga
     * pidiendo el mismo nivel y no llegue ninguna orden nueva.
     */
    var scale: Float = RumblePref.NORMAL.factor
        set(value) {
            field = if (value.isNaN()) 0f else value.coerceIn(0f, 1f)
            if (attached) execute(motor.set(raw * field, nowMs(), immediate = true))
        }

    /**
     * Quien tiene el reloj (el servicio del enlace): se le avisa tras cada
     * orden para que lo arranque si hace falta, también cuando la orden nace
     * en Ajustes y no de un RUMBLE.
     */
    var clock: (() -> Unit)? = null

    /** Hay motor que mover. Sin él, Ajustes lo dice y apaga los chips. */
    val hasMotor: Boolean
        get() = vibrator?.hasVibrator() == true

    /** El motor de juego se cree encendido: los hápticos de los botones se saltan. */
    val isOn: Boolean
        get() = motor.isOn

    /** El reloj tiene trabajo (renovar, comprobar el hueco tras un Stop o reintentar). */
    val busy: Boolean
        get() = motor.busy

    /** Solo para los tests: la última onda mandada al vibrador. */
    internal var lastEffect: VibrationEffect? = null
        private set

    /** Lo llama la app al arrancar (actividad y servicio; es idempotente). */
    fun attach(context: Context) {
        val app = context.applicationContext
        val v = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            (app.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as? VibratorManager)?.defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            app.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
        }
        vibrator = v
        amplitudes = v?.hasAmplitudeControl() == true
        attached = true
        scale = RumblePref.scale(AppPrefs.gameRumble(app))
    }

    /** Nivel crudo 0..1 de [RumbleTrack] (Start o Change); 0 = Stop. */
    fun set(rawLevel: Float) {
        raw = if (rawLevel.isNaN()) 0f else rawLevel.coerceIn(0f, 1f)
        execute(motor.set(raw * scale, nowMs()))
    }

    /** Stop de la máquina: el motor cancela cuando pase el hueco de silencio. */
    fun stop() = set(0f)

    /** Un tic del reloj (o la comprobación del hueco): renovar, cancelar o reintentar. */
    fun tick() = execute(motor.tick(nowMs()))

    /** Para el motor y olvida el nivel. Al perder el enlace, pasar a puntero o irse al fondo. */
    fun reset() {
        raw = 0f
        execute(motor.reset())
    }

    /**
     * Un pulso corto para el botón «Probar» de Ajustes. Mientras vibra el
     * juego no hace nada: un one-shot lo cortaría (o el sistema lo ignoraría).
     */
    fun pulse(scale: Float, ms: Long = 300) {
        val v = vibrator ?: return
        val strength = scale.coerceIn(0f, 1f)
        if (strength <= 0f || isOn) return
        val amp = if (amplitudes) amplitude(strength) else VibrationEffect.DEFAULT_AMPLITUDE
        send(v, VibrationEffect.createOneShot(ms, amp))
    }

    private fun execute(order: MotorOrder?) {
        when (order) {
            null -> return
            is MotorOrder.Vibrate -> vibrate(order.level)
            MotorOrder.Cancel -> cancel()
        }
        clock?.invoke()
    }

    private fun vibrate(level: Float) {
        val v = vibrator ?: return
        val effect = waveform(level)
        lastEffect = effect
        send(v, effect)
    }

    private fun send(v: Vibrator, effect: VibrationEffect) {
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                v.vibrate(effect, VibrationAttributes.createForUsage(VibrationAttributes.USAGE_PHYSICAL_EMULATION))
            } else {
                v.vibrate(effect)
            }
        } catch (e: Exception) {
            Log.w(TAG, "Vibración: el motor no acepta la orden: $e")
        }
    }

    private fun cancel() {
        val v = vibrator ?: return
        try {
            v.cancel()
        } catch (e: Exception) {
            Log.w(TAG, "Vibración: no se pudo cancelar: $e")
        }
    }

    /**
     * Onda FINITA de [RumbleMotor.LEASE_MS]. Con control de amplitud, encendido
     * todo el rato a la amplitud que toca; sin él, la intensidad se finge con el
     * reparto encendido/apagado del ciclo (lo único que puede ese motor).
     */
    private fun waveform(level: Float): VibrationEffect {
        val lease = RumbleMotor.LEASE_MS
        if (amplitudes) {
            val segment = lease / SEGMENTS
            return VibrationEffect.createWaveform(
                LongArray(SEGMENTS) { segment },
                IntArray(SEGMENTS) { amplitude(level) },
                -1
            )
        }
        val on = (CYCLE_MS * level).toLong().coerceIn(10L, CYCLE_MS)
        val off = CYCLE_MS - on
        val cycles = (lease / CYCLE_MS).toInt()
        val timings = LongArray(cycles * 2) { i -> if (i % 2 == 0) on else off }
        val amps = IntArray(cycles * 2) { i -> if (i % 2 == 0) VibrationEffect.DEFAULT_AMPLITUDE else 0 }
        return VibrationEffect.createWaveform(timings, amps, -1)
    }

    /** 0..1 → 1..255 (el 0 apagaría el motor en vez de vibrar flojito). */
    private fun amplitude(strength: Float): Int =
        (strength * 255f).toInt().coerceIn(1, 255)

    /** Milisegundos monótonos (el reloj de la máquina y del motor). */
    fun nowMs(): Long = SystemClock.elapsedRealtime()
}
