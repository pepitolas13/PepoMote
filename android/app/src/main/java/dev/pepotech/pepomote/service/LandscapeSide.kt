package dev.pepotech.pepomote.service

import dev.pepotech.pepomote.sensor.Frame

/** Orientación que pide la actividad para un [LandscapeSide]. */
enum class LandscapeOrientation { SensorLandscape, Landscape, ReverseLandscape }

/**
 * Hacia qué lado se gira el mando + Nunchuk (apaisado fijo). El sensor de
 * giro de algunos móviles le da la vuelta al mando (180°) con muy poco o
 * con falsos positivos, así que el lado se elige una vez y se queda:
 * [Left] = el móvil girado a la izquierda (el borde superior, el de la
 * cámara, queda a la izquierda), [Right] = girado a la derecha. [Unset] =
 * todavía no se ha elegido: la primera vez el sensor gira el mando como
 * siempre y se pregunta encima si está bien. [Sensor] = que decida el
 * sensor, como antes (Ajustes). Puro: testeable.
 */
enum class LandscapeSide(val pref: String) {
    Unset(""), Sensor("sensor"), Left("left"), Right("right");

    val orientation: LandscapeOrientation
        get() = when (this) {
            Left -> LandscapeOrientation.Landscape
            Right -> LandscapeOrientation.ReverseLandscape
            Unset, Sensor -> LandscapeOrientation.SensorLandscape
        }

    /** Darle la vuelta (180°); lo que no es un lado, tal cual. */
    fun flipped(): LandscapeSide = when (this) {
        Left -> Right
        Right -> Left
        else -> this
    }

    companion object {
        fun fromPref(s: String): LandscapeSide = values().firstOrNull { it != Unset && it.pref == s } ?: Unset

        /**
         * Lado que se está viendo, con la pantalla ya apaisada: en un móvil
         * (vertical de fábrica) ROTATION_90 es el borde superior a la
         * izquierda y ROTATION_270 a la derecha; en una tablet apaisada de
         * fábrica, ROTATION_0 y ROTATION_180.
         */
        fun current(displayRotation: Int): LandscapeSide =
            if (displayRotation == Frame.ROTATION_90 || displayRotation == Frame.ROTATION_0) Left else Right

        /** El que manda: el provisional («Darle la vuelta» sin confirmar) si lo hay; si no, el guardado. */
        fun effective(saved: LandscapeSide, provisional: LandscapeSide?): LandscapeSide = provisional ?: saved
    }
}
