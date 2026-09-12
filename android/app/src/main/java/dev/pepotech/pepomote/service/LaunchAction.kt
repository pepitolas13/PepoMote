package dev.pepotech.pepomote.service

/**
 * Accesos directos del icono (`xml/shortcuts.xml`): a qué pantalla ir al
 * arrancar. Función pura, testeada en LaunchActionTest.
 */
enum class LaunchAction {
    Pointer, Dolphin, WiiU, Nunchuk;

    companion object {
        private const val PREFIX = "dev.pepotech.pepomote.action."

        /** La acción del Intent, o null si no es uno de nuestros accesos. */
        fun parse(action: String?): LaunchAction? = when (action) {
            PREFIX + "POINTER" -> Pointer
            PREFIX + "DOLPHIN" -> Dolphin
            PREFIX + "WIIU" -> WiiU
            PREFIX + "NUNCHUK" -> Nunchuk
            else -> null
        }
    }
}
