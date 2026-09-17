package dev.pepotech.pepomote.service

import android.content.Context
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.control.RetroLayouts
import android.os.SystemClock
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.sensor.MotionEngine
import dev.pepotech.pepomote.net.ReceiverCapabilities
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
        val pad: String = LinkState.PAD_GAMEPAD,
        /**
         * El receptor confirmó el Nunchuk en el mismo móvil (`ok.nunchuk` o
         * eco de `nunchuk`): en Dolphin, apaisado = mando + Nunchuk.
         */
        val ownNunchuk: Boolean = false,
        /**
         * El receptor confirmó «solo pantalla» (`ok.screen_only` o eco de
         * `screen_only`); null = receptor anterior a 1.6, que no lo conoce.
         */
        val screenOnly: Boolean? = null,
        /** `ok.modes` contains "switch"; absent means an older receiver. */
        val supportsSwitch: Boolean = false,
        val platform: String = ReceiverCapabilities.DESKTOP,
        val textInput: Boolean = true,
        /** El receptor entiende el apuntado por inclinación (`ok.tilt`); false en receptores anteriores. */
        val supportsTilt: Boolean = false,
        /** `ok.modes` contains "retroarch" (PC receiver with the RetroArch network gamepad). */
        val supportsRetroArch: Boolean = false,
        /**
         * RetroArch: el juego cargado según el receptor (mensaje `game`); con
         * su consola se elige la plantilla de mando. Se conserva al cambiar de
         * modo (solo cuenta en RetroArch).
         */
        val game: RetroGame? = null
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
    const val MODE_SWITCH = "switch"
    const val MODE_RETROARCH = "retroarch"

    const val PAD_GAMEPAD = "gamepad"
    const val PAD_PRO = "pro"
    const val PAD_WIIMOTE = "wiimote"
    /** Modo RetroArch: mando apaisado de dos sticks, mando de NES (de lado) o pistola de luz. */
    const val PAD_RETROPAD = "retropad"
    const val PAD_NES = "nes"
    const val PAD_GUN = "gun"

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

    /** Nunchuk en el mismo móvil (modo Dolphin): pedirlo o quitarlo; el eco lo confirma. */
    @Volatile
    var sendNunchuk: ((Boolean) -> Unit)? = null

    /** Modo Wii U: el móvil solo como pantalla táctil (pantalla completa); el eco lo confirma. */
    @Volatile
    var sendScreenOnly: ((Boolean) -> Unit)? = null

    /**
     * RetroArch: la plantilla de consola efectiva cambió (juego nuevo, eco de
     * `pad` o elección a mano): el servicio la manda al receptor (`pad.layout`).
     */
    @Volatile
    var sendLayout: (() -> Unit)? = null

    /** RetroArch: mando de consola elegido a mano (preferencias); lo carga el servicio al conectar. */
    private val _retroLayoutChoice = MutableStateFlow(RetroLayoutChoice())
    val retroLayoutChoice: StateFlow<RetroLayoutChoice> = _retroLayoutChoice

    internal fun loadRetroLayouts(context: Context) {
        _retroLayoutChoice.value = AppPrefs.retroLayoutChoice(context)
    }

    /** Elegir a mano (null = automático) para el juego `path`; se guarda y se avisa al receptor. */
    fun pickRetroLayout(context: Context, path: String?, id: String?) {
        val next = _retroLayoutChoice.value.pick(path, id)
        _retroLayoutChoice.value = next
        AppPrefs.setRetroLayoutChoice(context, next)
        sendLayout?.invoke()
    }

    /**
     * Plantilla que tocaría como RetroPad en esta sesión: la elegida a mano
     * para el juego (o la global), si no la consola que anunció el PC, si no
     * el RetroPad completo.
     */
    fun wouldBeRetroLayout(link: UiLink.Connected?, choice: RetroLayoutChoice = _retroLayoutChoice.value): String {
        val game = link?.game
        return RetroLayouts.effective(game?.console, choice.choiceFor(game?.path?.takeIf { it.isNotEmpty() }))
    }

    /** La que se enseña: solo en RetroArch y siendo el RetroPad; si no, el RetroPad completo. */
    fun effectiveRetroLayout(link: UiLink.Connected?, choice: RetroLayoutChoice = _retroLayoutChoice.value): String =
        if (link?.mode == MODE_RETROARCH && link.pad == PAD_RETROPAD) wouldBeRetroLayout(link, choice) else RetroLayouts.RETROPAD.id

    /**
     * Modo RetroArch: tecla rápida por su nombre del protocolo (`save_state`,
     * `rewind`…) y si se pulsa (true) o se suelta (false); las de un toque
     * solo necesitan la pulsación.
     */
    @Volatile
    var sendHotkey: ((String, Boolean) -> Unit)? = null

    /**
     * Sensor del puntero cambiado en Ajustes con el enlace vivo: true =
     * acelerómetro (inclinación). El servicio lo cruza con lo que el
     * receptor anunció en su `ok` antes de ponérselo al motor.
     */
    @Volatile
    var setTilt: ((Boolean) -> Unit)? = null

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
        val connected = _flow.value as? UiLink.Connected
        val selected = connected?.let { Route.selectMode(mode, it) } ?: mode
        ButtonState.reset()
        _intent.value = when (selected) {
            MODE_CEMU -> PadIntent.WiiU
            MODE_SWITCH -> PadIntent.Switch
            MODE_RETROARCH -> PadIntent.RetroArch
            else -> PadIntent.None
        }
        val send = sendMode
        if (send != null && _flow.value is UiLink.Connected) send(selected) else pendingMode = selected
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
