package dev.pepotech.pepomote

import android.content.Intent
import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.os.Bundle
import android.view.HapticFeedbackConstants
import android.view.KeyEvent
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.control.UiSounds
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.service.LinkFailure
import dev.pepotech.pepomote.service.LinkForegroundService
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.PadScreen
import dev.pepotech.pepomote.service.Route
import dev.pepotech.pepomote.service.ScreenLink
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.service.alive
import dev.pepotech.pepomote.ui.screens.HomeStatus
import dev.pepotech.pepomote.ui.screens.HomeTone
import dev.pepotech.pepomote.ui.screens.ControllerLandscapeScreen
import dev.pepotech.pepomote.ui.screens.ControllerScreen
import dev.pepotech.pepomote.ui.screens.GamePadScreen
import dev.pepotech.pepomote.ui.screens.HomeScreen
import dev.pepotech.pepomote.ui.screens.NunchukScreen
import dev.pepotech.pepomote.ui.screens.OnboardingScreen
import dev.pepotech.pepomote.ui.screens.PairScreen
import dev.pepotech.pepomote.ui.screens.SettingsScreen
import dev.pepotech.pepomote.ui.theme.PepoMoteTheme

/**
 * `Controller` es el mando en general: la pantalla real (GamePad de Wii U,
 * layouts Wii o Nunchuk) la decide [Route] a partir del enlace y de la
 * intención pendiente del usuario.
 */
internal enum class Screen { Onboarding, Home, Pair, Controller, Nunchuk, Settings }

class MainActivity : ComponentActivity() {

    internal var currentScreen by mutableStateOf(Screen.Home)

    /** true = se entró al mando por la tarjeta Dolphin (pantalla solo-Dolphin). */
    internal var controllerDolphinOnly by mutableStateOf(false)

    /**
     * Rol con el que se entró (Mando/Dolphin/Wii U = wiimote, Nunchuk = nunchuk):
     * tras escanear el QR (o abrir el enlace profundo) el enlace arranca con él
     * y se abre su pantalla.
     */
    internal var linkRole by mutableStateOf(LinkState.ROLE_WIIMOTE)

    /**
     * Por qué está abierta la pantalla Conectar: el PC rechazó el
     * emparejamiento guardado (`bad_token`) y el escáner se ofrece ahí mismo
     * con la explicación. null = emparejamiento normal (primera vez, Ajustes).
     */
    internal var pairReason by mutableStateOf<String?>(null)

    /** A la pantalla Conectar (escáner QR), con explicación si viene de un rechazo. */
    internal fun openPair(reason: String? = null) {
        pairReason = reason
        currentScreen = Screen.Pair
    }

    /**
     * El enlace ha muerto con error (el servicio ya se ha parado). Si el PC no
     * reconoce el emparejamiento, al escáner con la explicación: el rol y el
     * modo pedidos se conservan y al escanear se sigue donde se estaba. Con
     * cualquier otro error, aviso y vuelta al inicio (y el modo pedido se
     * olvida, que no salte en una reconexión posterior).
     */
    internal fun onLinkFailed(failure: UiLink.Failed) {
        LinkState.clearFailure()
        if (LinkFailure.needsNewQr(failure.code)) {
            openPair(LinkFailure.rePairReason(PairStore.load(this)?.pcName))
        } else {
            Toast.makeText(this, "Error: ${failure.msg}", Toast.LENGTH_LONG).show()
            LinkState.pendingMode = null
            currentScreen = Screen.Home
        }
    }

    /** Hay un enlace vivo (o arrancando o rehaciéndose) como Nunchuk. */
    private fun linkIsNunchuk(): Boolean =
        LinkState.flow.value.alive && LinkState.role == LinkState.ROLE_NUNCHUK

