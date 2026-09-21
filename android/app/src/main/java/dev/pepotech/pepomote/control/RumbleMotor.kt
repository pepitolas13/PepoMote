package dev.pepotech.pepomote.control

import kotlin.math.abs

/** Lo que la política le manda al `Vibrator`: una orden FINITA a un nivel, o cancelar. */
sealed class MotorOrder {
    data class Vibrate(val level: Float) : MotorOrder()
    object Cancel : MotorOrder()
}

/**
 * Política del motor de la vibración de los juegos: la parte pura de
 * [GameRumble] (solo hilo principal, reloj inyectado, sin Android). Entra el
 * nivel efectivo que pide la máquina [RumbleTrack] (ya con la escala del
 * ajuste; 0 = parar), los tics del reloj del servicio y los reinicios; sale
 * la orden para el vibrador, o nada.
 *
 * Dos reglas que son las que evitan que un móvil se quede vibrando:
 *
 *  - **Ninguna orden es infinita.** Cada [MotorOrder.Vibrate] es una onda de
 *    [LEASE_MS] que el propio motor apaga al acabar aunque ningún `cancel()`
 *    llegue o el fabricante lo pierda; mientras el receptor siga refrescando,
 *    el tic la renueva a partir de [RENEW_AFTER_MS] (un segundo de margen si
 *    el hilo principal se atasca). Si todo lo demás falla, el motor calla solo.
 *  - **Un Stop no cancela en el acto**: espera [GAP_MS] de silencio. Los
 *    juegos de Wii pulsan el motor (encendido/apagado cada pocos fotogramas)
 *    para fingir intensidad y el receptor manda cada cambio; cancelar y
 *    arrancar a ese ritmo bombardea al sistema con `vibrate()`/`cancel()`,
 *    que es como una implementación que los procese en diferido acaba con un
 *    `vibrate()` aplicado después del último `cancel()`. Un Start dentro de
 *    la orden viva es un no-op: los pulsos se funden en una vibración continua.
 *
 * Tras cancelar se repite `cancel()` en los [IDLE_CANCELS] tics siguientes
 * (no cuesta nada y cubre una cancelación perdida), y hasta entonces [busy]
 * le pide al servicio que mantenga el reloj.
 */
class RumbleMotor {
    companion object {
        /** Duración de cada orden al vibrador: pase lo que pase, el motor calla al cabo de esto. */
        const val LEASE_MS = 3000L

        /** A partir de aquí el tic vuelve a mandar la orden (mientras haya nivel). */
        const val RENEW_AFTER_MS = 2000L

        /** Silencio seguido que hace falta para cancelar de verdad. */
        const val GAP_MS = 80L

        /** Un cambio de nivel menor no reemite la orden (cada reemisión es un corte de un instante). */
        const val LEVEL_EPS = 0.1f

        /** Veces que se repite `cancel()` en los tics de después de parar. */
        const val IDLE_CANCELS = 2
    }

    /** Nivel que pide el juego ahora (0 = quiere parar). */
    private var target = 0f

    /** Desde cuándo pide parar, con el motor aún encendido. */
    private var zeroSince: Long? = null

    /** Nivel de la orden viva (0 = motor apagado, que sepamos). */
    private var motor = 0f
    private var issuedAt = 0L
    private var idleLeft = 0

    /** El motor se cree encendido (para saltarse los hápticos de los botones). */
    val isOn: Boolean
        get() = motor > 0f

    /** Hay algo que el reloj tenga que atender: renovar, comprobar el hueco o reintentar. */
    val busy: Boolean
        get() = target > 0f || motor > 0f || zeroSince != null || idleLeft > 0

    /**
     * Nivel efectivo nuevo. Con el motor apagado arranca al instante (la
     * latencia importa); encendido, el cambio espera al tic, salvo
     * `immediate` (el chip de Ajustes), que reemite o cancela ya.
     */
    fun set(level: Float, nowMs: Long, immediate: Boolean = false): MotorOrder? {
        val l = if (level.isNaN()) 0f else level.coerceIn(0f, 1f)
        if (l <= 0f) {
            target = 0f
            if (motor <= 0f) return null
            if (immediate) return cancel()
            if (zeroSince == null) zeroSince = nowMs
            return null
        }
        zeroSince = null
        target = l
        if (motor <= 0f) return issue(l, nowMs)
        if (immediate && abs(l - motor) > LEVEL_EPS) return issue(l, nowMs)
        return null
    }

    /** El reloj (cada 0,1 s mientras [busy]) y la comprobación del hueco tras un Stop. */
    fun tick(nowMs: Long): MotorOrder? {
        if (target > 0f) {
            if (motor <= 0f) return issue(target, nowMs)
            if (abs(target - motor) > LEVEL_EPS || nowMs - issuedAt >= RENEW_AFTER_MS) return issue(target, nowMs)
            return null
        }
        if (motor > 0f) {
            val since = zeroSince ?: return cancel()
            return if (nowMs - since >= GAP_MS) cancel() else null
        }
        zeroSince = null
        if (idleLeft > 0) {
            idleLeft--
            return MotorOrder.Cancel
        }
        return null
    }

    /** Sesión nueva, modo puntero, app fuera de pantalla o enlace caído: cancela ya (aunque se crea apagado). */
    fun reset(): MotorOrder {
        target = 0f
        return cancel()
    }

    private fun issue(level: Float, nowMs: Long): MotorOrder {
        motor = level
        issuedAt = nowMs
        idleLeft = 0
        zeroSince = null
        return MotorOrder.Vibrate(level)
    }

    private fun cancel(): MotorOrder {
        motor = 0f
        zeroSince = null
        idleLeft = IDLE_CANCELS
        return MotorOrder.Cancel
    }
}
