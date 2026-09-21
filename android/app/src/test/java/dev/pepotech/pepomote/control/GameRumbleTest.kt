package dev.pepotech.pepomote.control

import android.content.Context
import android.os.Looper
import android.os.VibrationAttributes
import android.os.Vibrator
import android.view.HapticFeedbackConstants
import android.view.View
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowSystemClock
import org.robolectric.shadows.ShadowVibrator
import java.time.Duration

/**
 * El motor sobre el vibrador de Robolectric: ninguna orden de juego es una
 * onda sin fin, parar cancela tras el hueco, Apagada en Ajustes para al
 * instante y «Probar» ni los hápticos molestan al juego. El estado de
 * [ShadowVibrator] es estático y `attach` cachea el control de amplitud, así
 * que se fija ANTES de enganchar y se limpia entre pruebas.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class GameRumbleTest {
    private lateinit var context: Context
    private lateinit var shadow: ShadowVibrator

    @Before
    fun setUp() {
        context = RuntimeEnvironment.getApplication()
        // El singleton sobrevive entre pruebas: se deja apagado y sin reintentos pendientes
        GameRumble.clock = null
        GameRumble.reset()
        repeat(RumbleMotor.IDLE_CANCELS + 1) { GameRumble.tick() }
        assertFalse(GameRumble.busy)
        ShadowVibrator.reset()
        shadow = shadowOf(context.getSystemService(Vibrator::class.java))
        AppPrefs.setGameRumble(context, "high")
    }

    private fun attach(amplitude: Boolean) {
        shadow.setHasAmplitudeControl(amplitude)
        GameRumble.attach(context)
    }

    private fun advance(ms: Long) {
        ShadowSystemClock.advanceBy(Duration.ofMillis(ms))
        shadowOf(Looper.getMainLooper()).idle()
    }

    @Test
    fun laOndaConAmplitudEsFinitaYDeCuatroSegmentos() {
        attach(amplitude = true)
        GameRumble.set(1f)
        assertTrue(shadow.isVibrating)
        assertEquals(listOf(750L, 750L, 750L, 750L), shadow.pattern.toList())
        assertEquals("sin bucle", -1, shadow.repeat)
        assertTrue("$shadow", GameRumble.isOn)
        val effect = GameRumble.lastEffect.toString()
        assertTrue(effect, effect.contains("repeat=-1") || effect.contains("repeatIndex=-1"))
        val attrs = shadow.vibrationAttributesFromLastVibration as VibrationAttributes
        assertEquals(VibrationAttributes.USAGE_PHYSICAL_EMULATION, attrs.usage)
    }

    @Test
    fun sinControlDeAmplitudElPwmTambienEsFinito() {
        attach(amplitude = false)
        AppPrefs.setGameRumble(context, "normal")
        GameRumble.set(1f) // 0,65 efectivo → 39 ms encendido, 21 apagado
        val pattern = shadow.pattern.toList()
        assertEquals(100, pattern.size)
        assertEquals(39L, pattern[0])
        assertEquals(21L, pattern[1])
        assertEquals(3000L, pattern.sum())
        assertEquals(-1, shadow.repeat)
    }

    @Test
    fun unaOrdenQueNadieRenuevaCaducaSola() {
        attach(amplitude = true)
        GameRumble.set(1f)
        assertTrue(shadow.isVibrating)
        // Sin tics (el hilo principal atascado, el reloj muerto): el propio
        // vibrador acaba la onda al cabo de la duración, sin ningún cancel()
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(RumbleMotor.LEASE_MS + 10))
        assertFalse(shadow.isVibrating)
        assertFalse(shadow.isCancelled)
    }

    @Test
    fun pararCancelaTrasElHuecoYElRelojRenuevaMientrasHayNivel() {
        attach(amplitude = true)
        var wakes = 0
        GameRumble.clock = { wakes++ }
        GameRumble.set(1f)
        assertEquals("cada orden avisa al reloj", 1, wakes)
        GameRumble.stop()
        assertFalse("un Stop no cancela en el acto", shadow.isCancelled)
        advance(RumbleMotor.GAP_MS)
        GameRumble.tick()
        assertTrue(shadow.isCancelled)
        assertFalse(GameRumble.isOn)
        assertTrue("quedan reintentos", GameRumble.busy)

        ShadowVibrator.reset()
        GameRumble.set(0.5f)
        assertTrue(shadow.isVibrating)
        advance(RumbleMotor.RENEW_AFTER_MS)
        ShadowVibrator.reset()
        GameRumble.tick()
        assertTrue("renovada", shadow.isVibrating)
        assertEquals(listOf(750L, 750L, 750L, 750L), shadow.pattern.toList())
    }

    @Test
    fun apagadaEnAjustesParaAlInstanteYNormalVuelve() {
        attach(amplitude = true)
        GameRumble.set(1f)
        assertTrue(shadow.isVibrating)
        AppPrefs.setGameRumble(context, "off")
        assertTrue("Apagada cancela ya, sin esperar a otra orden", shadow.isCancelled)
        assertFalse(GameRumble.isOn)
        ShadowVibrator.reset()
        AppPrefs.setGameRumble(context, "normal")
        assertTrue("el juego sigue pidiendo: vuelve con la escala nueva", shadow.isVibrating)
        assertTrue(GameRumble.isOn)
    }

    @Test
    fun probarNoTocaElMotorMientrasVibraElJuego() {
        attach(amplitude = true)
        GameRumble.set(1f)
        GameRumble.pulse(1f)
        assertEquals("la onda del juego sigue", listOf(750L, 750L, 750L, 750L), shadow.pattern.toList())
        GameRumble.reset()
        repeat(RumbleMotor.IDLE_CANCELS + 1) { GameRumble.tick() }
        ShadowVibrator.reset()
        GameRumble.pulse(1f)
        assertEquals(300L, shadow.milliseconds)
        val attrs = shadow.vibrationAttributesFromLastVibration as VibrationAttributes
        assertEquals(VibrationAttributes.USAGE_PHYSICAL_EMULATION, attrs.usage)
    }

    @Test
    fun losHapticosDeLosBotonesSeSaltanConElJuegoVibrando() {
        attach(amplitude = true)
        var taps = 0
        val view = object : View(context) {
            override fun performHapticFeedback(feedbackConstant: Int): Boolean {
                taps++
                return true
            }
        }
        Haptics.tap(view, HapticFeedbackConstants.KEYBOARD_TAP)
        assertEquals(1, taps)
        GameRumble.set(1f)
        Haptics.tap(view, HapticFeedbackConstants.KEYBOARD_TAP)
        assertEquals("con el juego vibrando, ninguno", 1, taps)
        Haptics.tap(null, HapticFeedbackConstants.KEYBOARD_TAP)
    }
}
