package dev.pepotech.pepomote.server.retro

import org.json.JSONObject

/** La entrada más reciente del historial de RetroArch (`content_history.lpl`, `items[0]`). */
data class HistoryEntry(
    val path: String = "",
    val label: String = "",
    val corePath: String = "",
    val coreName: String = "",
    val dbName: String = "",
)

/** Resultado de leer el historial: roto o a medias (se reintenta), vacío, o su entrada más reciente. */
sealed class HistoryTop {
    object Broken : HistoryTop()
    object Empty : HistoryTop()
    data class Entry(val entry: HistoryEntry) : HistoryTop()
}

/** Lo que se anuncia a los móviles (`game`) y enseña el panel. */
data class RetroGameInfo(
    val console: Console?,
    /** Nombre de la consola; vacío si no tiene plantilla. */
    val system: String,
    val core: String,
    val title: String,
    val path: String,
)

/**
 * Qué juego acaba de cargar RetroArch, igual que `desktop/src/retroarch/history.rs`:
 * RetroArch escribe su historial (JSON) al cargar contenido, con la ruta, el
 * núcleo y la base de datos. Sin `GET_STATUS` (cierra la 1.22.2).
 */
object RetroHistory {
    fun parseTop(text: String): HistoryTop {
        val items = try {
            JSONObject(text).optJSONArray("items")
        } catch (_: Exception) {
            return HistoryTop.Broken
        } ?: return HistoryTop.Empty
        if (items.length() == 0) return HistoryTop.Empty
        val item = items.optJSONObject(0) ?: return HistoryTop.Empty
        fun s(k: String) = if (item.isNull(k)) "" else item.optString(k, "")
        return HistoryTop.Entry(HistoryEntry(s("path"), s("label"), s("core_path"), s("core_name"), s("db_name")))
    }

    /** «Sega - MS/GG/MD/CD (Genesis Plus GX)» → «Genesis Plus GX» (el último grupo entre paréntesis; si no lo hay, todo). */
    fun shortCoreName(displayName: String): String {
        val t = displayName.trim()
        val open = t.lastIndexOf('(')
        if (open >= 0) {
            val close = t.indexOf(')', open)
            if (close > open) {
                val inner = t.substring(open + 1, close).trim()
                if (inner.isNotEmpty()) return inner
            }
        }
        return t
    }

    /** Quita los grupos finales ` (USA)`, ` [!]`… y espacios. */
    internal fun cleanTitle(s: String): String {
        var t = s.trim()
        while (true) {
            val trimmed = t.trimEnd()
            val last = trimmed.lastOrNull() ?: break
            val open = when (last) { ')' -> '('; ']' -> '['; else -> break }
            val pos = trimmed.lastIndexOf(open)
            if (pos <= 0) break
            t = trimmed.substring(0, pos).trimEnd()
        }
        return t
    }

    /** Título: `label` si no está vacío, si no el nombre del archivo (tras `#`) sin extensión; sin los grupos finales. */
    fun titleFor(label: String, path: String): String {
        if (label.isNotBlank()) return cleanTitle(label)
        val name = RetroConsoles.contentFileName(path)
        val dot = name.lastIndexOf('.')
        val stem = if (dot > 0 && ' ' !in name.substring(dot + 1)) name.substring(0, dot) else name
        return cleanTitle(stem)
    }

    fun gameFrom(entry: HistoryEntry, systemId: String? = null): RetroGameInfo {
        val coreFile = RetroConsoles.contentFileName(entry.corePath)
        val console = RetroConsoles.consoleFor(systemId, coreFile, entry.path, entry.dbName)
        val core = RetroHistory.shortCoreName(entry.coreName).ifEmpty { RetroConsoles.coreStem(entry.corePath) }
        return RetroGameInfo(console, console?.displayName.orEmpty(), core, titleFor(entry.label, entry.path), entry.path)
    }

    /** `{"m":"game",...}`: las cinco claves siempre; sin juego, `console` null y cadenas vacías. */
    fun gameMessage(game: RetroGameInfo?): JSONObject = JSONObject().put("m", "game")
        .put("console", game?.console?.id ?: JSONObject.NULL)
        .put("system", game?.system.orEmpty()).put("core", game?.core.orEmpty())
        .put("title", game?.title.orEmpty()).put("path", game?.path.orEmpty())
}

/** Un historial localizado: su sello (mtime, tamaño) y su texto; null si desapareció. */
interface HistoryDocument {
    fun stamp(): Pair<Long, Long>?
    fun read(): String?
}

/** Dónde buscar el historial (la carpeta RetroArch elegida). */
fun interface HistorySource {
    fun locate(): HistoryDocument?
}

/**
 * Vigila el historial: relee cuando cambia (mtime, tamaño) o cuando RetroArch
 * acaba de responder (la misma partida recargada no toca el archivo); un JSON
 * a medias conserva lo anterior y reintenta; sin archivo, sin juego. `poll`
 * devuelve true solo cuando cambia el juego anunciado.
 */
class RetroGameWatch(private val source: HistorySource) {
    private var document: HistoryDocument? = null
    private var seen: Pair<Long, Long>? = null
    private var wasReachable = false
    var game: RetroGameInfo? = null
        private set
    /** Se ha encontrado el archivo alguna vez desde la última búsqueda. */
    var located: Boolean = false
        private set

    fun poll(reachable: Boolean): Boolean {
        val reopened = reachable && !wasReachable
        wasReachable = reachable
        val doc = document ?: source.locate().also { document = it; seen = null }
        located = doc != null
        if (doc == null) return update(null)
        val stamp = doc.stamp()
        if (stamp == null) {
            document = null
            seen = null
            located = false
            return update(null)
        }
        if (stamp == seen && !reopened) return false
        val text = doc.read() ?: return false
        val next = when (val top = RetroHistory.parseTop(text)) {
            HistoryTop.Broken -> return false
            HistoryTop.Empty -> null
            is HistoryTop.Entry -> RetroHistory.gameFrom(top.entry)
        }
        seen = stamp
        return update(next)
    }

    /** La carpeta cambió o se quitó: se vuelve a buscar (y se olvida el juego). */
    fun reset() {
        document = null
        seen = null
        located = false
        game = null
    }

    private fun update(next: RetroGameInfo?): Boolean {
        if (next == game) return false
        game = next
        return true
    }
}
