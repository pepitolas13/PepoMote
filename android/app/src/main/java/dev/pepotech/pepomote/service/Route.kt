package dev.pepotech.pepomote.service

/** Pantalla del mando que toca enseñar. */
enum class PadScreen { GamePad, Wii, Nunchuk }

/**
 * Intención pendiente del usuario: `WiiU` = tocó la tarjeta Wii U o el chip
 * Wii U y aún no ha llegado ningún eco/difusión de `mode` posterior. Mientras
 * dura, la pantalla GamePad se enseña de forma optimista.
 */
enum class PadIntent { None, WiiU }

/**
 * Routing del mando: función pura de (estado del enlace, intención pendiente),
 * sin Android, testeada en RouteTest.
 */
object Route {
    const val WARN_NEEDS_13 = "El PC necesita PepoMote 1.3 para Wii U"
    const val WARN_PLAYER_1 = "Solo el Jugador 1 cambia el modo"

    /**
     * Modo Wii U activo como GamePad/Pro: el receptor confirmó `cemu` y este
     * móvil (mando) no ha elegido ser Mando de Wii. Solo entonces se emiten
     * paquetes de 80 bytes.
     */
    fun isGamePad(link: UiLink): Boolean =
        link is UiLink.Connected && link.mode == LinkState.MODE_CEMU &&
            link.role == LinkState.ROLE_WIIMOTE && link.pad != LinkState.PAD_WIIMOTE

    /**
     * - Nunchuk si el enlace es un Nunchuk (nunca es GamePad, pida lo que pida);
     * - GamePad si el receptor confirmó `cemu` y este mando no es Mando de Wii;
     * - GamePad si hay intención Wii U pendiente, conectando o conectado;
     * - si no, el layout Wii (vertical / NES apaisado según orientación).
     */
    fun route(link: UiLink, intent: PadIntent): PadScreen {
        if (link is UiLink.Connected && link.role == LinkState.ROLE_NUNCHUK) return PadScreen.Nunchuk
        if (isGamePad(link)) return PadScreen.GamePad
        if (intent == PadIntent.WiiU && (link is UiLink.Connected || link is UiLink.Connecting)) {
            return PadScreen.GamePad
        }
        return PadScreen.Wii
    }

    /** Intención resultante y, si toca, el aviso a enseñar. */
    data class Outcome(val intent: PadIntent, val warning: String?)

    /**
     * Llega un eco/difusión de `mode`: consume la intención Wii U. Si el modo
     * no es `cemu`, el layout Wii vuelve con un aviso: el PC no soporta Wii U
     * (receptor antiguo: `ok` sin "cemu" en `modes`) o, si sí lo soporta pero
     * este móvil no es el Jugador 1, que solo él cambia el modo.
     */
    fun afterModeEcho(intent: PadIntent, mode: String, link: UiLink): Outcome {
        if (intent != PadIntent.WiiU) return Outcome(intent, null)
        if (mode == LinkState.MODE_CEMU) return Outcome(PadIntent.None, null)
        val c = link as? UiLink.Connected
        val warning = if (c != null && c.supportsCemu && c.slot != 0) WARN_PLAYER_1 else WARN_NEEDS_13
        return Outcome(PadIntent.None, warning)
    }
}
