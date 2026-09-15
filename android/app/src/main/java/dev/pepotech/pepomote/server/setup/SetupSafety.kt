package dev.pepotech.pepomote.server.setup

internal object SetupSafety {
    const val MAX_FILE_BYTES = 1_048_576
    const val MAX_BACKUP_BYTES = 16 * MAX_FILE_BYTES

    fun packages(target: EmulatorTarget): List<String> = when (target) {
        EmulatorTarget.Dolphin -> listOf("org.dolphinemu.dolphinemu")
        EmulatorTarget.Eden -> listOf("dev.eden.eden_emulator", "dev.legacy.eden_emulator", "com.miHoYo.Yuanshen").let { base -> base + base.map { "$it.nightly" } }
    }

    fun configDirectory(target: EmulatorTarget) = if (target == EmulatorTarget.Dolphin) "Config" else "config"

    fun validateRoot(target: EmulatorTarget, authority: String, packageName: String, documentId: String): String {
        if (packageName !in packages(target) || authority != "$packageName.user") {
            throw SetupException("Choose the ${target.name} folder from the emulator's own document provider, not a file manager or Android/data.", reason = SetupErrorReason.Selection)
        }
        val config = configDirectory(target)
        return when (documentId) {
            "root" -> if (target == EmulatorTarget.Dolphin) config else throw SetupException("Select Eden's root/ document or config folder.", reason = SetupErrorReason.Selection)
            "root/" -> config
            "root/$config" -> ""
            else -> throw SetupException("Select the ${target.name} root or its $config folder. Other folders cannot be used for setup.", reason = SetupErrorReason.Selection)
        }
    }

    fun validatePath(target: EmulatorTarget, path: String) {
        val valid = when (target) {
            EmulatorTarget.Eden -> path == "config.ini"
            EmulatorTarget.Dolphin -> path in setOf("DSUClient.ini", "WiimoteNew.ini") ||
                Regex("Profiles/Wiimote/PepoMote Android J[1-4](?: Nunchuk)?(?: \\((?:[2-9]|[1-9][0-9]{1,2})\\))?\\.ini").matches(path)
        }
        if (!valid || path.length > 160 || path.split('/').any { it == "." || it == ".." || it.isEmpty() } || '\\' in path) {
            throw SetupException("Refusing an unexpected emulator configuration path.")
        }
    }
}
