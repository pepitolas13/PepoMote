package dev.pepotech.pepomote.net

import android.util.Log
import dev.pepotech.pepomote.control.UpdateCheck
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

/**
 * Un HEAD a `releases/latest` de GitHub SIN seguir la redirección: la
 * etiqueta de la última versión va en la cabecera `Location`. Cualquier fallo
 * (sin red, GitHub caído) es silencioso: null y una línea en el log.
 */
object UpdateClient {
    private const val TAG = "PepoMote/update"

    suspend fun latest(): UpdateCheck.Version? = withContext(Dispatchers.IO) {
        var conn: HttpURLConnection? = null
        try {
            conn = (URL(UpdateCheck.LATEST_URL).openConnection() as HttpURLConnection).apply {
                instanceFollowRedirects = false
                requestMethod = "HEAD"
                connectTimeout = UpdateCheck.TIMEOUT_MS
                readTimeout = UpdateCheck.TIMEOUT_MS
                setRequestProperty("User-Agent", UpdateCheck.USER_AGENT)
            }
            val code = conn.responseCode
            if (code !in 300..399) {
                Log.i(TAG, "respuesta $code sin redirección")
                return@withContext null
            }
            val v = UpdateCheck.versionFromLocation(conn.getHeaderField("Location"))
            if (v == null) Log.i(TAG, "redirección sin etiqueta de versión")
            v
        } catch (e: Exception) {
            Log.i(TAG, "sin respuesta de GitHub: ${e.message}")
            null
        } finally {
            conn?.disconnect()
        }
    }
}
