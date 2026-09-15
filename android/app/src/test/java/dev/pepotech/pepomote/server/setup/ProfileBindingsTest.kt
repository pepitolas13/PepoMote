package dev.pepotech.pepomote.server.setup

import org.junit.Assert.*
import org.junit.Test

class ProfileBindingsTest {
    @Test fun dolphinUsesOfficialAndroidImuirAndFullMotionBindings() {
        val profile = ProfileBindings.dolphin(2, true)
        assertEquals("DSUClient/2/PepoMote", profile["Device"])
        assertEquals("True", profile["IMUIR/Enabled"])
        assertEquals("`Touch Button`", profile["IMUIR/Recenter"])
        assertFalse(profile.keys.any { it.startsWith("IMUPointer/") })
        assertEquals("Cross", profile["Buttons/A"])
        assertEquals("Circle", profile["Buttons/B"])
        assertEquals("Square", profile["Buttons/1"])
        assertEquals("Triangle", profile["Buttons/2"])
        assertEquals("`Pad N`", profile["D-Pad/Up"])
        assertEquals("`Accel Forward`", profile["IMUAccelerometer/Forward"])
        assertEquals("`Gyro Yaw Right`", profile["IMUGyroscope/Yaw Right"])
        assertEquals("True", profile["Extension/Attach MotionPlus"])
        assertEquals("`DSUClient/2/PepoMote:L1`", profile["Nunchuk/Buttons/C"])
        assertEquals("`DSUClient/2/PepoMote:R1`", profile["Nunchuk/Buttons/Z"])
        assertEquals("`DSUClient/2/PepoMote:Left X-`", profile["Nunchuk/Stick/Left"])
        assertEquals("`DSUClient/2/PepoMote:Accel Backward`", profile["Nunchuk/IMUAccelerometer/Backward"])
        assertEquals("None", ProfileBindings.dolphin(0, false)["Extension"])
    }

    @Test fun edenUsesAndroidPlayerNamesAndDsuBitmaskParamPackages() {
        val controls = ProfileBindings.eden(listOf(SetupPlayer(1)), "10.0.0.5:26760")
        assertEquals("0", controls["player_1_type"])
        assertEquals("true", controls["player_1_connected"])
        assertEquals("button:8192,engine:cemuhookudp,guid:0000000000000000000000007f000001,pad:5,port:26760", controls["player_1_button_a"])
        assertEquals("button:524288,engine:cemuhookudp,guid:0000000000000000000000007f000001,pad:5,port:26760", controls["player_1_button_screenshot"])
        assertEquals("axis_x:2,axis_y:3,deadzone:0.050000,range:1.000000,engine:cemuhookudp,guid:0000000000000000000000007f000001,pad:5,port:26760", controls["player_1_rstick"])
        assertEquals("motion:0,engine:cemuhookudp,guid:0000000000000000000000007f000001,pad:5,port:26760", controls["player_1_motionleft"])
        assertEquals(controls["player_1_motionleft"], controls["player_1_motionright"])
        assertEquals("10.0.0.5:26760,127.0.0.1:26760", controls["udp_input_servers"])
        assertEquals("true", controls["motion_enabled"])
        assertFalse(controls.keys.any { it.startsWith("player_0_") })
        controls.filterKeys { !it.endsWith("\\default") }.forEach { (key, _) -> assertEquals(key, "false", controls["$key\\default"]) }
    }

    @Test fun edenDoesNotExceedEightUdpServersOrGuessAnInvalidServerOffset() {
        val full = (1..8).joinToString(",") { "10.0.0.$it:26760" }
        assertThrows(SetupException::class.java) { ProfileBindings.eden(listOf(SetupPlayer(0)), full) }
        assertThrows(SetupException::class.java) { ProfileBindings.eden(listOf(SetupPlayer(0)), "malformed") }
        for (ambiguous in listOf("10.0.0.2:08", "10.0.0.2:026760", "010.0.0.2:26760", "10.0.0.2:+26760")) {
            assertThrows(ambiguous, SetupException::class.java) { ProfileBindings.eden(listOf(SetupPlayer(0)), ambiguous) }
        }
    }

    @Test fun dolphinServerMergeKeepsForeignEntriesWithoutDuplicatingOwnedEndpoint() {
        assertEquals("Other:10.0.0.1:12345;PepoMote:127.0.0.1:26760;", ProfileBindings.dolphinServers("Other:10.0.0.1:12345;PepoMote:127.0.0.1:26760;"))
        assertEquals("Other:10.0.0.1:12345;PepoMote:127.0.0.1:26760;", ProfileBindings.dolphinServers("Other:10.0.0.1:12345;"))
        assertThrows(SetupException::class.java) { ProfileBindings.dolphinServers("PepoMote:192.168.1.10:26760;") }
        assertThrows(SetupException::class.java) { ProfileBindings.dolphinServers("MyOtherApp:127.0.0.1:26760;") }
    }
}
