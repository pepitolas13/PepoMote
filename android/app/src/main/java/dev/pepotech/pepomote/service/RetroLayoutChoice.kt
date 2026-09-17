package dev.pepotech.pepomote.service

import org.json.JSONArray
import org.json.JSONObject

/**
 * Mando de consola elegido a mano en RetroArch: [global] («auto» o un id de
 * plantilla) y, por juego (ruta del contenido que anuncia el receptor), la
 * plantilla que se prefiere (tope [CAP], el más viejo fuera). Puro e
 * inmutable; se guarda como JSON en las preferencias.
 */
data class RetroLayoutChoice(
    val global: String = AUTO,
    val byPath: List<Pair<String, String>> = emptyList()
) {
    /** La elegida a mano para ese juego (o la global), o null = automático. */
    fun choiceFor(path: String?): String? =
        path?.let { p -> byPath.firstOrNull { it.first == p }?.second }
            ?: global.takeIf { it != AUTO }

    /**
     * Elegir [id] (null = automático) para el juego [path] (null = sin juego
     * cargado: vale para todos hasta que RetroArch cargue uno). «Automático»
     * con un juego deja ese juego siguiendo al PC, y la global también.
     */
    fun pick(path: String?, id: String?): RetroLayoutChoice {
        if (path == null) return copy(global = id ?: AUTO)
        val rest = byPath.filter { it.first != path }
        return if (id == null) {
            copy(global = AUTO, byPath = rest)
        } else {
            copy(byPath = (rest + (path to id)).takeLast(CAP))
        }
    }

    fun encode(): String = JSONObject()
        .put("global", global)
        .put("byPath", JSONArray().apply { byPath.forEach { put(JSONArray().put(it.first).put(it.second)) } })
        .toString()

    companion object {
        const val AUTO = "auto"
        const val CAP = 50

        fun decode(text: String?): RetroLayoutChoice {
            if (text.isNullOrBlank()) return RetroLayoutChoice()
            return try {
                val o = JSONObject(text)
                val pairs = o.optJSONArray("byPath")?.let { arr ->
                    (0 until arr.length()).mapNotNull { i ->
                        val p = arr.optJSONArray(i) ?: return@mapNotNull null
                        if (p.length() < 2) null else p.getString(0) to p.getString(1)
                    }
                } ?: emptyList()
                RetroLayoutChoice(global = o.optString("global", AUTO).ifBlank { AUTO }, byPath = pairs.takeLast(CAP))
            } catch (_: Exception) {
                RetroLayoutChoice()
            }
        }
    }
}
