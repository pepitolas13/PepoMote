package dev.pepotech.pepomote.service

/** Saved requests are separate for Wii U and Switch. */
object PadPreference {
    fun key(mode: String): String? = when (mode) {
        "cemu" -> "cemuPad"
        "switch" -> "switchPad"
        else -> null
    }

    fun normalize(mode: String, pad: String?): String = when (mode) {
        "switch" -> "pro"
        else -> if (pad == "wiimote") "wiimote" else "gamepad"
    }

    /** Switch always uses Pro, including obsolete saved or received controller names. */
    fun effective(mode: String, pad: String): String = if (mode == "switch") "pro" else pad
}