    /**
     * Al mando (wiimote) en el modo pedido. El rol no se cambia en caliente:
     * un enlace vivo como Nunchuk se rehace como mando. `cemu` deja intención
     * Wii U: el GamePad se abre al instante. `dolphinOnly`: se entra por la
     * tarjeta Dolphin (pantalla solo-Dolphin, sin selector de modo).
     */
    internal fun openController(mode: String, dolphinOnly: Boolean) {
        controllerDolphinOnly = dolphinOnly
        linkRole = LinkState.ROLE_WIIMOTE
        when {
            // Ya conectado como mando (por Dolphin o lo que sea): al mando en
            // ese modo — nunca al escáner
            LinkState.flow.value.alive && !linkIsNunchuk() -> {
                LinkState.requestMode(mode)
                currentScreen = Screen.Controller
            }

            PairStore.load(this) != null -> {
                LinkState.requestMode(mode) // se aplica al llegar el ok
                LinkForegroundService.start(this)
                currentScreen = Screen.Controller
            }

            // Sin emparejar: al escáner, y el modo pedido se aplica al
            // conectar tras el QR (Wii U abre el GamePad)
            else -> {
                LinkState.requestMode(mode)
                openPair()
            }
        }
    }

    /** Tarjeta «Mando»: al mando sin tocar el modo del receptor. */
    internal fun openPad() {
        controllerDolphinOnly = false
        linkRole = LinkState.ROLE_WIIMOTE
        when {
            // Sin emparejar no hay mando que abrir: al escáner
            PairStore.load(this) == null -> openPair()

            else -> {
                if (linkIsNunchuk()) LinkForegroundService.start(this)
                currentScreen = Screen.Controller
            }
        }
    }

    /** Tarjeta «Nunchuk»: el móvil de la otra mano. */
    internal fun openNunchuk() {
        linkRole = LinkState.ROLE_NUNCHUK
        when {
            linkIsNunchuk() -> currentScreen = Screen.Nunchuk

            PairStore.load(this) != null -> {
                // Rehace el enlace si estaba vivo como mando
                LinkForegroundService.start(this, LinkState.ROLE_NUNCHUK)
                currentScreen = Screen.Nunchuk
            }

            else -> openPair()
        }
    }

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
        // Enlace profundo pepomote://pair?… al abrir la app (no al recrearla)
        if (savedInstanceState == null) intent?.data?.let { onPairContent(it.toString()) }
    }

    /** singleTop: con la app ya abierta, el enlace profundo llega aquí. */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.data?.let { onPairContent(it.toString()) }
    }

    /**
     * La doble pantalla del GamePad solo se recibe con la app a la vista:
     * en segundo plano (ON_STOP) se cierra el canal y al volver se reabre.
     */
    override fun onStart() {
        super.onStart()
        ScreenLink.setForeground(true)
    }

    override fun onStop() {
        ScreenLink.setForeground(false)
        super.onStop()
    }

    /**
     * Contenido del QR o del enlace profundo (pepomote://pair?v=1&host=…&port=…&t=…&name=…):
     * si vale, empareja, arranca el enlace con el rol elegido y abre su pantalla.
     */
    internal fun onPairContent(contents: String) {
        val pairing = PairStore.parsePairUrl(contents)
        if (pairing == null) {
            Toast.makeText(this, "Ese QR no es de PepoMote", Toast.LENGTH_LONG).show()
            return
        }
        PairStore.save(this, pairing)
        pairReason = null
        // Lo pedido antes de tener que escanear (Wii U, Dolphin…) se vuelve a
        // pedir: la intención se restaura y el modo va en cuanto llegue el ok
        if (linkRole == LinkState.ROLE_WIIMOTE) LinkState.pendingMode?.let { LinkState.requestMode(it) }
        // Servicio ANTES del diálogo de permiso: pedirlo primero dejaba el
        // arranque del servicio compitiendo con el diálogo del sistema y el
        // primer emparejamiento fallaba en algunos OEMs.
        LinkForegroundService.start(this, linkRole)
        currentScreen = if (linkRole == LinkState.ROLE_NUNCHUK) Screen.Nunchuk else Screen.Controller
    }

    /** Bits pulsados por las teclas de volumen: su UP se procesa SIEMPRE. */
    private var volumeHeld = 0

    /**
     * Botones físicos de volumen mientras el mando (o el GamePad) está abierto:
     * subir = A, bajar = gatillo B. Tacto real con latencia cero. Configurable
     * en Ajustes. La duración mínima del toque en el cable la pone ButtonState
     * (PressLatch).
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

    // Error de conexión como EFECTO (no en plena composición, que lo
    // repetía), esté la pantalla que esté: al escáner si el PC ya no
    // reconoce el emparejamiento, o aviso y vuelta al inicio; el estado se
    // limpia para que el próximo Conectar no rebote con el error viejo.
    LaunchedEffect(link) {
        (link as? UiLink.Failed)?.let { activity.onLinkFailed(it) }
    }

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
        activity.onPairContent(contents)
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

        Screen.Home -> {
            val l = link
            val pcName = PairStore.load(context)?.pcName
            val status = when (l) {
                is UiLink.Connected -> HomeStatus(HomeTone.On, stringResource(R.string.status_connected_to, l.pcName))
                is UiLink.Connecting -> HomeStatus(HomeTone.Busy, stringResource(R.string.status_connecting))
                is UiLink.Reconnecting -> HomeStatus(HomeTone.Busy, stringResource(R.string.status_reconnecting, l.pcName))
                else -> HomeStatus(
                    HomeTone.Off,
                    if (pcName != null) stringResource(R.string.status_disconnected_pc, pcName)
                    else stringResource(R.string.status_disconnected)
                )
            }
            HomeScreen(
                status = status,
                onConnect = { activity.openController(LinkState.MODE_POINTER, dolphinOnly = false) },
                onController = { activity.openPad() },
                onDolphin = { activity.openController(LinkState.MODE_DOLPHIN, dolphinOnly = true) },
                onWiiU = { activity.openController(LinkState.MODE_CEMU, dolphinOnly = false) },
                onNunchuk = { activity.openNunchuk() },
                onNewPairing = { activity.currentScreen = Screen.Settings }
            )
        }

        Screen.Pair -> PairScreen(
            reason = activity.pairReason,
            onScanQr = { scanQr() },
            onBack = {
                LinkState.pendingMode = null // lo pedido antes del QR ya no va
                activity.currentScreen = Screen.Home
            }
        )

        Screen.Settings -> SettingsScreen(
            onNewPairing = {
                activity.linkRole = LinkState.ROLE_WIIMOTE // QR desde Ajustes: mando
                LinkState.pendingMode = null
                activity.openPair()
            },
            onBack = { activity.currentScreen = Screen.Home }
        )

        Screen.Nunchuk -> {
            NunchukScreen(
                link = link,
                onDisconnect = {
                    LinkForegroundService.stop(context)
                    activity.currentScreen = Screen.Home
                }
            )
        }

        Screen.Controller -> ControllerRoute(activity, link)
    }
}

/**
 * El mando, según [Route]: GamePad de Wii U (confirmado o pedido y pendiente;
 * se fuerza apaisado), Nunchuk, o los layouts Wii de siempre según la
 * orientación.
 */
