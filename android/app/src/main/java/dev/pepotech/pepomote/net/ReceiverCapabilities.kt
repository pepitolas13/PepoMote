package dev.pepotech.pepomote.net

/** Additive receiver identity: old PC handshakes retain their original modes. */
object ReceiverCapabilities {
    const val DESKTOP = "desktop"
    const val ANDROID = "android"

    fun platform(value: String?): String = if (value.equals(ANDROID, ignoreCase = true)) ANDROID else DESKTOP

    fun modes(platform: String, supportsCemu: Boolean, supportsSwitch: Boolean): List<String> = buildList {
        if (platform != ANDROID) add("pointer")
        add("dolphin")
        if (platform != ANDROID && supportsCemu) add("cemu")
        if (supportsSwitch) add("switch")
    }

    /** A saved PC preference must not open a desktop-only layout on Android. */
    fun select(requested: String?, current: String, platform: String, supportsCemu: Boolean, supportsSwitch: Boolean): String {
        val available = modes(platform, supportsCemu, supportsSwitch)
        return requested?.takeIf { it in available } ?: current.takeIf { it in available } ?: available.first()
    }
}
