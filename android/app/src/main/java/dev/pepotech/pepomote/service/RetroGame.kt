package dev.pepotech.pepomote.service

import org.json.JSONObject

/**
 * Lo que el receptor sabe del juego cargado en RetroArch (mensaje `game`,
 * PROTOCOL.md §3): con [console] (un id de `RetroLayouts.CONSOLE_IDS`, o null
 * si no la conoce) el móvil elige la plantilla de mando, y [path] es la clave
 * para recordar la elegida a mano por juego. Solo cuenta en modo RetroArch;
 * se conserva al cambiar de modo y se pierde al desconectar.
 */
data class RetroGame(
    val console: String?,
    val system: String,
    val core: String,
    val title: String,
    val path: String
) {
    companion object {
        /** `{"m":"game",...}` → ficha; null cuando el receptor no tiene juego (todo vacío). */
        fun parse(msg: JSONObject): RetroGame? {
            val console = msg.optString("console", "").takeIf { it.isNotBlank() && !msg.isNull("console") }
            val game = RetroGame(
                console = console,
                system = msg.optString("system", ""),
                core = msg.optString("core", ""),
                title = msg.optString("title", ""),
                path = msg.optString("path", "")
            )
            return game.takeIf { it.title.isNotEmpty() || it.path.isNotEmpty() || it.console != null }
        }
    }
}
