import Foundation

/// Las dos decisiones del giro del mando universal, puras para poder
/// probarlas. Las mismas que en Android.
enum PadAim {
    /// ¿El móvil manda el giro a cero? Solo con el mando universal
    /// confirmado y sin haber dicho que sí. Las dos condiciones importan: sin
    /// el eco del modo los paquetes todavía alimentan lo de antes (el
    /// puntero, por ejemplo) y el giro a cero le congelaría el cursor, y
    /// Switch y RetroArch comparten pantalla y formato con el mando universal
    /// pero apuntan con el giro igual que siempre.
    static func motionOff(universalPad: Bool, operative: Bool, pref: String) -> Bool {
        universalPad && operative && pref != AppPrefs.padAimOn
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
