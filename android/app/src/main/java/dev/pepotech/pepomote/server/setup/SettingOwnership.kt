package dev.pepotech.pepomote.server.setup

/** Only known managed Eden settings receive its native quote/default equivalence rules. */
internal object SettingOwnership {
    private val edenPlayerKey = Regex("player_[0-3]_(?:type|connected|button_(?:a|b|x|y|lstick|rstick|l|r|zl|zr|plus|minus|dleft|dup|dright|ddown|slleft|srleft|home|screenshot|slright|srright)|lstick|rstick|motionleft|motionright)")
    private val edenGlobals = setOf("enable_udp_controller", "udp_input_servers", "motion_enabled")

    fun matches(path: String, ini: IniDocument, group: List<ManagedSetting>): Boolean {
        if (path != "config.ini" || group.any { it.key.section != "Controls" }) return group.all { ini.value(it.key) == it.appliedValue }
        val base = group.singleOrNull { !it.key.name.endsWith("\\default") } ?: return false
        val key = base.key.name
        if (key !in edenGlobals && !edenPlayerKey.matches(key)) return group.all { ini.value(it.key) == it.appliedValue }
        val current = ini.value(base.key) ?: return false
        val expected = normalize(key, base.appliedValue)
        if (normalize(key, current) != expected) return false
        val marker = group.singleOrNull { it.key.name == "$key\\default" } ?: return false
        return when (ini.value(marker.key)?.lowercase()) {
            "false" -> true
            // Eden itself rewrites these to true when the selected value equals its built-in default.
            "true" -> nativeDefault(key) == expected
            else -> false
        }
    }

    private fun normalize(key: String, raw: String): String {
        val value = raw.replace("\"", "") // Exactly how Android Config::ReadStringSetting reads these strings.
        return when {
            key.endsWith("_type") -> value.toIntOrNull()?.toString() ?: value
            key.endsWith("_connected") || key == "enable_udp_controller" || key == "motion_enabled" -> value.lowercase()
            else -> value
        }
    }

    private fun nativeDefault(key: String): String = when {
        key.endsWith("_type") -> "0"
        key.endsWith("_connected") -> if (key == "player_0_connected") "true" else "false"
        key == "motion_enabled" -> "true"
        key == "enable_udp_controller" -> "false"
        key == "udp_input_servers" -> "127.0.0.1:26760"
        else -> "" // Android buttons, analogs and motions have empty string defaults.
    }
}
