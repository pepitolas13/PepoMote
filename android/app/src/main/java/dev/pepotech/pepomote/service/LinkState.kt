package dev.pepotech.pepomote.service

import android.content.Context
import android.os.SystemClock
import dev.pepotech.pepomote.sensor.MotionEngine
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
        val slot: Int = 0,
        /** "wiimote" o "nunchuk" (lo que confirmó el receptor en el ok). */
        val role: String = LinkState.ROLE_WIIMOTE,
        /** Jugador (1..4) al que pertenece este móvil; un Nunchuk va con el Wiimote de ese jugador. */
        val player: Int = slot + 1,
        /** El receptor sabe de Wii U (`ok.modes` trae "cemu"); false en receptores antiguos. */
        val supportsCemu: Boolean = false,
        /**
         * Mando efectivo en modo Wii U: "gamepad" (jugador 1), "pro" (2-4) o
         * "wiimote" (a petición, solo durante esta sesión). Lo fija el ok y lo
         * cambia el eco de `pad`.
         */
        val pad: String = LinkState.PAD_GAMEPAD
    ) : UiLink()

    data class Failed(val code: String, val msg: String) : UiLink()

    /**
     * La sesión se cayó y el servicio la está rehaciendo solo (intento
     * `attempt`, espera creciente, dos minutos como mucho). Las pantallas del
     * mando se quedan y lo pedido (Wii U, Mando de Wii) se repone al volver.
     */
    data class Reconnecting(val pcName: String, val attempt: Int) : UiLink()
}

/** Hay enlace (vivo, arrancando o rehaciéndose): el mando tiene sentido en pantalla. */
val UiLink.alive: Boolean
    get() = this is UiLink.Connected || this is UiLink.Connecting || this is UiLink.Reconnecting

/** Aviso transitorio (del receptor o propio): la UI lo enseña ~6 s desde [atMs] (elapsedRealtime). */
data class Notice(val text: String, val atMs: Long)

/** Estado observable del enlace, publicado por LinkForegroundService. */
object LinkState {
    const val ROLE_WIIMOTE = "wiimote"
    const val ROLE_NUNCHUK = "nunchuk"

    const val MODE_POINTER = "pointer"
    const val MODE_DOLPHIN = "dolphin"
    const val MODE_CEMU = "cemu"

    const val PAD_GAMEPAD = "gamepad"
    const val PAD_PRO = "pro"
    const val PAD_WIIMOTE = "wiimote"

    private val _flow = MutableStateFlow<UiLink>(UiLink.Disconnected)
    val flow: StateFlow<UiLink> = _flow

    private val _notice = MutableStateFlow<Notice?>(null)
    val notice: StateFlow<Notice?> = _notice

    /** Intención pendiente del usuario (ver [Route]). */
    private val _intent = MutableStateFlow(PadIntent.None)
    val intent: StateFlow<PadIntent> = _intent

    /**
     * Rol del enlace vivo (o arrancando). Lo fija el servicio al arrancar; la
     * UI lo consulta para saber si hay que rehacer el enlace con otro rol.
     */
    @Volatile
    var role: String = ROLE_WIIMOTE
        internal set

    /** Cambia el modo pointer/dolphin/cemu; lo conecta el servicio al ControlClient. */
    @Volatile
    var sendMode: ((String) -> Unit)? = null

    /** Modo a aplicar en cuanto se complete la próxima conexión. */
    @Volatile
    var pendingMode: String? = null

    /** Modo Wii U: elegir "wiimote" (Mando de Wii) o "gamepad" (volver a GamePad/Pro). */
    @Volatile
    var sendPad: ((String) -> Unit)? = null

    /**
     * Modo Wii U: texto para el teclado en pantalla de Cemu (`\n` = Intro,
     * `\b` (U+0008) = borrar; ver [dev.pepotech.pepomote.control.TextInput]).
     */
    @Volatile
    var sendText: ((String) -> Unit)? = null

    /**
     * Motor de sensores del enlace vivo: la pantalla GamePad le fija
     * `kind`/`rotation` al entrar y los restaura al salir.
     */
    @Volatile
    var motion: MotionEngine? = null
        internal set

    /**
     * Pide un modo al receptor. `cemu` deja la intención Wii U (la pantalla
     * GamePad aparece al instante, optimista, y el eco la confirma o la
     * deshace); cualquier otro modo la quita. Si el enlace aún no está, el
     * modo se aplica en cuanto llegue el `ok`.
     */
    fun requestMode(mode: String) {
        _intent.value = if (mode == MODE_CEMU) PadIntent.WiiU else PadIntent.None
        val send = sendMode
        if (send != null && _flow.value is UiLink.Connected) send(mode) else pendingMode = mode
    }

    /** Salir / desconectar: no queda nada pendiente. */
    fun clearIntent() {
        _intent.value = PadIntent.None
    }

    /** Eco o difusión de `mode`: consume la intención Wii U y avisa si no era cemu (salvo si lo decidió el PC). */
    internal fun resolveIntent(context: Context, mode: String, byPc: Boolean = false) {
        val out = Route.afterModeEcho(_intent.value, mode, _flow.value, byPc)
        _intent.value = out.intent
        out.warning?.let { publishNotice(context.getString(it)) }
    }

    internal fun publish(state: UiLink) {
        _flow.value = state
        // Sin enlace no hay intención que mantener
        if (state is UiLink.Disconnected || state is UiLink.Failed) _intent.value = PadIntent.None
    }

    internal fun updateConnected(transform: (UiLink.Connected) -> UiLink.Connected) {
        val cur = _flow.value
        if (cur is UiLink.Connected) _flow.value = transform(cur)
    }

    internal fun publishNotice(text: String) {
        _notice.value = Notice(text, SystemClock.elapsedRealtime())
    }

    /** Error ya mostrado: vuelve a Desconectado para que no se re-dispare. */
    internal fun clearFailure() {
        if (_flow.value is UiLink.Failed) _flow.value = UiLink.Disconnected
    }
}
