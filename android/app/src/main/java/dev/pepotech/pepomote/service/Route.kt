package dev.pepotech.pepomote.service

import androidx.annotation.StringRes
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.sensor.Frame
import dev.pepotech.pepomote.net.ReceiverCapabilities

/** Pantalla del mando que toca enseñar. */
enum class PadScreen { GamePad, Wii, Nunchuk }

/**
 * Intención pendiente del usuario: `WiiU` = tocó la tarjeta Wii U o el chip
 * Wii U y aún no ha llegado ningún eco/difusión de `mode` posterior. Mientras
 * dura, la pantalla GamePad se enseña de forma optimista.
 */
enum class PadIntent { None, WiiU, Switch, RetroArch }

/**
 * Routing del mando: función pura de (estado del enlace, intención pendiente),
 * sin Android, testeada en RouteTest.
 */
object Route {
    /** Modes the owner can select from a compact controller menu. */
    fun availableModes(link: UiLink.Connected): List<String> {
        if (link.slot != 0 || link.role != LinkState.ROLE_WIIMOTE) return emptyList()
        return ReceiverCapabilities.modes(link.platform, link.supportsCemu, link.supportsSwitch, link.supportsRetroArch)
    }

    fun isAndroidReceiver(link: UiLink): Boolean =
        (link as? UiLink.Connected)?.platform == ReceiverCapabilities.ANDROID

    fun selectMode(requested: String?, link: UiLink.Connected): String =
        if (link.platform == ReceiverCapabilities.ANDROID)
            ReceiverCapabilities.select(requested, link.mode, link.platform, link.supportsCemu, link.supportsSwitch, link.supportsRetroArch)
        else requested ?: link.mode

    @StringRes
    val WARN_NEEDS_13: Int = R.string.warn_needs_13

    @StringRes
    val WARN_PLAYER_1: Int = R.string.warn_player_1

    @StringRes
    val WARN_NEEDS_SWITCH: Int = R.string.warn_needs_switch

    @StringRes
    val WARN_NEEDS_RETROARCH: Int = R.string.warn_needs_retroarch

    /**
     * Modo Wii U activo como GamePad/Pro: el receptor confirmó `cemu` y este
     * móvil (mando) no ha elegido ser Mando de Wii. Solo entonces se emiten
     * paquetes de 80 bytes.
     */
    fun isGamePad(link: UiLink): Boolean = link is UiLink.Connected &&
        link.role == LinkState.ROLE_WIIMOTE && when (link.mode) {
            LinkState.MODE_CEMU -> link.supportsCemu && link.pad in setOf(LinkState.PAD_GAMEPAD, LinkState.PAD_PRO)
            LinkState.MODE_SWITCH -> link.supportsSwitch
            // RetroArch: el mando apaisado es el RetroPad; el de NES y la
            // pistola son los layouts de Wii de siempre (72 bytes)
            LinkState.MODE_RETROARCH -> link.supportsRetroArch && link.pad == LinkState.PAD_RETROPAD
            else -> false
        }

    /** Pending intent wins over the previous receiver mode while the echo is in flight. */
    fun displayMode(link: UiLink, intent: PadIntent): String = when (intent) {
        PadIntent.WiiU -> LinkState.MODE_CEMU
        PadIntent.Switch -> LinkState.MODE_SWITCH
        PadIntent.RetroArch -> LinkState.MODE_RETROARCH
        PadIntent.None -> (link as? UiLink.Connected)?.mode ?: LinkState.MODE_CEMU
    }

    /**
     * - Nunchuk si el enlace es un Nunchuk (nunca es GamePad, pida lo que pida);
     * - GamePad si el receptor confirmó `cemu` y este mando no es Mando de Wii;
     * - GamePad si hay intención Wii U pendiente y hay enlace (conectando, conectado o reconectando);
     * - si no, el layout Wii (vertical / NES apaisado según orientación).
     */
    fun route(link: UiLink, intent: PadIntent): PadScreen {
        if (link is UiLink.Connected && link.role == LinkState.ROLE_NUNCHUK) return PadScreen.Nunchuk
        if (intent != PadIntent.None && link.alive) return PadScreen.GamePad
        if (isGamePad(link)) return PadScreen.GamePad
        return PadScreen.Wii
    }

    /**
     * Layout Wii apaisado = mando + Nunchuk en un solo móvil: modo Dolphin,
     * este móvil es mando y el receptor ha confirmado el Nunchuk propio (un
     * receptor antiguo no lo confirma y se queda el NES de siempre).
     */
    fun wiiLandscapeNunchuk(link: UiLink): Boolean =
        link is UiLink.Connected && link.mode == LinkState.MODE_DOLPHIN &&
            link.role == LinkState.ROLE_WIIMOTE && link.ownNunchuk

