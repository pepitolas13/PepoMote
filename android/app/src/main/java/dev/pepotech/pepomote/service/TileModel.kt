package dev.pepotech.pepomote.service

import androidx.annotation.StringRes
import dev.pepotech.pepomote.R

/**
 * Tile de Ajustes rápidos: qué enseña y qué hace un toque, como función pura
 * de (enlace, hay PC emparejado). Testeado en TileModelTest.
 */
data class TileModel(val active: Boolean, @StringRes val subtitle: Int, val arg: String? = null) {
    companion object {
        fun of(link: UiLink, hasPc: Boolean): TileModel = when (link) {
            is UiLink.Connected -> TileModel(true, R.string.status_connected_to, link.pcName)
            is UiLink.Connecting -> TileModel(true, R.string.status_connecting)
            is UiLink.Reconnecting -> TileModel(true, R.string.reconnecting_short)
            else -> TileModel(false, if (hasPc) R.string.status_disconnected else R.string.tile_no_pc)
        }
    }
}

enum class TileAction { Stop, Start, OpenApp }

/** Un toque: con enlace vivo, desconectar; con PC, conectar; sin PC, abrir la app. */
fun tileAction(link: UiLink, hasPc: Boolean): TileAction = when {
    link.alive -> TileAction.Stop
    hasPc -> TileAction.Start
    else -> TileAction.OpenApp
}
