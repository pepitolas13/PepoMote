package dev.pepotech.pepomote.service

import android.graphics.Bitmap
import dev.pepotech.pepomote.net.BitmapDecoder
import dev.pepotech.pepomote.net.ScreenClient
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Canal de pantalla del GamePad (doble pantalla): una sola instancia viva de
 * [ScreenClient], y solo mientras coinciden tres cosas:
 * - hay enlace: el servicio lo enlaza con host, puerto y sesión del `ok` y
 *   lo desenlaza al cerrar el enlace;
 * - la pantalla GamePad la pide, con el tamaño de su zona táctil en píxeles
 *   (y la retira al salir, o si el móvil pasa a Pro / Mando de Wii);
 * - la app está en primer plano (la actividad, entre ON_START y ON_STOP).
 *
 * Cada cambio recalcula: abrir y cerrar son idempotentes, y un tamaño o un
 * enlace nuevos con el canal abierto lo rehacen. Sin hilos ni sockets
 * huérfanos: el cliente anterior se cierra antes de crear otro.
 */
object ScreenLink {
    private data class Endpoint(val host: String, val port: Int, val sessionId: Int)
    private data class Request(val width: Int, val height: Int)

    private val _client = MutableStateFlow<ScreenClient<Bitmap>?>(null)

    /** Cliente vivo (la pantalla GamePad pinta sus flujos), o null. */
    val client: StateFlow<ScreenClient<Bitmap>?> = _client

    private var endpoint: Endpoint? = null
    private var request: Request? = null
    private var foreground = false
    private var live: Pair<Endpoint, Request>? = null

    /** El servicio, con el `ok` de un mando (un Nunchuk no tiene pantalla). */
    @Synchronized
    fun bind(host: String, port: Int, sessionId: Int) {
        endpoint = Endpoint(host, port, sessionId)
        reconcile()
    }

    /** El servicio, al cerrar el enlace. */
    @Synchronized
    fun unbind() {
        endpoint = null
        reconcile()
    }

    /** La pantalla GamePad operativa y como GamePad: tamaño máximo que quiere (píxeles). */
    @Synchronized
    fun request(width: Int, height: Int) {
        request = Request(width, height)
        reconcile()
    }

    /** La pantalla GamePad se va (o deja de ser GamePad). */
    @Synchronized
    fun release() {
        request = null
        reconcile()
    }

    /** La actividad: ON_START → true, ON_STOP → false. */
    @Synchronized
    fun setForeground(value: Boolean) {
        foreground = value
        reconcile()
    }

    private fun reconcile() {
        val ep = endpoint
        val req = request
        val wanted = if (ep != null && req != null && foreground) ep to req else null
        if (wanted == live) return
        _client.value?.close()
        _client.value = null
        live = wanted
        if (wanted != null) {
            _client.value = ScreenClient(
                wanted.first.host, wanted.first.port, wanted.first.sessionId,
                wanted.second.width, wanted.second.height, BitmapDecoder()
            ).also { it.start() }
        }
    }
}
