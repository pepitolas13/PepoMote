package dev.pepotech.pepomote.server.setup

internal object ProfileBindings {
    private const val ENDPOINT = "127.0.0.1:26760"
    private const val GUID = "0000000000000000000000007f000001"

    fun dolphin(slot: Int, nunchuk: Boolean): Map<String, String> {
        val values = linkedMapOf("Device" to "DSUClient/$slot/PepoMote")
        values += linkedMapOf(
            "Buttons/A" to "Cross", "Buttons/B" to "Circle", "Buttons/1" to "Square", "Buttons/2" to "Triangle",
            "Buttons/-" to "Share", "Buttons/+" to "Options", "Buttons/Home" to "PS",
            "D-Pad/Up" to "`Pad N`", "D-Pad/Down" to "`Pad S`", "D-Pad/Left" to "`Pad W`", "D-Pad/Right" to "`Pad E`"
        )
        for (direction in listOf("Up", "Down", "Left", "Right", "Forward", "Backward")) {
            values["IMUAccelerometer/$direction"] = "`Accel $direction`"
        }
        for (direction in listOf("Pitch Up", "Pitch Down", "Roll Left", "Roll Right", "Yaw Left", "Yaw Right")) {
            values["IMUGyroscope/$direction"] = "`Gyro $direction`"
        }
        values += linkedMapOf(
            "IMUGyroscope/Dead Zone" to "3.", "IMUIR/Enabled" to "True", "IMUIR/Recenter" to "`Touch Button`",
            "IMUIR/Total Yaw" to "25.", "Extension" to if (nunchuk) "Nunchuk" else "None",
            "Extension/Attach MotionPlus" to "True"
        )
        if (nunchuk) {
            fun input(name: String) = "`DSUClient/$slot/PepoMote:$name`"
            values["Nunchuk/Buttons/C"] = input("L1")
            values["Nunchuk/Buttons/Z"] = input("R1")
            for ((direction, axis) in listOf("Up" to "Left Y+", "Down" to "Left Y-", "Left" to "Left X-", "Right" to "Left X+")) {
                values["Nunchuk/Stick/$direction"] = input(axis)
            }
            for (direction in listOf("Up", "Down", "Left", "Right", "Forward", "Backward")) {
                values["Nunchuk/IMUAccelerometer/$direction"] = input("Accel $direction")
            }
        }
        values["Options/Battery"] = "`Battery`"
        return values
    }

    fun eden(players: List<SetupPlayer>, servers: String?): Map<String, String> {
        val values = linkedMapOf<String, String>()
        fun setting(key: String, value: String) {
            values[key] = value
            values["$key\\default"] = "false"
        }
        val serverList = servers.orEmpty().trim().removeSurrounding("\"").split(',').map(String::trim).filter(String::isNotEmpty).distinct().toMutableList()
        for (server in serverList) {
            val parts = server.split(':')
            val address = parts.first().split('.')
            if (parts.size != 2 || address.size != 4 || address.any { it.toIntOrNull() !in 0..255 || it.toIntOrNull()?.toString() != it } ||
                parts[1].toIntOrNull() !in 1..65535 || parts[1].toIntOrNull()?.toString() != parts[1]) {
                throw SetupException("Eden has an unsupported UDP server entry. Use IPv4 addresses and decimal ports in udp_input_servers before preparing PepoMote.")
            }
        }
        if (ENDPOINT !in serverList) serverList += ENDPOINT
        if (serverList.size > 8) throw SetupException("Eden supports eight UDP servers. Remove an unused server before adding PepoMote.")
        val padOffset = serverList.indexOf(ENDPOINT) * 4
        setting("enable_udp_controller", "true")
        setting("udp_input_servers", serverList.joinToString(","))
        setting("motion_enabled", "true")
        val buttons = linkedMapOf(
            "a" to 8192, "b" to 16384, "x" to 4096, "y" to 32768, "lstick" to 2, "rstick" to 4,
            "l" to 1024, "r" to 2048, "zl" to 256, "zr" to 512, "plus" to 8, "minus" to 1,
            "dleft" to 128, "dup" to 16, "dright" to 32, "ddown" to 64, "slleft" to 0, "srleft" to 0,
            "home" to 262144, "screenshot" to 524288, "slright" to 0, "srright" to 0
        )
        for (player in players.sortedBy { it.slot }) {
            val prefix = "player_${player.slot}_"
            fun binding(fields: String) = "$fields,engine:cemuhookudp,guid:$GUID,pad:${padOffset + player.slot},port:26760"
            setting(prefix + "type", "0")
            setting(prefix + "connected", "true")
            for ((name, id) in buttons) setting(prefix + "button_$name", if (id == 0) "[empty]" else binding("button:$id"))
            setting(prefix + "lstick", binding("axis_x:0,axis_y:1,deadzone:0.050000,range:1.000000"))
            setting(prefix + "rstick", binding("axis_x:2,axis_y:3,deadzone:0.050000,range:1.000000"))
            setting(prefix + "motionleft", binding("motion:0"))
            setting(prefix + "motionright", binding("motion:0"))
        }
        return values
    }

    fun dolphinServers(servers: String?): String {
        val entries = servers.orEmpty().split(';').map(String::trim).filter(String::isNotEmpty)
        val expected = "PepoMote:$ENDPOINT"
        for (entry in entries) {
            val parts = entry.split(':')
            if (parts.size != 3 || parts.any(String::isEmpty)) throw SetupException("Dolphin has an invalid DSU server entry. Correct its DSUClient.ini before preparing PepoMote.")
            if ((parts[0] == "PepoMote" || parts.drop(1).joinToString(":") == ENDPOINT) && entry != expected) {
                throw SetupException("Dolphin already has a different DSU server named PepoMote or using $ENDPOINT. Rename or remove that conflicting entry first.")
            }
        }
        return (entries.filter { it != expected } + expected).joinToString(";", postfix = ";")
    }
}
