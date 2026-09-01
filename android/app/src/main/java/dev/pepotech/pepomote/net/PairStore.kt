package dev.pepotech.pepomote.net

import android.content.Context
import android.net.Uri

data class Pairing(
    val host: String,
    val port: Int,
    val token: String,
    val pcName: String
)

object PairStore {
    private const val PREFS = "pairing"

    fun save(context: Context, p: Pairing) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putString("host", p.host)
            .putInt("port", p.port)
            .putString("token", p.token)
            .putString("pcName", p.pcName)
            .apply()
    }

    fun load(context: Context): Pairing? {
        val sp = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
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
