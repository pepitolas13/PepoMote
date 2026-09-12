package dev.pepotech.pepomote.net

import android.content.Context
import android.content.SharedPreferences
import android.net.Uri

data class Pairing(
    val host: String,
    val port: Int,
    val token: String,
    val pcName: String
)

/**
 * Los PCs emparejados (varios) y cuál es el actual. Un token es un PC. Las
 * claves planas (host/port/token/pcName) de versiones anteriores se migran a
 * la lista y se siguen escribiendo como espejo del actual.
 */
object PairStore {
    private const val PREFS = "pairing"
    private const val KEY_LIST = "list"
    private const val KEY_CURRENT = "current"

    private fun prefs(context: Context): SharedPreferences =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    /** Todos los PCs guardados, en el orden en que se emparejaron. */
    fun all(context: Context): List<Pairing> {
        val sp = prefs(context)
        if (sp.contains(KEY_LIST)) return PairList.decode(sp.getString(KEY_LIST, ""))
        val flat = loadFlat(sp) ?: return emptyList()
        sp.edit().putString(KEY_LIST, PairList.encode(listOf(flat))).putString(KEY_CURRENT, flat.token).apply()
        return listOf(flat)
    }

    /** El PC actual (con el que se conecta); null sin ninguno. */
    fun current(context: Context): Pairing? {
        val list = all(context)
        val token = prefs(context).getString(KEY_CURRENT, null)
        return list.firstOrNull { it.token == token } ?: list.firstOrNull()
    }

    fun load(context: Context): Pairing? = current(context)

    /** Guarda (por token) y lo deja como actual. */
    fun save(context: Context, p: Pairing) = upsert(context, p)

    fun upsert(context: Context, p: Pairing) {
        write(context, PairList.upsert(all(context), p), p.token)
    }

    /** Otro de los guardados pasa a ser el actual. */
    fun select(context: Context, token: String) {
        val list = all(context)
        if (list.any { it.token == token }) write(context, list, token)
    }

    fun forget(context: Context, token: String) {
        val current = prefs(context).getString(KEY_CURRENT, null)
        val list = PairList.forget(all(context), token)
        write(context, list, PairList.nextCurrent(list, current, token))
    }

    private fun write(context: Context, list: List<Pairing>, current: String?) {
        val e = prefs(context).edit().putString(KEY_LIST, PairList.encode(list))
        if (current != null) e.putString(KEY_CURRENT, current) else e.remove(KEY_CURRENT)
        // espejo plano del actual (así lo leían las versiones anteriores)
        val cur = list.firstOrNull { it.token == current }
        if (cur != null) {
            e.putString("host", cur.host).putInt("port", cur.port).putString("token", cur.token).putString("pcName", cur.pcName)
        } else {
            e.remove("host").remove("port").remove("token").remove("pcName")
        }
        e.apply()
    }

    private fun loadFlat(sp: SharedPreferences): Pairing? {
        val host = sp.getString("host", null) ?: return null
        val token = sp.getString("token", null) ?: return null
        return Pairing(host, sp.getInt("port", 26761), token, sp.getString("pcName", "PC") ?: "PC")
    }

    /** Parsea pepomote://pair?v=1&host=..&port=..&t=..&name=.. */
    fun parsePairUrl(url: String): Pairing? {
        return try {
            val uri = Uri.parse(url)
            if (uri.scheme != "pepomote" || uri.host != "pair") return null
            if (uri.getQueryParameter("v") != "1") return null
            val host = uri.getQueryParameter("host") ?: return null
            val token = uri.getQueryParameter("t") ?: return null
            val port = uri.getQueryParameter("port")?.toIntOrNull() ?: 26761
            val name = uri.getQueryParameter("name") ?: "PC"
            Pairing(host, port, token, name)
        } catch (_: Exception) {
            null
        }
    }
}
