package dev.pepotech.pepomote.service

/** Saved requests are separate for Wii U, Switch and RetroArch. */
object PadPreference {
    /** Mandos de RetroArch (mensaje `pad`, PROTOCOL.md §3). */
    val RETRO_PADS = listOf(LinkState.PAD_RETROPAD, LinkState.PAD_NES, LinkState.PAD_GUN)

    fun key(mode: String): String? = when (mode) {
        "cemu" -> "cemuPad"
        "switch" -> "switchPad"
        "retroarch" -> "retroPad"
        else -> null
    }

    fun normalize(mode: String, pad: String?): String = when (mode) {
        "switch" -> "pro"
        "retroarch" -> if (pad in RETRO_PADS) pad!! else LinkState.PAD_RETROPAD
        else -> if (pad == "wiimote") "wiimote" else "gamepad"
    }

    /**
     * Switch always uses Pro, including obsolete saved or received controller
     * names; RetroArch only its three pads (anything else is the RetroPad).
     */
    fun effective(mode: String, pad: String): String = when (mode) {
        "switch" -> "pro"
        "retroarch" -> normalize(mode, pad)
        else -> pad
    }
}
