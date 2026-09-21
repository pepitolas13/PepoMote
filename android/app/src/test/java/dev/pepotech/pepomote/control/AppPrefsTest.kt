package dev.pepotech.pepomote.control

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class AppPrefsTest {
    @Test fun elClicAntesDeEscribirVieneEncendido() {
        val context: Context = RuntimeEnvironment.getApplication()
        assertTrue(AppPrefs.keyboardClickFirst(context))
        AppPrefs.setKeyboardClickFirst(context, false)
        assertFalse(AppPrefs.keyboardClickFirst(context))
        AppPrefs.setKeyboardClickFirst(context, true)
        assertTrue(AppPrefs.keyboardClickFirst(context))
    }

    /**
     * Mando universal: el giro no está elegido hasta que se contesta la
     * pregunta de la primera vez, y mientras tanto va apagado. Las dos
     * respuestas se recuerdan (y ninguna deja la pregunta pendiente).
     */
    @Test fun elGiroDelMandoUniversalEmpiezaSinElegirYApagado() {
        val context: Context = RuntimeEnvironment.getApplication()
        assertEquals(AppPrefs.PAD_AIM_ASK, AppPrefs.padAim(context))
        assertFalse(AppPrefs.padAimEnabled(context))

        AppPrefs.setPadAim(context, true)
        assertEquals(AppPrefs.PAD_AIM_ON, AppPrefs.padAim(context))
        assertTrue(AppPrefs.padAimEnabled(context))

        AppPrefs.setPadAim(context, false)
        assertEquals(AppPrefs.PAD_AIM_OFF, AppPrefs.padAim(context))
        assertFalse(AppPrefs.padAimEnabled(context))
    }

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

    /**
     * Los dos ajustes de pulsación vienen como toca (deslizar no, mantener
     * sí) y cambiarlos llega al momento a [PressMode], que es lo que leen los
     * botones (como `setSoundsEnabled` con `UiSounds.enabled`).
     */
    @Test fun losModosDePulsacionVienenComoTocaYLosSettersAvisanAPressMode() {
        val context: Context = RuntimeEnvironment.getApplication()
        assertEquals(false, AppPrefs.slidePress(context))
        assertEquals(true, AppPrefs.stickyPress(context))

        AppPrefs.setSlidePress(context, true)
        assertEquals(true, AppPrefs.slidePress(context))
        assertEquals(true, PressMode.slide)

        AppPrefs.setStickyPress(context, false)
        assertEquals(false, AppPrefs.stickyPress(context))
        assertEquals(false, PressMode.sticky)

        // como estaban: PressMode es un objeto y vive más que esta prueba
        AppPrefs.setSlidePress(context, false)
        AppPrefs.setStickyPress(context, true)
        assertEquals(false, PressMode.slide)
        assertEquals(true, PressMode.sticky)
    }
}
