package dev.pepotech.pepomote.net

/** Additive receiver identity: old PC handshakes retain their original modes. */
object ReceiverCapabilities {
    const val DESKTOP = "desktop"
    const val ANDROID = "android"

    fun platform(value: String?): String = if (value.equals(ANDROID, ignoreCase = true)) ANDROID else DESKTOP

    fun modes(platform: String, supportsCemu: Boolean, supportsSwitch: Boolean, supportsRetroArch: Boolean = false,
              supportsGamepad: Boolean = false): List<String> = buildList {
        if (platform != ANDROID) add("pointer")
        add("dolphin")
        if (platform != ANDROID && supportsCemu) add("cemu")
        if (supportsSwitch) add("switch")
        // RetroArch: lo anuncian en ok.modes tanto el receptor del PC como el servidor Android.
        if (supportsRetroArch) add("retroarch")
        // Mando universal: solo el receptor de PC, que es quien puede crear un
        // mando de Xbox virtual (ViGEm en Windows, uinput en Linux).
        if (platform != ANDROID && supportsGamepad) add("gamepad")
    }

    /** A saved PC preference must not open a desktop-only layout on Android. */
    fun select(requested: String?, current: String, platform: String, supportsCemu: Boolean, supportsSwitch: Boolean,
               supportsRetroArch: Boolean = false, supportsGamepad: Boolean = false): String {
        val available = modes(platform, supportsCemu, supportsSwitch, supportsRetroArch, supportsGamepad)
        return requested?.takeIf { it in available } ?: current.takeIf { it in available } ?: available.first()
    }
}
