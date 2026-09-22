package dev.pepotech.pepomote.ui.screens

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import dev.pepotech.pepomote.ui.theme.PepoMoteTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * La primera pantalla de la app: el botón «¡Vamos!» tiene que verse y
 * funcionar en un móvil pequeño (360×640 dp) aunque la letra del sistema
 * esté al doble, que es cuando los tres pasos ya no caben y el botón se
 * salía por abajo sin forma de avanzar.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "es-rES-w360dp-h640dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class OnboardingScreenUiTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private fun show(fontScale: Float, onDone: () -> Unit) {
        compose.setContent {
            PepoMoteTheme {
                val d = LocalDensity.current
                CompositionLocalProvider(LocalDensity provides Density(d.density, fontScale)) {
                    OnboardingScreen(onDone)
                }
            }
        }
    }

    private fun assertLetsGoOnScreenAndWorking(done: () -> Boolean) {
        val root = compose.onRoot().getBoundsInRoot()
        val button = compose.onNodeWithText("¡Vamos!")
        val b = button.getBoundsInRoot()
        assertTrue("el botón acaba en ${b.bottom} con la pantalla en ${root.bottom}", b.bottom <= root.bottom)
        assertTrue("el botón empieza en ${b.top}", b.top >= root.top)
        // sin sitio, la columna de antes lo aplastaba a 0 dp de alto
        assertTrue("el botón mide ${b.height}", b.height >= 60.dp)
        button.assertIsDisplayed().performClick()
        assertTrue("¡Vamos! no avanzó", done())
    }

    @Test fun conLaLetraAlDobleElBotonSigueEnPantallaYAvanza() {
        var done = false
        show(fontScale = 2f) { done = true }
        assertLetsGoOnScreenAndWorking { done }
    }

    @Test fun conLaLetraNormalTambien() {
        var done = false
        show(fontScale = 1f) { done = true }
        assertLetsGoOnScreenAndWorking { done }
    }
}
