package dev.pepotech.pepomote.control

import android.content.Context
import android.os.Build
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager

/**
 * Motor de la vibración que piden los juegos (RUMBLE del receptor,
 * PROTOCOL.md §4.5), aparte de la háptica de los botones ([UiSounds] y
 * `PressBit`). Es el gemelo de `GameRumble.swift`.
 *
 * Android no tiene un «motor continuo con intensidad» portable, así que se
 * usa lo mejor que haya:
 *  - API 26+: una onda que se repite (`createWaveform(..., repeat = 0)`) con
 *    amplitud, así que la intensidad es de verdad y parar es inmediato.
 *  - Sin control de amplitud: la misma onda sin amplitudes, con el nivel
 *    metido en el reparto encendido/apagado del ciclo.
 * El nivel se vuelve a aplicar en cada orden, así que cambiar el chip de
 * Ajustes se nota al instante.
 *
 * Todo lo que toca el [Vibrator] va bajo un candado: las órdenes llegan del
 * hilo del socket y la prueba de Ajustes del hilo de la interfaz.
 */
object GameRumble {
    /** Ciclo de la onda que se repite; corto para que parar se note al momento. */
    private const val CYCLE_MS = 60L

    private val lock = Any()

    @Volatile
    private var vibrator: Vibrator? = null

    @Volatile
    private var amplitudes = false

    /** Hay motor que mover. Sin él, Ajustes lo dice y apaga los chips. */
    val hasMotor: Boolean
        get() = vibrator?.hasVibrator() == true

    /** Lo llama la app al arrancar; sin esto no hay motor que tocar. */
    fun attach(context: Context) {
        val app = context.applicationContext
        val v = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            (app.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as? VibratorManager)?.defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            app.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
        }
        vibrator = v
        amplitudes = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && v?.hasAmplitudeControl() == true
    }

    /** Aplica una orden de [RumbleTrack] con la escala del ajuste ya puesta. */
    fun apply(command: RumbleCommand, scale: Float) {
        when (command) {
            is RumbleCommand.Start -> set(command.level * scale)
            is RumbleCommand.Change -> set(command.level * scale)
            RumbleCommand.Stop -> set(0f)
        }
    }

    /** Un pulso corto para el botón «Probar» de Ajustes. */
    fun pulse(scale: Float, ms: Long = 300) {
        val v = vibrator ?: return
        val strength = scale.coerceIn(0f, 1f)
        if (strength <= 0f) return
        synchronized(lock) {
            try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    val amp = if (amplitudes) amplitude(strength) else VibrationEffect.DEFAULT_AMPLITUDE
                    v.vibrate(VibrationEffect.createOneShot(ms, amp))
                } else {
                    @Suppress("DEPRECATION")
                    v.vibrate(ms)
                }
            } catch (_: Exception) {
            }
        }
    }

    /** Para el motor y olvida el nivel. Al perder el enlace o irse al fondo. */
    fun stop() = set(0f)

    private fun set(target: Float) {
        val v = vibrator ?: return
        val strength = target.coerceIn(0f, 1f)
        synchronized(lock) {
            if (strength <= 0f) {
                try {
                    v.cancel()
                } catch (_: Exception) {
                }
                return
            }
            try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    v.vibrate(waveform(strength))
                } else {
                    // Sin VibrationEffect: un patrón que se repite, sin amplitud
                    val on = (CYCLE_MS * strength).toLong().coerceAtLeast(10L)
                    @Suppress("DEPRECATION")
                    v.vibrate(longArrayOf(0, on, CYCLE_MS - on), 0)
                }
            } catch (_: Exception) {
            }
        }
    }

    /**
     * Onda que se repite. Con control de amplitud, encendido todo el ciclo a
     * la amplitud que toca; sin él, la intensidad se finge con el reparto
     * encendido/apagado del ciclo (lo único que puede ese motor).
     */
    private fun waveform(strength: Float): VibrationEffect {
        if (amplitudes) {
            return VibrationEffect.createWaveform(
                longArrayOf(CYCLE_MS),
                intArrayOf(amplitude(strength)),
                0
            )
        }
        val on = (CYCLE_MS * strength).toLong().coerceAtLeast(10L)
        return VibrationEffect.createWaveform(longArrayOf(0, on, CYCLE_MS - on), 0)
    }

    /** 0..1 → 1..255 (el 0 apagaría el motor en vez de vibrar flojito). */
    private fun amplitude(strength: Float): Int =
        (strength * 255f).toInt().coerceIn(1, 255)

    /** Solo para los tests del reloj: milisegundos monótonos. */
    fun nowMs(): Long = SystemClock.elapsedRealtime()
}
