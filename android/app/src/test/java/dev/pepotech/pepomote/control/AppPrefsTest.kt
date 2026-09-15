package dev.pepotech.pepomote.control

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class AppPrefsTest {
    @Test fun savedJoyConChoicesMigrateToProWithoutChangingWiiUPreferences() {
        val context: Context = RuntimeEnvironment.getApplication()
        val prefs = context.getSharedPreferences("app", Context.MODE_PRIVATE)
        for (old in listOf("joycons", "joycon_side", "joycon_r")) {
            prefs.edit().putString("switchPad", old).putString("cemuPad", "wiimote").commit()
            assertEquals("pro", AppPrefs.pad(context, "switch"))
            assertEquals("pro", prefs.getString("switchPad", null))
            assertEquals("wiimote", AppPrefs.pad(context, "cemu"))
        }
        AppPrefs.setPad(context, "switch", "joycons")
        assertEquals("pro", prefs.getString("switchPad", null))
        AppPrefs.setPad(context, "cemu", "gamepad")
        assertEquals("gamepad", AppPrefs.pad(context, "cemu"))
    }
}
