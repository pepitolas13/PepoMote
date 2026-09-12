package dev.pepotech.pepomote.service

/**
 * Tile de Ajustes rápidos: qué enseña y qué hace un toque, como función pura
 * de (enlace, hay PC emparejado). Testeado en TileModelTest.
 */
data class TileModel(val active: Boolean, val subtitle: String) {
    companion object {
        fun of(link: UiLink, hasPc: Boolean): TileModel = when (link) {
            is UiLink.Connected -> TileModel(true, "Conectado a ${link.pcName}")
            is UiLink.Connecting -> TileModel(true, "Conectando…")
            is UiLink.Reconnecting -> TileModel(true, "Reconectando…")
            else -> TileModel(false, if (hasPc) "Sin conexión" else "Sin PC")
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
