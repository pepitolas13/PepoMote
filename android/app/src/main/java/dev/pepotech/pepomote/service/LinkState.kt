package dev.pepotech.pepomote.service

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

sealed class UiLink {
    data object Disconnected : UiLink()
    data object Connecting : UiLink()
    data class Connected(
        val pcName: String,
        val mode: String,
        val rttMs: Float?,
        val sensorHz: Float,
        /** 0 = Jugador 1 (controla puntero y modo); 1..3 = jugadores extra */
        val slot: Int = 0
    ) : UiLink()

    data class Failed(val code: String, val msg: String) : UiLink()
}

/** Estado observable del enlace, publicado por LinkForegroundService. */
object LinkState {
    private val _flow = MutableStateFlow<UiLink>(UiLink.Disconnected)
    val flow: StateFlow<UiLink> = _flow

    /** Cambia el modo pointer/dolphin; lo conecta el servicio al ControlClient. */
    @Volatile
    var sendMode: ((String) -> Unit)? = null

    /** Modo a aplicar en cuanto se complete la próxima conexión. */
    @Volatile
    var pendingMode: String? = null

    internal fun publish(state: UiLink) {
        _flow.value = state
    }

    internal fun updateConnected(transform: (UiLink.Connected) -> UiLink.Connected) {
        val cur = _flow.value
        if (cur is UiLink.Connected) _flow.value = transform(cur)
    }

    /** Error ya mostrado: vuelve a Desconectado para que no se re-dispare. */
    internal fun clearFailure() {
        if (_flow.value is UiLink.Failed) _flow.value = UiLink.Disconnected
    }
}
