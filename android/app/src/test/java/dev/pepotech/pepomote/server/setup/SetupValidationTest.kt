package dev.pepotech.pepomote.server.setup

import org.junit.Assert.*
import org.junit.Test

class SetupValidationTest {
    @Test fun onlyExactConfigurationAndGeneratedProfilePathsAreAccepted() {
        for (path in listOf("DSUClient.ini", "WiimoteNew.ini", "Profiles/Wiimote/PepoMote Android J4 Nunchuk (12).ini")) SetupSafety.validatePath(EmulatorTarget.Dolphin, path)
        SetupSafety.validatePath(EmulatorTarget.Eden, "config.ini")
        assertThrows(SetupException::class.java) { SetupSafety.validatePath(EmulatorTarget.Dolphin, "Profiles/Wiimote/My profile.ini") }
    }

    @Test fun exactAllowedPackageAndAuthorityAreRequired() {
        assertEquals("Config", SetupSafety.validateRoot(EmulatorTarget.Dolphin, "org.dolphinemu.dolphinemu.user", "org.dolphinemu.dolphinemu", "root/"))
        assertEquals("", SetupSafety.validateRoot(EmulatorTarget.Eden, "dev.legacy.eden_emulator.user", "dev.legacy.eden_emulator", "root/config"))
        for (authority in listOf("com.fake.Eden.user", "com.android.externalstorage.documents", "dev.eden.eden_emulator.user.evil")) {
            assertThrows(SetupException::class.java) { SetupSafety.validateRoot(EmulatorTarget.Eden, authority, "dev.eden.eden_emulator", "root/") }
        }
        assertThrows(SetupException::class.java) { SetupSafety.validateRoot(EmulatorTarget.Eden, "dev.eden.eden_emulator.user", "com.fake.Eden", "root/") }
    }

    @Test fun traversalAndroidDataAndUnrelatedSubtreesAreRejected() {
        for (id in listOf("root", "root/../config", "root/config/../../nand", "root/Android/data", "root/nand", "root/config/profiles", "root2/config", "root/config/", "root\\config")) {
            assertThrows(id, SetupException::class.java) { SetupSafety.validateRoot(EmulatorTarget.Eden, "dev.eden.eden_emulator.user", "dev.eden.eden_emulator", id) }
        }
        for (path in listOf("../config.ini", "/config.ini", "config/../../nand", "Android/data/file.ini", "Profiles\\outside.ini", "Profiles/Wiimote/../../x.ini")) {
            assertThrows(path, SetupException::class.java) { SetupSafety.validatePath(EmulatorTarget.Dolphin, path) }
        }
    }
}
