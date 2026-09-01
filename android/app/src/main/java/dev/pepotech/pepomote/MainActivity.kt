package dev.pepotech.pepomote

import android.Manifest
import android.content.res.Configuration
import android.os.Build
import android.os.Bundle
import android.view.HapticFeedbackConstants
import android.view.KeyEvent
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.control.UiSounds
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.service.LinkForegroundService
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.screens.ControllerLandscapeScreen
import dev.pepotech.pepomote.ui.screens.ControllerScreen
import dev.pepotech.pepomote.ui.screens.HomeScreen
import dev.pepotech.pepomote.ui.screens.OnboardingScreen
import dev.pepotech.pepomote.ui.screens.PairScreen
import dev.pepotech.pepomote.ui.screens.SettingsScreen
import dev.pepotech.pepomote.ui.theme.PepoMoteTheme

internal enum class Screen { Onboarding, Home, Pair, Controller, Settings }

class MainActivity : ComponentActivity() {

    internal var currentScreen by mutableStateOf(Screen.Home)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        UiSounds.init(this)
        if (!AppPrefs.onboarded(this)) currentScreen = Screen.Onboarding
        setContent {
            PepoMoteTheme {
                Root(this)
            }
        }
    }

    /**
     * Volumen-abajo = gatillo B mientras el mando está abierto: tacto físico
     * real con latencia táctil cero. Configurable en Ajustes.
     */
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (event.keyCode == KeyEvent.KEYCODE_VOLUME_DOWN &&
            currentScreen == Screen.Controller &&
            AppPrefs.volDownIsB(this)
        ) {
            when (event.action) {
                KeyEvent.ACTION_DOWN -> if (event.repeatCount == 0) {
                    ButtonState.set(ButtonState.B, true)
                    UiSounds.blip()
                    window.decorView.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                }

                KeyEvent.ACTION_UP -> ButtonState.set(ButtonState.B, false)
            }
            return true
        }
        return super.dispatchKeyEvent(event)
    }
}

@Composable
private fun Root(activity: MainActivity) {
    val context = LocalContext.current
    val link by LinkState.flow.collectAsState()

    // Gesto/botón atrás: dentro de la app vuelve al inicio en vez de salir.
    // En Home (y en el onboarding) se comporta como siempre: sale.
    androidx.activity.compose.BackHandler(
        enabled = activity.currentScreen != Screen.Home &&
            activity.currentScreen != Screen.Onboarding
    ) {
        activity.currentScreen = Screen.Home
    }

    val notifPermission = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { /* con o sin permiso, el servicio arranca; solo cambia la notificación */ }

    fun ensureNotifPermission() {
        if (Build.VERSION.SDK_INT >= 33) {
            notifPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    val qrLauncher = rememberLauncherForActivityResult(ScanContract()) { result ->
        val contents = result.contents ?: return@rememberLauncherForActivityResult
        val pairing = PairStore.parsePairUrl(contents)
        if (pairing == null) {
            Toast.makeText(context, "Ese QR no es de PepoMote", Toast.LENGTH_LONG).show()
            return@rememberLauncherForActivityResult
        }
        PairStore.save(context, pairing)
        // Servicio ANTES del diálogo de permiso: pedirlo primero dejaba el
        // arranque del servicio compitiendo con el diálogo del sistema y el
        // primer emparejamiento fallaba en algunos OEMs.
        LinkForegroundService.start(context)
        ensureNotifPermission()
        activity.currentScreen = Screen.Controller
    }

    fun scanQr() {
        qrLauncher.launch(
            ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setPrompt("Apunta al QR de PepoMote en tu PC")
                .setBeepEnabled(false)
                .setOrientationLocked(true)
        )
    }

    when (activity.currentScreen) {
        Screen.Onboarding -> OnboardingScreen(onDone = {
            AppPrefs.setOnboarded(context)
            activity.currentScreen = Screen.Home
        })

        Screen.Home -> HomeScreen(
            connected = link is UiLink.Connected,
            onConnect = {
                if (PairStore.load(context) != null && link is UiLink.Disconnected) {
                    LinkForegroundService.start(context)
                    ensureNotifPermission()
                    activity.currentScreen = Screen.Controller
                } else {
                    activity.currentScreen = Screen.Pair
                }
            },
            onController = { activity.currentScreen = Screen.Controller },
            onDolphin = {
                if (link is UiLink.Connected) {
                    LinkState.sendMode?.invoke("dolphin")
                    activity.currentScreen = Screen.Controller
                } else if (PairStore.load(context) != null) {
                    LinkState.pendingMode = "dolphin"
                    LinkForegroundService.start(context)
                    ensureNotifPermission()
                    activity.currentScreen = Screen.Controller
                } else {
                    activity.currentScreen = Screen.Pair
                }
            },
            onNewPairing = { activity.currentScreen = Screen.Settings }
        )

        Screen.Pair -> PairScreen(
            onScanQr = { scanQr() },
            onBack = { activity.currentScreen = Screen.Home }
        )

        Screen.Settings -> SettingsScreen(
            onNewPairing = { activity.currentScreen = Screen.Pair },
            onBack = { activity.currentScreen = Screen.Home }
        )

        Screen.Controller -> {
            if (link is UiLink.Failed) {
                val f = link as UiLink.Failed
                Toast.makeText(context, "Error: ${f.msg}", Toast.LENGTH_LONG).show()
                activity.currentScreen = Screen.Home
            }
            val landscape =
                LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
            val onDisconnect = {
                LinkForegroundService.stop(context)
                activity.currentScreen = Screen.Home
            }
            if (landscape) {
                ControllerLandscapeScreen(link = link, onDisconnect = onDisconnect)
            } else {
                ControllerScreen(link = link, onDisconnect = onDisconnect)
            }
        }
    }
}
