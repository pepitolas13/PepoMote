package dev.pepotech.pepomote.service

import android.graphics.Bitmap
import android.graphics.Canvas
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import dev.pepotech.pepomote.control.UpdateAsset
import dev.pepotech.pepomote.control.UpdateCheck
import dev.pepotech.pepomote.control.UpdateManifest
import dev.pepotech.pepomote.ui.components.UpdateDialog
import dev.pepotech.pepomote.ui.theme.PepoMoteTheme
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowDialog
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "es-rES-w393dp-h851dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class UpdateUiTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val release = UpdateManifest(UpdateCheck.Version(1,11,0), "", mapOf("es" to listOf("Mejor conexión"), "en" to listOf("Better connection")), UpdateAsset("PepoMote.apk", "", 100, "a".repeat(64)), "")
    @Test fun offerExplainsChangesAndRequiresExplicitDownloadClick() {
        var clicked = false
        compose.setContent { PepoMoteTheme { UpdateDialog(UpdateUiState(release=release), "es", { clicked=true }, {}, {}, {}) } }
        compose.onNodeWithText("• Mejor conexión").assertIsDisplayed()
        capture("update-offer.png")
        assertFalse(clicked)
        compose.onNodeWithText("Descargar e instalar").performClick()
        assertTrue(clicked)
    }
    @Test fun installedVersionKeepsItsNotesWithoutInstallActions() {
        compose.setContent { PepoMoteTheme { UpdateDialog(UpdateUiState(release=release, check=UpdateCheckStatus.Current, installable=false), "es", {}, {}, {}, {}) } }
        compose.onNodeWithText("• Mejor conexión").assertIsDisplayed()
        compose.onNodeWithText("Descargar e instalar").assertDoesNotExist()
        compose.onNodeWithText("Instalar").assertDoesNotExist()
        compose.onNodeWithText("PepoMote 1.11.0 instalada").assertIsDisplayed()
    }
    @Test fun progressCanBeCancelledAndDoesNotOfferDuplicateDownload() {
        var cancelled = false
        compose.setContent { PepoMoteTheme { UpdateDialog(UpdateUiState(release=release, phase=UpdatePhase.Downloading, bytes=50), "es", {}, {}, { cancelled=true }, {}) } }
        compose.onNodeWithText("Descargando… 50 %").assertIsDisplayed()
        capture("update-progress.png")
        compose.onNodeWithText("Descargar e instalar").assertDoesNotExist()
        compose.onNodeWithText("Cancelar descarga").performClick()
        assertTrue(cancelled)
    }
    @Test fun deniedPermissionRequiresClickAndInstallerReturnNeverClaimsSuccess() {
        val denied = UpdateUiState(phase=UpdatePhase.PermissionRequired).permissionResult(false)
        assertFalse(denied.autoInstall)
        assertEquals(UpdatePhase.PermissionRequired, denied.phase)
        assertTrue(denied.permissionResult(true).autoInstall)
        val returned = UpdateUiState(release=release).installerReturned()
        assertFalse(returned.autoInstall)
        compose.setContent { PepoMoteTheme { UpdateDialog(returned, "es", {}, {}, {}, {}) } }
        compose.onNodeWithText("Android ha abierto el instalador. Si cancelaste o la instalación no terminó, puedes intentarlo otra vez.").assertIsDisplayed()
    }
    private fun capture(name: String) {
        compose.runOnIdle {
            val view = requireNotNull(ShadowDialog.getLatestDialog()?.window?.decorView)
            val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
            view.draw(Canvas(bitmap))
            val directory = File("build/update-previews").apply { mkdirs() }
            File(directory, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
}
