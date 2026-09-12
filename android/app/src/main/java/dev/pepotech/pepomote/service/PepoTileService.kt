package dev.pepotech.pepomote.service

import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.content.Context
import dev.pepotech.pepomote.MainActivity
import dev.pepotech.pepomote.control.LocaleHelper
import dev.pepotech.pepomote.net.PairStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * Tile de Ajustes rápidos: enseña el estado del enlace y, con un toque,
 * conecta o desconecta sin abrir la app (sin PC emparejado, la abre).
 */
class PepoTileService : TileService() {

    private var scope: CoroutineScope? = null

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(LocaleHelper.wrap(newBase))
    }

    override fun onStartListening() {
        super.onStartListening()
        scope?.cancel()
        scope = CoroutineScope(Dispatchers.Main + SupervisorJob()).also { s ->
            s.launch { LinkState.flow.collect { render(it) } }
        }
    }

    override fun onStopListening() {
        scope?.cancel()
        scope = null
        super.onStopListening()
    }

    private fun render(link: UiLink) {
        val tile = qsTile ?: return
        val m = TileModel.of(link, PairStore.load(this) != null)
        tile.state = if (m.active) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.label = "PepoMote"
        if (Build.VERSION.SDK_INT >= 29) {
            tile.subtitle = if (m.arg != null) getString(m.subtitle, m.arg) else getString(m.subtitle)
        }
        tile.updateTile()
    }

    override fun onClick() {
        super.onClick()
        when (tileAction(LinkState.flow.value, PairStore.load(this) != null)) {
            TileAction.Stop -> LinkForegroundService.stop(this)
            TileAction.Start -> try {
                LinkForegroundService.start(this)
            } catch (_: Exception) {
                // Android 14+ puede vetar un servicio en primer plano desde aquí:
                // la app lo arranca al abrirse
                openApp()
            }

            TileAction.OpenApp -> openApp()
        }
    }

    private fun openApp() {
        val i = Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= 34) {
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, i, PendingIntent.FLAG_IMMUTABLE))
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(i)
        }
    }
}
