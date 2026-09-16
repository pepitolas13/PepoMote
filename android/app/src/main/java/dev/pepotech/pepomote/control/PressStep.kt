package dev.pepotech.pepomote.control

/**
 * Qué hace un dedo apoyado cuando se mueve por encima de los botones, según
 * los dos ajustes de pulsación. Función pura, la misma en las tres apps (como
 * [DpadModel] y `dpad.rs`): se prueba en la JVM.
 *
 * - «Pulsar deslizando» ([slide]): manda lo que hay bajo el dedo; salirse de
 *   un botón lo suelta y entrar en otro lo pulsa, aunque el dedo naciera en
 *   el vacío.
 * - «Mantener al salir del botón» ([sticky], de serie): lo que se pulsó
 *   sigue pulsado hasta levantar el dedo, aunque se salga del botón.
 * - Sin ninguno de los dos: salirse del botón lo suelta y el dedo se queda
 *   libre; ya no vuelve a coger nada hasta levantarlo (lo de Compose de
 *   siempre, `tryAwaitRelease`).
 */
object PressStep {
    /** Lo que le toca al dedo tras el movimiento. */
    sealed interface Move {
        /** Nada cambia: sigue con lo que tuviera (o sin nada). */
        data object Keep : Move

        /** Pasa a [bit]; `null` = se queda libre, sin nada pulsado. */
        data class To(val bit: Int?) : Move
    }

    /**
     * [held]: el bit que lleva ese dedo (null = ninguno). [over]: el bit que
     * hay bajo él ahora (null = ninguno).
     */
    fun next(slide: Boolean, sticky: Boolean, held: Int?, over: Int?): Move = when {
        slide -> if (over == held) Move.Keep else Move.To(over)
        sticky -> Move.Keep
        held != null && over != held -> Move.To(null)
        else -> Move.Keep
    }
}
