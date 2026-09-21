package dev.pepotech.pepomote.ui.screens

import android.graphics.Bitmap
import android.graphics.Canvas
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.DpRect
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.theme.PepoMoteTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * Mando vertical en pantalla: en un móvil pequeño (360×640 dp) la B cabe
 * entera; en un móvil normal las medidas son las de siempre.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "es-rES-w360dp-h640dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ControllerScreenUiTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private val link = UiLink.Connected("PC", LinkState.MODE_POINTER, null, 100f, supportsCemu = true, supportsSwitch = true)

    // Sin sensores saldría la tarjeta del giroscopio encima del mando
    @Before fun quietGyroCard() { AppPrefs.setGyroWarnShown(compose.activity) }

    private fun show() {
        compose.setContent { PepoMoteTheme { ControllerScreen(link, showChips = true) {} } }
    }

    private fun trigger(): DpRect = compose.onNodeWithTag("wii_trigger").getBoundsInRoot()

    private fun assertTriggerOnScreen(): DpRect {
        val root = compose.onRoot().getBoundsInRoot()
        val b = trigger()
        assertTrue("B termina en ${b.bottom} con la pantalla en ${root.bottom}", b.bottom <= root.bottom - 12.dp + 0.5.dp)
        assertTrue("B empieza en ${b.top}", b.top >= root.top)
        return b
    }

    @Test fun movilPequenoEncogeYLaBQuedaEnPantalla() {
        show()
        val b = assertTriggerOnScreen()
        assertTrue("B mide ${b.height}", b.height < 88.dp)
        assertTrue("B mide ${b.height}", b.height >= 52.8.dp)
        val media = compose.onNodeWithText("Multimedia ▼").getBoundsInRoot()
        assertTrue("Multimedia (${media.bottom}) pisa la B (${b.top})", media.bottom <= b.top + 0.5.dp)
        compose.onNodeWithContentDescription("A").assertIsDisplayed()
        compose.onNodeWithContentDescription("1").assertIsDisplayed()
        compose.onNodeWithContentDescription("2").assertIsDisplayed()
        capture("wii-remote-360x640-ui-test.png")
    }

    @Test fun abrirMultimediaNoEmpujaLaB() {
        show()
        compose.onNodeWithText("Multimedia ▼").performClick()
        compose.onNodeWithContentDescription("⏯").assertIsDisplayed()
        val b = assertTriggerOnScreen()
        val play = compose.onNodeWithContentDescription("⏯").getBoundsInRoot()
        assertTrue("la fila multimedia (${play.bottom}) pisa la B (${b.top})", play.bottom <= b.top + 0.5.dp)
        capture("wii-remote-360x640-media-ui-test.png")
    }

    @Test
    @Config(qualifiers = "es-rES-w393dp-h851dp-xxhdpi")
    fun movilNormalNoCambia() {
        show()
        val b = assertTriggerOnScreen()
        assertEquals(88f, b.height.value, 0.01f)
        assertEquals(148f, compose.onNodeWithContentDescription("A").getBoundsInRoot().height.value, 0.01f)
        capture("wii-remote-393x851-ui-test.png")
    }

    private fun capture(name: String) {
        val root = listOf(File("../.."), File("..")).map { it.canonicalFile }
            .first { File(it, "android/app/src").isDirectory && File(it, "protocol/vectors").isDirectory }
        val directory = File(root, "dist/android-server-preview/screenshots").apply { mkdirs() }
        compose.runOnIdle {
            val view = compose.activity.window.decorView
            val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
            view.draw(Canvas(bitmap))
            File(directory, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
}
