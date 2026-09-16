package dev.pepotech.pepomote.service

import android.content.pm.ActivityInfo
import android.view.Surface
import kotlin.math.abs

/** Hacia dónde mira físicamente el móvil según el sensor de giro. */
enum class Facing {
    /** Derecho (posición natural). */
    Portrait,

    /** Girado a la izquierda: el borde superior (la cámara) queda a la izquierda. */
    LandscapeLeft,

    /** Girado a la derecha: el borde superior queda a la derecha. */
    LandscapeRight,

    /** Boca abajo, plano o desconocido: no se propone nada. */
    Other
}

/**
 * Propuesta de girar la pantalla con el giro automático del sistema
 * bloqueado: el móvil nota con el acelerómetro que lo has girado y te lo
 * PROPONE (un botón); nunca gira solo. Lógica pura, testeable: [facing]
 * clasifica los grados del sensor con histéresis y [proposal] dice qué
 * orientación pedir si no coincide con la de la pantalla.
 */
object RotationProposal {
    /** Media banda (°) alrededor de cada orientación: fuera de ella se conserva la anterior (histéresis). */
    const val BAND_DEG = 30

    /**
     * [degrees]: giro horario del móvil respecto a su posición natural (0..359;
     * -1 = plano o desconocido), como lo da `OrientationEventListener`. Solo
     * cambia de [Facing] dentro de ±[BAND_DEG] de la nueva orientación.
     */
    fun facing(degrees: Int, previous: Facing): Facing {
        if (degrees < 0) return previous
        val d = ((degrees % 360) + 360) % 360
        fun near(center: Int): Boolean =
            abs(d - center) <= BAND_DEG || abs(d - center - 360) <= BAND_DEG || abs(d - center + 360) <= BAND_DEG
        return when {
            near(0) -> Facing.Portrait
            near(90) -> Facing.LandscapeRight // giro horario: el borde superior a la derecha
            near(270) -> Facing.LandscapeLeft // giro antihorario: el borde superior a la izquierda
            near(180) -> Facing.Other // boca abajo
            else -> previous
        }
    }

    /**
     * Rotación de pantalla (`Surface.ROTATION_*`, respecto a la posición
     * natural) con la que la pantalla se vería derecha mirando así; null si
     * no hay nada que proponer.
     */
    fun wantedRotation(facing: Facing): Int? = when (facing) {
        Facing.Portrait -> Surface.ROTATION_0
        Facing.LandscapeLeft -> Surface.ROTATION_90
        Facing.LandscapeRight -> Surface.ROTATION_270
        Facing.Other -> null
    }

    /**
     * Orientación a pedir (`ActivityInfo.SCREEN_ORIENTATION_*`) para que la
     * pantalla quede como mira el móvil, o null si ya coincide (o no hay nada
     * que proponer). [naturalPortrait]: el aparato es vertical de fábrica
     * (un móvil); una tableta apaisada de fábrica tiene los nombres cruzados.
     */
    fun proposal(facing: Facing, displayRotation: Int, naturalPortrait: Boolean = true): Int? {
        val wanted = wantedRotation(facing) ?: return null
        if (wanted == displayRotation) return null
        return if (naturalPortrait) {
            when (wanted) {
                Surface.ROTATION_90 -> ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
                Surface.ROTATION_270 -> ActivityInfo.SCREEN_ORIENTATION_REVERSE_LANDSCAPE
                else -> ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
            }
        } else {
            when (wanted) {
                Surface.ROTATION_90 -> ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
                Surface.ROTATION_270 -> ActivityInfo.SCREEN_ORIENTATION_REVERSE_PORTRAIT
                else -> ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
            }
        }
    }
}

/**
 * Reloj de la propuesta: se propone cuando el móvil lleva [SETTLE_MS]
 * mirando hacia un lado distinto del de la pantalla; la propuesta caduca sola
 * a los [SHOW_MS] (y no vuelve hasta que el móvil cambie otra vez de lado), y
 * desaparece en cuanto móvil y pantalla coinciden. Pura: el tiempo entra por
 * parámetro.
 */
class RotationProposalMachine {
    companion object {
        /** Cuánto tiene que llevar el móvil girado antes de proponer (ms): ni un vistazo ni un vaivén. */
        const val SETTLE_MS = 900L

        /** Cuánto dura la propuesta en pantalla si no se toca (ms). */
        const val SHOW_MS = 6000L
    }

    var facing: Facing = Facing.Other
        private set

    /** Propuesta en pantalla (`ActivityInfo.SCREEN_ORIENTATION_*`), o null. */
    var proposal: Int? = null
        private set

    private var mismatchSince = -1L
    private var shownAt = -1L

    /** Propuesta ya caducada: no se repite hasta que el móvil cambie de lado. */
    private var spent: Int? = null

    /**
     * Muestra del sensor. [enabled]: con el mando en pantalla, orientación
     * libre y el giro automático del sistema bloqueado (si está activo, ya
     * gira el sistema). Devuelve la propuesta vigente tras procesarla.
     */
    fun onDegrees(degrees: Int, displayRotation: Int, naturalPortrait: Boolean, nowMs: Long, enabled: Boolean = true): Int? {
        facing = RotationProposal.facing(degrees, facing)
        val wanted = if (enabled) RotationProposal.proposal(facing, displayRotation, naturalPortrait) else null
        if (wanted == null) {
            // coinciden (o nada que proponer): sin propuesta, y la próxima cuenta de cero
            proposal = null
            spent = null
            mismatchSince = -1L
            return null
        }
        if (spent != null && spent != wanted) spent = null // otro lado: se puede volver a proponer
        if (proposal != null && proposal != wanted) {
            // cambió de lado con la propuesta en pantalla: la nueva, con su asentamiento
            proposal = null
            mismatchSince = nowMs
        }
        if (proposal == wanted) {
            if (nowMs - shownAt >= SHOW_MS) {
                proposal = null
                spent = wanted
            }
            return proposal
        }
        if (spent == wanted) return null
        if (mismatchSince < 0) mismatchSince = nowMs
        if (nowMs - mismatchSince >= SETTLE_MS) {
            proposal = wanted
            shownAt = nowMs
        }
        return proposal
    }

    /** El usuario la acepta: devuelve la orientación a pedir (o null si no había propuesta). */
    fun accept(): Int? {
        val p = proposal ?: return null
        proposal = null
        spent = null
        mismatchSince = -1L
        return p
    }

    /** Fuera del mando (o sensor apagado): nada en pantalla. */
    fun clear() {
        proposal = null
        spent = null
        mismatchSince = -1L
    }
}
