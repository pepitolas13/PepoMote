package dev.pepotech.pepomote.control

/**
 * Las dos decisiones del giro del mando universal, puras y sin Android para
 * poder probarlas (las mismas en iOS).
 */
object PadAim {
    /**
     * ¿El móvil manda el giro a cero? Solo con el mando universal
     * confirmado, y si el giro está apagado o hay un teclado abierto. Las dos
     * primeras condiciones importan: sin el eco del modo los paquetes todavía
     * alimentan lo de antes (el puntero, por ejemplo) y el giro a cero le
     * congelaría el cursor, y Switch y RetroArch comparten pantalla y formato
     * con el mando universal pero apuntan con el giro igual que siempre.
     *
     * [keyboardOpen]: escribiendo, el móvil se menea en la mano y la cámara
     * del juego se iría sola mientras se busca. Es el mismo remedio que la
     * pose congelada del modo puntero.
     */
    fun motionOff(
        universalPad: Boolean,
        operative: Boolean,
        pref: String,
        keyboardOpen: Boolean = false
    ): Boolean =
        universalPad && operative && (keyboardOpen || pref != AppPrefs.PAD_AIM_ON)

    /**
     * ¿Sale la pregunta de la primera vez? Con el mando universal
     * confirmado, sin contestar, con el lado del apaisado ya elegido (una
     * pregunta cada vez) y con la cabecera fuera de en medio: plegada, o de
     * las que no se pliegan solas.
     */
    fun shouldAsk(
        universalPad: Boolean,
        operative: Boolean,
        pref: String,
        sideChosen: Boolean,
        headerExpanded: Boolean,
        autoCollapse: Boolean
    ): Boolean =
        universalPad && operative && pref == AppPrefs.PAD_AIM_ASK && sideChosen &&
            (!headerExpanded || !autoCollapse)
}