    /** Modo RetroArch confirmado por un receptor que lo entiende (este móvil es mando). */
    fun isRetroArch(link: UiLink): Boolean =
        link is UiLink.Connected && link.mode == LinkState.MODE_RETROARCH &&
            link.supportsRetroArch && link.role == LinkState.ROLE_WIIMOTE

    /** RetroArch como mando de NES: el Mando Wii de lado, fijo en apaisado. */
    fun retroNes(link: UiLink): Boolean =
        isRetroArch(link) && (link as UiLink.Connected).pad == LinkState.PAD_NES

    /** RetroArch como pistola de luz: el Mando Wii apuntando (B dispara, A recarga). */
    fun retroGun(link: UiLink): Boolean =
        isRetroArch(link) && (link as UiLink.Connected).pad == LinkState.PAD_GUN

    /**
     * Pantallas del mando que van fijas en apaisado (el sensor solo decide
     * entre los dos apaisados): el GamePad de Wii U y el mando + Nunchuk de
     * Dolphin, y el mando de NES de RetroArch. El mando se gira solo: no
     * hace falta girar el móvil.
     */
    fun forcesLandscape(link: UiLink, intent: PadIntent): Boolean =
        route(link, intent) == PadScreen.GamePad || wiiLandscapeNunchuk(link) || retroNes(link)

    /**
     * Con el mando de lado (NES) el móvil ES un Mando de Wii girado: en
     * Dolphin y en Wii U como Mando de Wii el juego espera el mando con el
     * extremo IR a la izquierda y aplica él mismo el giro (cruceta y
     * acelerómetro). Solo entonces: en modo puntero las flechas siguen
     * siendo flechas del PC.
     */
    fun sidewaysDpad(link: UiLink): Boolean {
        val c = link as? UiLink.Connected ?: return false
        return c.mode == LinkState.MODE_DOLPHIN || (c.mode == LinkState.MODE_CEMU && c.pad == LinkState.PAD_WIIMOTE)
    }

    /**
     * Giro de los sensores con el mando de lado (NES). Como mando girado
     * (Dolphin, Wii U-Mando): el móvil con el borde superior a la izquierda
     * (ROTATION_90) ya es el mando con el IR a la izquierda, y con el borde
     * a la derecha (ROTATION_270) se gira 180° para que dé igual hacia dónde
     * se gire (el volante de Mario Kart gira bien en las dos). En modo
     * puntero el móvil de lado apunta con su borde largo, como el GamePad:
     * los sensores se remapean con la rotación de la pantalla y el cursor
     * sigue la mano.
     */
    fun sidewaysRotation(link: UiLink, displayRotation: Int): Int =
        if (sidewaysDpad(link)) {
            if (displayRotation == Frame.ROTATION_270) Frame.ROTATION_180 else Frame.ROTATION_0
        } else {
            displayRotation
        }

    /** Intención resultante y, si toca, el aviso a enseñar (recurso de texto). */
    data class Outcome(val intent: PadIntent, @StringRes val warning: Int?)

    /**
     * Llega un eco/difusión de `mode`: consume la intención Wii U. Si el modo
     * no es `cemu`, el layout Wii vuelve con un aviso: el PC no soporta Wii U
     * (receptor antiguo: `ok` sin "cemu" en `modes`) o, si sí lo soporta pero
     * este móvil no es el Jugador 1, que solo él cambia el modo. Si el modo lo
     * decidió el PC (`byPc`: modo automático al abrir o cerrar un emulador) la
     * intención se descarta sin aviso: el notice del PC ya lo explica.
     */
    fun afterModeEcho(intent: PadIntent, mode: String, link: UiLink, byPc: Boolean = false): Outcome {
        if (intent == PadIntent.None) return Outcome(intent, null)
        val wanted = when (intent) {
            PadIntent.Switch -> LinkState.MODE_SWITCH
            PadIntent.RetroArch -> LinkState.MODE_RETROARCH
            else -> LinkState.MODE_CEMU
        }
        if (mode == wanted || byPc) return Outcome(PadIntent.None, null)
        val c = link as? UiLink.Connected
        val supported = when (intent) {
            PadIntent.Switch -> c?.supportsSwitch == true
            PadIntent.RetroArch -> c?.supportsRetroArch == true
            else -> c?.supportsCemu == true
        }
        val warning = if (supported && c?.slot != 0) WARN_PLAYER_1
            else when (intent) {
                PadIntent.Switch -> WARN_NEEDS_SWITCH
                PadIntent.RetroArch -> WARN_NEEDS_RETROARCH
                else -> WARN_NEEDS_13
            }
        return Outcome(PadIntent.None, warning)
    }
}
