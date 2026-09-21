import Foundation

/// Las dos decisiones del giro del mando universal, puras para poder
/// probarlas. Las mismas que en Android.
enum PadAim {
    /// ¿El móvil manda el giro a cero? Solo con el mando universal
    /// confirmado, y si el giro está apagado o hay un teclado abierto. Las dos
    /// primeras condiciones importan: sin el eco del modo los paquetes todavía
    /// alimentan lo de antes (el puntero, por ejemplo) y el giro a cero le
    /// congelaría el cursor, y Switch y RetroArch comparten pantalla y formato
    /// con el mando universal pero apuntan con el giro igual que siempre.
    ///
    /// `keyboardOpen`: escribiendo, el móvil se menea en la mano y la cámara
    /// del juego se iría sola mientras se busca. Es el mismo remedio que la
    /// pose congelada del modo puntero.
    static func motionOff(universalPad: Bool, operative: Bool, pref: String, keyboardOpen: Bool = false) -> Bool {
        universalPad && operative && (keyboardOpen || pref != AppPrefs.padAimOn)
    }

    /// ¿Sale la pregunta de la primera vez? Con el mando universal
    /// confirmado, sin contestar, con el lado del apaisado ya elegido (una
    /// pregunta cada vez) y con la cabecera fuera de en medio: plegada, o de
    /// las que no se pliegan solas.
    static func shouldAsk(
        universalPad: Bool,
        operative: Bool,
        pref: String,
        sideChosen: Bool,
        headerExpanded: Bool,
        autoCollapse: Bool
    ) -> Bool {
        // La cabecera desplegada tapa el sitio de la tarjeta, salvo que no
        // vaya a plegarse sola (lector de pantalla)
        let headerOut = !headerExpanded || !autoCollapse
        return universalPad && operative && pref == AppPrefs.padAimAsk && sideChosen && headerOut
    }
}
