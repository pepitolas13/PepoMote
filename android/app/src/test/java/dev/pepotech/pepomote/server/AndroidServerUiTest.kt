package dev.pepotech.pepomote.server

import android.graphics.Bitmap
import android.graphics.Canvas
import android.content.Context
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.ProviderInfo
import android.net.Uri
import android.provider.DocumentsContract
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.junit4.StateRestorationTester
import dev.pepotech.pepomote.server.core.ReceiverConfig
import dev.pepotech.pepomote.server.core.ReceiverMode
import dev.pepotech.pepomote.server.core.ReceiverSnapshot
import dev.pepotech.pepomote.server.core.ReceiverPeerSnapshot
import dev.pepotech.pepomote.server.core.RetroStatus
import dev.pepotech.pepomote.server.retro.Console
import dev.pepotech.pepomote.server.retro.RetroArchApp
import dev.pepotech.pepomote.server.retro.RetroFolder
import dev.pepotech.pepomote.server.retro.RetroGameInfo
import android.content.ComponentName
import android.content.IntentFilter
import dev.pepotech.pepomote.server.setup.AndroidBackupStore
import dev.pepotech.pepomote.server.setup.EmulatorTarget
import dev.pepotech.pepomote.server.setup.SetupJournal
import dev.pepotech.pepomote.server.setup.PendingTransaction
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.net.ReceiverInfo
import dev.pepotech.pepomote.ui.screens.PairScreen
import dev.pepotech.pepomote.ui.screens.HomeScreen
import dev.pepotech.pepomote.ui.screens.HomeStatus
import dev.pepotech.pepomote.ui.screens.HomeTone
import dev.pepotech.pepomote.ui.screens.ModeChips
import dev.pepotech.pepomote.ui.theme.PepoMoteTheme
import kotlinx.coroutines.flow.flowOf
import org.junit.After
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.shadows.ShadowDialog
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "es-rES-w393dp-h851dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AndroidServerUiTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Before fun clearPendingSetup() {
        RetroArchApp.forgetVerified(compose.activity)
        RetroFolder.forget(compose.activity)
        compose.activity.getSharedPreferences("server_setup_pending", Context.MODE_PRIVATE).edit().clear().commit()
        ServerSetupJobs.refresh(compose.activity)
        ServerSetupJobs.dismissError()
    }

    @After fun reset() { ServerState.mutable.value = ServerUiState() }

    private fun home(android: Boolean = false, onServer: () -> Unit = {}) {
        compose.setContent {
            PepoMoteTheme {
                HomeScreen(HomeStatus(HomeTone.Off, "Sin conexión"), "ES", "English",
                    {}, {}, {}, {}, {}, {}, {}, {}, onServer = onServer, androidReceiver = android)
            }
        }
    }

    @Test fun serverIsAVisiblePrimaryActionOnHome() {
        var opened = false
        home(onServer = { opened = true })
        val server = compose.onNodeWithText("Servidor").assertIsDisplayed()
        val dolphin = compose.onNodeWithText("Dolphin")
        assertTrue("Server belongs in the first row", server.fetchSemanticsNode().boundsInRoot.top <
            dolphin.fetchSemanticsNode().boundsInRoot.top)
        server.performClick()
        assertTrue(opened)
        capture("home-server-ui-test.png")
    }

    @Test fun androidConnectionHomeOnlyOffersSupportedConsoleModes() {
        home(android = true)
        compose.onNodeWithText("Dolphin").assertIsDisplayed()
        compose.onNodeWithText("Eden").assertIsDisplayed()
        compose.onNodeWithText("Wii U").assertDoesNotExist()
        compose.onNodeWithText("Mando", substring = false).assertDoesNotExist()
        compose.onNodeWithText("Nunchuk", substring = false).assertDoesNotExist()
        capture("android-controller-home-ui-test.png")
    }

    @Test fun androidHomeOffersRetroArch() {
        home(android = true)
        compose.onNodeWithText("RetroArch", substring = false).assertIsDisplayed()
        compose.onNodeWithText("pistola", substring = true).assertDoesNotExist()
    }

    @Test fun androidChipsOfferRetroArchWhenTheServerAnnouncesIt() {
        compose.setContent { PepoMoteTheme {
            ModeChips("dolphin", supportsCemu = true, supportsSwitch = true, supportsRetroArch = true, androidReceiver = true)
        } }
        compose.onNodeWithText("RetroArch").assertIsDisplayed()
        compose.onNodeWithText("Eden").assertIsDisplayed()
        compose.onNodeWithText("Puntero").assertDoesNotExist()
        compose.onNodeWithText("Wii U").assertDoesNotExist()
    }

    @Test fun retroArchCardOffersInstallThenTheOneTimeGuideAndTheFolderPicker() {
        ServerState.mutable.value = ServerUiState(
            config = ReceiverConfig("Android de prueba", "test-only-qr-credential", "1234"),
            hosts = listOf("192.168.1.20"),
            receiver = ReceiverSnapshot(true, ReceiverMode.Dolphin, 26761, 26760, emptyList(), 0))
        compose.setContent { PepoMoteTheme { ServerScreen {} } }
        compose.onNode(hasScrollToNodeAction()).performScrollToNode(hasText("RetroArch", substring = false))
        compose.onNodeWithText("RetroArch", substring = false).performClick()
        compose.onNodeWithText("Instalar RetroArch").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Abrir RetroArch").assertDoesNotExist()
        capture("server-retroarch-install-ui-test.png")
    }

    @Test fun installedRetroArchGetsTheOneTimeGuideAndTheFolderPicker() {
        // Con RetroArch instalado: la guía de una sola vez, abrirlo y la carpeta para el mando de cada consola
        val component = ComponentName("com.retroarch.aarch64", "com.retroarch.browser.mainmenu.MainMenuActivity")
        shadowOf(compose.activity.packageManager).addActivityIfNotPresent(component)
        shadowOf(compose.activity.packageManager).addIntentFilterForActivity(component, IntentFilter(Intent.ACTION_MAIN).apply { addCategory(Intent.CATEGORY_LAUNCHER) })
        ServerState.mutable.value = ServerUiState(
            config = ReceiverConfig("Android de prueba", "test-only-qr-credential", "1234"),
            hosts = listOf("192.168.1.20"),
            receiver = ReceiverSnapshot(true, ReceiverMode.RetroArch, 26761, 26760, emptyList(), 0))
        compose.setContent { PepoMoteTheme { ServerScreen {} } }
        compose.onNode(hasScrollToNodeAction()).performScrollToNode(hasText("Abrir RetroArch"))
        compose.onNodeWithText("Activa el mando en red en RetroArch (solo una vez)").assertIsDisplayed()
        compose.onNodeWithText("1. Abre RetroArch", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Instalar RetroArch").assertDoesNotExist()
        capture("server-retroarch-guide-ui-test.png")
        compose.onNodeWithText("Abrir RetroArch").performClick()
        assertEquals("com.retroarch.aarch64", shadowOf(compose.activity).nextStartedActivity.component!!.packageName)
        compose.onNodeWithText("Elegir la carpeta RetroArch").performScrollTo().performClick()
        compose.onNodeWithText("1. Toca «Usar esta carpeta».", substring = true).assertIsDisplayed()
        capture("server-retroarch-folder-ui-test.png")
        compose.onNodeWithText("Ir al permiso").performClick()
        val intent = shadowOf(compose.activity).nextStartedActivityForResult.intent
        assertEquals(Intent.ACTION_OPEN_DOCUMENT_TREE, intent.action)
        @Suppress("DEPRECATION")
        assertEquals(DocumentsContract.buildDocumentUri("com.android.externalstorage.documents", "primary:RetroArch"),
            intent.getParcelableExtra<Uri>(DocumentsContract.EXTRA_INITIAL_URI))
    }

    @Test fun retroArchAnsweringShowsReadyLiveStatusAndTheLoadedGame() {
        val component = ComponentName("com.retroarch.aarch64", "com.retroarch.browser.mainmenu.MainMenuActivity")
        shadowOf(compose.activity.packageManager).addActivityIfNotPresent(component)
        shadowOf(compose.activity.packageManager).addIntentFilterForActivity(component, IntentFilter(Intent.ACTION_MAIN).apply { addCategory(Intent.CATEGORY_LAUNCHER) })
        RetroArchApp.markVerified(compose.activity, "1.22.2")
        ServerState.mutable.value = ServerUiState(
            config = ReceiverConfig("Android de prueba", "test-only-qr-credential", "1234"),
            hosts = listOf("192.168.1.20"),
            receiver = ReceiverSnapshot(true, ReceiverMode.RetroArch, 26761, 26760, emptyList(), 0,
                retro = RetroStatus(reachable = true, version = "1.22.2", pollsPerSec = 60f,
                    game = RetroGameInfo(Console.Md, "Mega Drive", "Genesis Plus GX", "Cave Story", "/roms/cave.zip"))))
        compose.setContent { PepoMoteTheme { ServerScreen {} } }
        compose.onNode(hasScrollToNodeAction()).performScrollToNode(hasText("Jugar con RetroArch"))
        compose.onNodeWithText("Conexión con RetroArch comprobada").assertIsDisplayed()
        compose.onNodeWithText("RetroArch 1.22.2 responde · 60 sondeos por segundo").assertIsDisplayed()
        compose.onNodeWithText("Elegir la carpeta RetroArch").assertExists()
        compose.onNodeWithText("Ver la guía otra vez").assertExists()
        capture("server-retroarch-ready-ui-test.png")
        // Sin RetroArch delante pero ya comprobado: sigue listo, con la guía a mano
        ServerState.mutable.value = ServerState.flow.value.copy(receiver = ServerState.flow.value.receiver!!.copy(retro = RetroStatus()))
        compose.onNodeWithText("Conexión con RetroArch comprobada").assertIsDisplayed()
        compose.onNodeWithText("RetroArch 1.22.2 ya respondió.", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Ver la guía otra vez").performScrollTo().performClick()
        compose.onNodeWithText("1. Abre RetroArch", substring = true).assertIsDisplayed()
    }

    @Test fun retroArchDesinstaladoVuelveAOfrecerInstalarAunqueRespondieraAntes() {
        RetroArchApp.markVerified(compose.activity, "1.22.2")
        ServerState.mutable.value = ServerUiState(
            receiver = ReceiverSnapshot(true, ReceiverMode.RetroArch, 26761, 26760, emptyList(), 0))
        compose.setContent { PepoMoteTheme { ServerScreen {} } }
        compose.onNodeWithText("Instalar RetroArch").assertExists()
        compose.onNodeWithText("Jugar con RetroArch").assertDoesNotExist()
    }

    @Test fun androidModeChipsCannotOfferPointerOrCemuEvenWithStalePcCapabilities() {
        compose.setContent { PepoMoteTheme {
            ModeChips("dolphin", supportsCemu = true, supportsSwitch = true, androidReceiver = true)
        } }
        compose.onNodeWithText("Dolphin").assertIsDisplayed()
        compose.onNodeWithText("Eden").assertIsDisplayed()
        compose.onNodeWithText("Puntero").assertDoesNotExist()
        compose.onNodeWithText("Wii U").assertDoesNotExist()
    }

    @Test fun serverDashboardShowsPairingAndHonestEmulatorSetup() {
        ServerState.mutable.value = ServerUiState(
            config = ReceiverConfig("Android de prueba", "test-only-qr-credential", "1234"),
            hosts = listOf("192.168.1.20"),
            receiver = ReceiverSnapshot(true, ReceiverMode.Dolphin, 26761, 26760, emptyList(), 0))
        compose.setContent { PepoMoteTheme { ServerScreen {} } }
        compose.onNodeWithText("1234").performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription("QR para conectar otro móvil a este servidor Android").assertExists()
        capture("server-pairing-ui-test.png")
        compose.onNodeWithText("Copiar enlace").performScrollTo().performClick()
        compose.onNodeWithText("Enlace copiado").assertIsDisplayed()
        val clipboard = compose.activity.getSystemService(ClipboardManager::class.java)
        assertEquals("192.168.1.20", PairStore.parsePairUrl(clipboard.primaryClip!!.getItemAt(0).text.toString())!!.host)
        assertTrue(clipboard.primaryClip!!.description.extras!!.getBoolean("android.content.extra.IS_SENSITIVE"))
        compose.onNode(hasScrollToNodeAction()).performScrollToNode(hasText("Eden", substring = false))
        compose.onNodeWithText("Eden", substring = false).assertIsDisplayed().performClick()
        compose.onNodeWithText("Movimiento en Eden:", substring = true).performScrollTo().assertIsDisplayed()
        capture("server-eden-ui-test.png")
    }

    @Test fun addressChoiceIsHiddenAndVpnLossNeverSubstitutesTheLocalQr() {
        ServerState.mutable.value = ServerUiState(
            config = ReceiverConfig("Android de prueba", "test-only-qr-credential", "1234"),
            hosts = listOf("192.168.1.20", "192.168.43.1"), vpnHosts = listOf("100.100.10.2"),
            receiver = ReceiverSnapshot(true, ReceiverMode.Dolphin, 26761, 26760, emptyList(), 0))
        compose.setContent { PepoMoteTheme { ServerScreen {} } }
        compose.onNodeWithText("192.168.1.20", substring = true).assertDoesNotExist()
        compose.onNodeWithText("192.168.43.1", substring = true).assertDoesNotExist()
        compose.onNodeWithText("¿Estáis en redes distintas?").performScrollTo().performClick()
        compose.onNodeWithText("WireGuard · Recomendada").assertIsDisplayed()
        compose.onNodeWithText("Instalar WireGuard").performScrollTo().performClick()
        assertEquals("https://www.wireguard.com/install/", shadowOf(compose.activity).nextStartedActivity.dataString)
        compose.onNodeWithText("Cómo conectar con Tailscale").performScrollTo().performClick()
        assertEquals("https://tailscale.com/docs/install/android", shadowOf(compose.activity).nextStartedActivity.dataString)
        compose.onNodeWithText("Usar conexión VPN").assertIsEnabled().performClick()
        compose.onNodeWithText("Compartir enlace del mando").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Copiar enlace").performScrollTo().performClick()
        val clipboard = compose.activity.getSystemService(ClipboardManager::class.java)
        assertEquals("100.100.10.2", PairStore.parsePairUrl(clipboard.primaryClip!!.getItemAt(0).text.toString())!!.host)
        capture("server-vpn-ui-test.png")
        compose.runOnIdle { ServerState.mutable.value = ServerState.flow.value.copy(vpnHosts = emptyList()) }
        compose.onNodeWithContentDescription("QR para conectar otro móvil a este servidor Android").assertDoesNotExist()
        compose.onNodeWithText("Copiar enlace").assertDoesNotExist()
        compose.onNodeWithText("Enlace copiado").assertDoesNotExist()
        compose.onNodeWithText("La VPN se ha desconectado.", substring = true).assertExists()
        compose.onNodeWithText("Volver a la conexión local").performScrollTo().performClick()
        compose.onNodeWithText("1234").performScrollTo().assertIsDisplayed()
    }

    @Test fun firstSetupExplainsTwoTapsAndTargetsTheEmulatorRootDirectly() {
        val packageName = "org.dolphinemu.dolphinemu"
        shadowOf(compose.activity.packageManager).addOrUpdateProvider(ProviderInfo().apply {
            this.packageName = packageName; authority = "$packageName.user"
            name = "OfficialDocumentsProvider"; exported = true; grantUriPermissions = true
        })
        ServerState.mutable.value = ServerUiState(
            config = ReceiverConfig("Android de prueba", "test-only-qr-credential", "1234"),
            hosts = listOf("192.168.1.20"),
            receiver = ReceiverSnapshot(true, ReceiverMode.Dolphin, 26761, 26760, emptyList(), 0))
        compose.setContent { PepoMoteTheme { ServerScreen {} } }
        compose.onNode(hasScrollToNodeAction()).performScrollToNode(hasText("2. Elige dónde jugar"))
        compose.waitUntil(5000) { compose.onAllNodesWithText("Configurar Dolphin").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Configurar Dolphin").performScrollTo().performClick()
        compose.onNodeWithText("Conecta PepoMote con Dolphin").assertIsDisplayed()
        compose.onNodeWithText("1. Toca «Usar esta carpeta».", substring = true).assertIsDisplayed()
        capture("server-permission-guide-ui-test.png")
        compose.onNodeWithText("Ir al permiso").performClick()
        val intent = shadowOf(compose.activity).nextStartedActivityForResult.intent
        assertEquals(Intent.ACTION_OPEN_DOCUMENT_TREE, intent.action)
        @Suppress("DEPRECATION")
        assertEquals(DocumentsContract.buildRootUri("$packageName.user", "root"),
            intent.getParcelableExtra<Uri>(DocumentsContract.EXTRA_INITIAL_URI))
    }

    @Test fun pendingRestartGuidesToExactAppAndRequiresTheVisitBeforeOpening() {
        val packageName = "org.dolphinemu.dolphinemu"
        shadowOf(compose.activity.packageManager).addOrUpdateProvider(ProviderInfo().apply {
            this.packageName = packageName; authority = "$packageName.user"
            name = "OfficialDocumentsProvider"; exported = true; grantUriPermissions = true
        })
        compose.activity.getSharedPreferences("server_setup_pending", Context.MODE_PRIVATE).edit()
            .putString("Dolphin", packageName).commit()
        ServerSetupJobs.refresh(compose.activity)
        ServerState.mutable.value = ServerUiState(
            receiver = ReceiverSnapshot(true, ReceiverMode.Dolphin, 26761, 26760, emptyList(), 0))
        compose.setContent { PepoMoteTheme { ServerScreen {} } }
        compose.onNode(hasScrollToNodeAction()).performScrollToNode(hasText("2. Elige dónde jugar"))
        compose.waitUntil(5000) { compose.onAllNodesWithText("Terminar configuración").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Terminar configuración").performScrollTo().performClick()
        compose.onNodeWithText("Ya lo he cerrado: jugar").assertIsNotEnabled()
        capture("server-restart-guide-ui-test.png")
        compose.onNodeWithText("Ir a la ficha de Dolphin").performClick()
        val intent = shadowOf(compose.activity).nextStartedActivity
        assertEquals(android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS, intent.action)
        assertEquals("package:$packageName", intent.dataString)
        compose.onNodeWithText("Ya lo he cerrado: jugar").assertIsEnabled()
    }

    @Test fun sharedVpnLinkCanBePastedWithoutTouchingSavedPcPairings() {
        var connected: String? = null
        val link = ServerIdentity.pairUrl(ReceiverConfig("Android remoto", "test-only-qr-credential", "1234"), "100.100.10.2")
        compose.setContent { PepoMoteTheme { PairScreen({}, {}, onPairLink = { connected = it }, discoverReceivers = { flowOf(emptyList()) }) } }
        compose.onNodeWithText("Introducir enlace").performScrollTo().performClick()
        compose.onNodeWithText("2. Pulsa Copiar enlace", substring = true).assertExists()
        compose.onNodeWithText("Enlace copiado del servidor").performTextInput("enlace incorrecto")
        compose.onNodeWithText("Conectar", substring = false).performScrollTo().performClick()
        assertNull(connected)
        compose.onNodeWithText("Este enlace no es válido.", substring = true).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Enlace copiado del servidor").performTextReplacement(link)
        capture("server-enter-link-ui-test.png")
        compose.onNodeWithText("Conectar", substring = false).performScrollTo().performClick()
        assertEquals(link, connected)
        assertEquals("android", PairStore.parsePairUrl(connected!!)!!.platform)
        assertEquals("100.100.10.2", PairStore.parsePairUrl(connected!!)!!.host)
    }

    @Test fun codeEntryPreservesLeadingZeroesAcrossComposableStateRestoration() {
        val pc = ReceiverInfo("Mi PC", "192.168.1.8", 26761)
        var connected: Pair<ReceiverInfo, String>? = null
        val restoration = StateRestorationTester(compose)
        restoration.setContent { PepoMoteTheme {
            PairScreen({}, {}, onDiscovered = { receiver, code -> connected = receiver to code },
                discoverReceivers = { flowOf(listOf(pc)) })
        } }
        compose.onNodeWithText("Escanear QR").assertIsDisplayed()
        compose.onNodeWithText("Conectar con código").assertIsDisplayed()
        capture("pair-options-ui-test.png")
        compose.onNodeWithText("Conectar con código").performClick()
        compose.onNodeWithText("Mi PC").performClick()
        compose.onNodeWithText("Código de conexión").performTextInput("004")
        compose.onNodeWithText("Conectar", substring = false).assertIsNotEnabled()
        compose.onNodeWithText("Código de conexión").performTextReplacement("00a42")
        restoration.emulateSavedInstanceStateRestore()
        compose.onNodeWithText("Código de conexión").assertTextContains("0042")
        compose.onNodeWithText("Mi PC").assertIsDisplayed()
        capture("pair-four-digit-code-ui-test.png")
        compose.onNodeWithText("Conectar", substring = false).assertIsEnabled().performClick()
        assertEquals(pc to "0042", connected)
    }

    @Test fun nearbyDesktopCardAlsoOpensCodeEntryDirectly() {
        val pc = ReceiverInfo("PC cercano", "192.168.1.8", 26761)
        compose.setContent { PepoMoteTheme { PairScreen({}, {}, discoverReceivers = { flowOf(listOf(pc)) }) } }
        compose.onNodeWithText("PC cercano").performScrollTo().performClick()
        compose.onNodeWithText("Código de conexión").assertIsDisplayed()
        compose.onNodeWithText("Escribe el código de 4 dígitos", substring = true).assertIsDisplayed()
    }

    @Test fun olderAndroidSixDigitCodeStillConnectsWithoutSubmittingEarly() {
        val android = ReceiverInfo("Android anterior", "192.168.1.9", 26761, "android")
        var connected: String? = null
        compose.setContent { PepoMoteTheme {
            PairScreen({}, {}, onDiscovered = { _, code -> connected = code }, discoverReceivers = { flowOf(listOf(android)) })
        } }
        compose.onNodeWithText("Android anterior").performScrollTo().performClick()
        compose.onNodeWithText("Código de conexión").performTextInput("1234")
        assertNull(connected)
        compose.onNodeWithText("Código de conexión").performTextInput("5")
        compose.onNodeWithText("Conectar", substring = false).assertIsNotEnabled()
        compose.onNodeWithText("Código de conexión").performTextInput("6")
        compose.onNodeWithText("Conectar", substring = false).assertIsEnabled().performClick()
        assertEquals("123456", connected)
    }

    @Test fun interruptedSetupOffersRecoveryInsteadOfOpeningTheEmulator() {
        val packageName = "org.dolphinemu.dolphinemu"
        shadowOf(compose.activity.packageManager).addOrUpdateProvider(ProviderInfo().apply {
            this.packageName = packageName; authority = "$packageName.user"
            name = "OfficialDocumentsProvider"; exported = true; grantUriPermissions = true
        })
        compose.activity.getSharedPreferences("emulator_setup", Context.MODE_PRIVATE).edit()
            .putString("Dolphin.package", packageName).commit()
        AndroidBackupStore(compose.activity, EmulatorTarget.Dolphin).save(
            SetupJournal(emptyList(), PendingTransaction(emptyList(), emptyList())))
        compose.activity.getSharedPreferences("server_setup_pending", Context.MODE_PRIVATE).edit()
            .putString("Dolphin", packageName).commit()
        ServerSetupJobs.refresh(compose.activity)
        ServerState.mutable.value = ServerUiState(
            receiver = ReceiverSnapshot(true, ReceiverMode.Dolphin, 26761, 26760, emptyList(), 0))
        compose.setContent { PepoMoteTheme { ServerScreen {} } }
        compose.onNode(hasScrollToNodeAction()).performScrollToNode(hasText("2. Elige dónde jugar"))
        compose.waitUntil(5000) { compose.onAllNodesWithText("Restaurar controles anteriores").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Terminar configuración").assertDoesNotExist()
        compose.onNodeWithText("Jugar con Dolphin").assertDoesNotExist()
        compose.onNodeWithText("Restaurar controles anteriores").performScrollTo().performClick()
        compose.onNodeWithText("Se restaurarán los ajustes", substring = true).assertIsDisplayed()
    }

    @Test @Config(qualifiers = "es-rES-w360dp-h640dp-night-xxhdpi")
    fun connectedControllerCollapsesQrAndNarrowDarkScreenKeepsSetupAccessible() {
        ServerState.mutable.value = ServerUiState(
            config = ReceiverConfig("Android de prueba", "test-only-qr-credential", "1234"),
            hosts = listOf("192.168.1.20"), receiver = ReceiverSnapshot(true, ReceiverMode.Eden, 26761, 26760,
                listOf(ReceiverPeerSnapshot("test-player", 0, "Mi mando", false, 80, 100.0, 5, 100)), 1))
        compose.setContent { PepoMoteTheme(dark = true) { ServerScreen {} } }
        compose.onNodeWithContentDescription("QR para conectar otro móvil a este servidor Android").assertDoesNotExist()
        compose.onNodeWithText("Añadir otro mando").assertIsDisplayed()
        compose.onNode(hasScrollToNodeAction()).performScrollToNode(hasText("2. Elige dónde jugar"))
        compose.onNodeWithText("Eden", substring = false).assertIsDisplayed()
        capture("server-connected-dark-ui-test.png")
    }

    private fun capture(name: String) {
        val root = listOf(File("../.."), File("..")).map { it.canonicalFile }
            .first { File(it, "android/app/src").isDirectory && File(it, "protocol/vectors").isDirectory }
        val directory = File(root, "dist/android-server-preview/screenshots").apply { mkdirs() }
        compose.runOnIdle {
            val view = ShadowDialog.getLatestDialog()?.takeIf { it.isShowing }?.window?.decorView
                ?: compose.activity.window.decorView
            val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
            view.draw(Canvas(bitmap))
            File(directory, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
}