@Composable
private fun ControllerRoute(activity: MainActivity, link: UiLink) {
    val context = LocalContext.current
    val intent by LinkState.intent.collectAsState()

    val onDisconnect = {
        LinkState.clearIntent()
        LinkForegroundService.stop(context)
        activity.currentScreen = Screen.Home
    }

    when (Route.route(link, intent)) {
        PadScreen.GamePad -> {
            // Apaisado fijo mientras dure el GamePad; al salir, como estaba
            DisposableEffect(Unit) {
                activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
                onDispose {
                    activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
                }
            }
            GamePadScreen(link = link, onDisconnect = onDisconnect)
        }

        PadScreen.Nunchuk -> NunchukScreen(link = link, onDisconnect = onDisconnect)

        PadScreen.Wii -> {
            val landscape =
                LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
            // Selector Puntero/Dolphin/Wii U: entrando por Conectar/Mando/Wii U
            // con el ajuste activo (y siempre dentro de Wii U). Por Dolphin:
            // pantalla solo-Dolphin. Igual en vertical y de lado.
            val showChips = !activity.controllerDolphinOnly && AppPrefs.showDolphinChips(context)
            if (landscape) {
                ControllerLandscapeScreen(link = link, showChips = showChips, onDisconnect = onDisconnect)
            } else {
                ControllerScreen(link = link, showChips = showChips, onDisconnect = onDisconnect)
            }
        }
    }
}
