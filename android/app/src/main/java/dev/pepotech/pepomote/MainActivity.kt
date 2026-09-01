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
import androidx.compose.runtime.LaunchedEffect
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

    /** true = se entró al mando por la tarjeta Dolphin (pantalla solo-Dolphin). */
    internal var controllerDolphinOnly by mutableStateOf(false)

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

    /** Bits pulsados por las teclas de volumen: su UP se procesa SIEMPRE. */
    private var volumeHeld = 0

    /**
     * Botones físicos de volumen mientras el mando está abierto: subir = A,
     * bajar = gatillo B. Tacto real con latencia cero. Configurable en Ajustes.
     * La duración mínima del toque en el cable la pone ButtonState (PressLatch).
     */
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val bit = when (event.keyCode) {
            KeyEvent.KEYCODE_VOLUME_UP -> ButtonState.A
            KeyEvent.KEYCODE_VOLUME_DOWN -> ButtonState.B
            else -> return super.dispatchKeyEvent(event)
        }
        when (event.action) {
            KeyEvent.ACTION_DOWN -> if (currentScreen == Screen.Controller && AppPrefs.volDownIsB(this)) {
                if (event.repeatCount == 0) {
                    volumeHeld = volumeHeld or bit
                    ButtonState.set(bit, true)
                    if (bit == ButtonState.A) UiSounds.pop() else UiSounds.blip()
                    window.decorView.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                }
                return true
            }

            // Lo que pulsamos nosotros lo soltamos nosotros, aunque ya no
            // estemos en el mando: si no, el botón se quedaba pulsado
            KeyEvent.ACTION_UP -> if (volumeHeld and bit != 0) {
                volumeHeld = volumeHeld and bit.inv()
                ButtonState.set(bit, false)
                return true
            }
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

    // Sin petición de permiso de notificaciones: el servicio funciona igual
    // sin él; si el usuario lo concede a mano, la notificación con
    // "Desconectar" aparece. Cero fricción en el primer arranque.
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
        activity.currentScreen = Screen.Controller
    }

    fun scanQr() {
        qrLauncher.launch(
            ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setPrompt("Apunta al QR de PepoMote en tu PC")
                .setBeepEnabled(false)
                .setOrientationLocked(true)
                .setCaptureActivity(PortraitCaptureActivity::class.java)
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
                activity.controllerDolphinOnly = false
                when {
                    // Ya conectado (por Dolphin o lo que sea): al mando en
                    // modo puntero — nunca al escáner
                    link is UiLink.Connected || link is UiLink.Connecting -> {
                        LinkState.sendMode?.invoke("pointer")
                        activity.currentScreen = Screen.Controller
                    }

                    PairStore.load(context) != null -> {
                        LinkState.pendingMode = "pointer"
                        LinkForegroundService.start(context)
                        activity.currentScreen = Screen.Controller
                    }

                    else -> activity.currentScreen = Screen.Pair
                }
            },
            onController = {
                activity.controllerDolphinOnly = false
                activity.currentScreen = Screen.Controller
            },
            onDolphin = {
                activity.controllerDolphinOnly = true
                when {
                    link is UiLink.Connected || link is UiLink.Connecting -> {
                        LinkState.sendMode?.invoke("dolphin")
                        activity.currentScreen = Screen.Controller
                    }

                    PairStore.load(context) != null -> {
                        LinkState.pendingMode = "dolphin"
                        LinkForegroundService.start(context)
                        activity.currentScreen = Screen.Controller
                    }

                    else -> activity.currentScreen = Screen.Pair
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
            // Error de conexión: aviso y vuelta al inicio como EFECTO (no en
            // plena composición, que lo repetía) y el estado se limpia para
            // que el próximo Conectar no rebote con el error viejo.
            LaunchedEffect(link) {
                (link as? UiLink.Failed)?.let { f ->
                    Toast.makeText(context, "Error: ${f.msg}", Toast.LENGTH_LONG).show()
                    LinkState.clearFailure()
                    activity.currentScreen = Screen.Home
                }
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
                ControllerScreen(
                    link = link,
                    // Selector Puntero/Dolphin: solo entrando por Conectar/Mando
                    // y con el ajuste activo. Por Dolphin: pantalla solo-Dolphin.
                    showChips = !activity.controllerDolphinOnly &&
                        AppPrefs.showDolphinChips(context),
                    onDisconnect = onDisconnect
                )
            }
        }
    }
}
