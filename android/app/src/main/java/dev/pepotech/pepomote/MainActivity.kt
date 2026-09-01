package dev.pepotech.pepomote

import android.Manifest
import android.os.Build
import android.os.Bundle
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
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.service.LinkForegroundService
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.screens.ControllerScreen
import dev.pepotech.pepomote.ui.screens.HomeScreen
import dev.pepotech.pepomote.ui.screens.PairScreen
import dev.pepotech.pepomote.ui.theme.PepoMoteTheme

private enum class Screen { Home, Pair, Controller }

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            PepoMoteTheme {
                Root()
            }
        }
    }
}

@Composable
private fun Root() {
    val context = LocalContext.current
    var screen by remember { mutableStateOf(Screen.Home) }
    val link by LinkState.flow.collectAsState()

    val notifPermission = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { /* con o sin permiso, el servicio arranca; solo cambia la notificación */ }

    val qrLauncher = rememberLauncherForActivityResult(ScanContract()) { result ->
        val contents = result.contents ?: return@rememberLauncherForActivityResult
        val pairing = PairStore.parsePairUrl(contents)
        if (pairing == null) {
            Toast.makeText(context, "Ese QR no es de PepoMote", Toast.LENGTH_LONG).show()
            return@rememberLauncherForActivityResult
        }
        PairStore.save(context, pairing)
        if (Build.VERSION.SDK_INT >= 33) {
            notifPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        LinkForegroundService.start(context)
        screen = Screen.Controller
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

    when (screen) {
        Screen.Home -> HomeScreen(
            connected = link is UiLink.Connected,
            onConnect = {
                // Si ya hay emparejamiento guardado, reconecta directo
                if (PairStore.load(context) != null && link is UiLink.Disconnected) {
                    if (Build.VERSION.SDK_INT >= 33) {
                        notifPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
                    }
                    LinkForegroundService.start(context)
                    screen = Screen.Controller
                } else {
                    screen = Screen.Pair
                }
            },
            onController = { screen = Screen.Controller },
            onDolphin = {
                LinkState.sendMode?.invoke("dolphin")
                screen = Screen.Controller
            },
            onNewPairing = { screen = Screen.Pair }
        )

        Screen.Pair -> PairScreen(
            onScanQr = { scanQr() },
            onBack = { screen = Screen.Home }
        )

        Screen.Controller -> {
            if (link is UiLink.Failed) {
                val f = link as UiLink.Failed
                Toast.makeText(context, "Error: ${f.msg}", Toast.LENGTH_LONG).show()
                screen = Screen.Home
            }
            ControllerScreen(
                link = link,
                onDisconnect = {
                    LinkForegroundService.stop(context)
                    screen = Screen.Home
                }
            )
        }
    }
}
