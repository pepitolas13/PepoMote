package dev.pepotech.pepomote.net

import dev.pepotech.pepomote.control.TextInput
import dev.pepotech.pepomote.service.PadPreference
import dev.pepotech.pepomote.service.RetroGame
import org.json.JSONObject
import java.io.BufferedReader
import java.io.BufferedWriter
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** Canal de control TCP (PROTOCOL.md §3): hello/ok/err, ping 1 Hz, mode, pad, notice. */
class ControlClient(
    private val host: String,
    private val port: Int,
    private val token: String,
    private val deviceName: String,
    private val deviceModel: String,
    /** "wiimote" (mando, el valor por defecto del receptor) o "nunchuk". */
    private val role: String,
    private val callbacks: Callbacks,
    /** Mando con su propio Nunchuk (`"nunchuk":"own"` en el hello; solo rol mando). */
    private val ownNunchuk: Boolean = false,
    /** Modo Wii U: el móvil solo hace de pantalla táctil (`"screen_only":true` en el hello; solo rol mando). */
    private val screenOnly: Boolean = false
) {
    /** Lo que confirma el receptor en el `ok`. */
    data class Ok(
        val sessionId: Int,
        val udpPort: Int,
        val mode: String,
        val slot: Int,
        /** El rol que confirma el receptor. */
        val role: String,
        /** 1..4 (0 si el ok no lo trae). */
        val player: Int,
        /** `modes` contiene "cemu": el receptor sabe de Wii U (ausente en receptores antiguos). */
        val supportsCemu: Boolean,
        /** Mando efectivo en modo Wii U: gamepad / pro / wiimote (ausente = gamepad). */
        val pad: String,
        /** Nombre del PC (`ok.name`); vacío si el receptor no lo manda. */
        val name: String = "",
        /** El receptor confirma el Nunchuk propio (`"own"`); "none" si no, o receptor antiguo. */
        val nunchuk: String = "none",
        /** El receptor conoce «solo pantalla» (`ok.screen_only`); null = receptor anterior a 1.6. */
        val screenOnly: Boolean? = null,
        val supportsSwitch: Boolean = false,
        val platform: String = ReceiverCapabilities.DESKTOP,
        val textInput: Boolean = true,
        /** Permanent credential returned after pairing with a short PC or Android code. */
        val pairToken: String? = null,
        /**
         * El receptor entiende el apuntado por inclinación (`ok.tilt`, INPUT
         * flags bit4). false = receptor anterior o servidor Android: el bit no
         * se envía nunca (lo descartarían).
         */
        val supportsTilt: Boolean = false,
        /** `ok.modes` contiene "retroarch": el receptor del PC entiende RetroArch. */
        val supportsRetroArch: Boolean = false,
        val supportsFrameRotation: Boolean = false,
        /** `ok.modes` contiene "gamepad": el receptor del PC puede hacer de mando de Xbox. */
        val supportsGamepad: Boolean = false,
        /**
         * `ok.rumble`: qué puede hacer el receptor con la vibración de los
         * juegos ("ready", "driver", "denied", "unsupported"); se guarda tal
         * cual y Ajustes lo traduce. null = receptor sin vibración.
         */
        val rumble: String? = null
    )

    interface Callbacks {
        fun onOk(ok: Ok)
        fun onError(code: String, msg: String)

        /**
         * Eco del `mode` pedido o difusión del receptor al cambiarlo otro móvil:
         * autoritativo. `byPc`: lo cambió el PC por su cuenta (modo automático
         * al abrir o cerrar Dolphin o Cemu), no fue respuesta a nadie.
         */
        fun onModeChanged(mode: String, byPc: Boolean)

        /** Eco del `pad`: mando efectivo (gamepad / pro / wiimote). */
        fun onPadChanged(pad: String, player: Int?)

        /** Eco del `nunchuk`: el receptor aplica (o no) el Nunchuk propio. */
        fun onNunchukChanged(own: Boolean)

        /** Eco del `screen_only`: el receptor aplica (o no) el modo «solo pantalla». */
        fun onScreenOnlyChanged(on: Boolean) {}
        /** RetroArch: el juego cargado según el receptor (`game`; null = ninguno). */
        fun onGameChanged(game: RetroGame?) {}

        /** Aviso transitorio del receptor (banner ~6 s). */
        fun onNotice(text: String)
        fun onClosed()
    }

    @Volatile
    private var running = true
    @Volatile
    private var confirmedMode = "pointer"
    @Volatile private var confirmedPlatform = ReceiverCapabilities.DESKTOP
    private var socket: Socket? = null
    private var writer: BufferedWriter? = null

    /**
     * TODA escritura al socket pasa por este hilo. Llamar a sendMode() desde
     * la UI escribía red en el hilo principal: Android lanza
     * NetworkOnMainThreadException y el mensaje se perdía en silencio.
     * El ejecutor único además serializa las escrituras (sin @Synchronized).
     */
    private val outbound = Executors.newSingleThreadExecutor { r ->
        Thread(r, "pepomote-control-out").apply { isDaemon = true }
    }

    private val thread = Thread({
        try {
            val s = Socket()
            s.tcpNoDelay = true
            s.connect(InetSocketAddress(host, port), 4000)
            s.soTimeout = 7000
            socket = s
            val w = BufferedWriter(OutputStreamWriter(s.getOutputStream(), Charsets.UTF_8))
            writer = w
            val r = BufferedReader(InputStreamReader(s.getInputStream(), Charsets.UTF_8))

            sendJson(
                JSONObject()
                    .put("m", "hello")
                    .put("pv", 1)
                    .put("token", token)
                    // PC receivers read `code`; Android 1.8.0 reads the same short code in `token`.
                    .apply { if (PairList.isPairingCode(token)) put("code", token) }
                    .put("name", deviceName)
                    .put("model", deviceModel)
                    // Ausente = wiimote (receptores anteriores no lo conocen)
                    .apply { if (role == "nunchuk") put("role", role) }
                    // Nunchuk en el mismo móvil: un receptor antiguo lo ignora (y no lo confirma)
                    .apply { if (role != "nunchuk" && ownNunchuk) put("nunchuk", "own") }
                    // Solo pantalla (Wii U): un receptor antiguo lo ignora (y no lo confirma)
                    .apply { if (role != "nunchuk" && screenOnly) put("screen_only", true) }
                // Sin `pad`: en Wii U se empieza siempre como GamePad/Pro (lo dice el ok)
            )

            while (running) {
                val line = r.readLine() ?: break
                if (line.isBlank()) continue
                // Una línea que no sea JSON no rompe el bucle
                val msg = try {
                    JSONObject(line)
                } catch (_: Exception) {
                    continue
                }
                when (msg.optString("m")) {
                    "ok" -> {
                        confirmedPlatform = ReceiverCapabilities.platform(msg.optString("platform"))
                        confirmedMode = msg.optString("mode", "pointer")
                        if (confirmedPlatform == ReceiverCapabilities.ANDROID) {
                            confirmedMode = ReceiverCapabilities.select(null, confirmedMode, confirmedPlatform,
                                false, supportsMode(msg, "switch"), supportsMode(msg, "retroarch"))
                        }
                        callbacks.onOk(
                            Ok(
                                sessionId = msg.getInt("session_id"),
                                udpPort = msg.optInt("udp_port", port),
                                mode = confirmedMode,
                                slot = msg.optInt("slot", 0),
                                role = msg.optString("role", role),
                                player = msg.optInt("player", 0),
                                supportsCemu = supportsMode(msg, "cemu"),
                                pad = PadPreference.effective(confirmedMode, msg.optString("pad", "gamepad")),
                                name = msg.optString("name", ""),
                                nunchuk = msg.optString("nunchuk", "none"),
                                screenOnly = if (msg.has("screen_only")) msg.optBoolean("screen_only") else null,
                                supportsSwitch = supportsMode(msg, "switch"),
                                platform = confirmedPlatform,
                                textInput = confirmedPlatform != ReceiverCapabilities.ANDROID && msg.optBoolean("text_input", true),
                                pairToken = sequenceOf(msg.optString("pair_token"), msg.optString("token"))
                                    .firstOrNull { it.isNotBlank() && it.length <= 512 },
                                supportsTilt = msg.optBoolean("tilt", false),
                                supportsFrameRotation = msg.optBoolean("frame_rotation", false),
                                supportsRetroArch = supportsMode(msg, "retroarch"),
                                supportsGamepad = supportsMode(msg, "gamepad"),
                                rumble = msg.optString("rumble", "").takeIf { it.isNotBlank() }
                            )
                        )
                    }

                    "err" -> {
                        callbacks.onError(msg.optString("code"), msg.optString("msg"))
                        return@Thread
                    }

                    "ping" -> sendJson(JSONObject().put("m", "pong").put("t", msg.opt("t")))
                    "pong" -> Unit
                    "mode" -> {
                        val received = msg.optString("mode", "pointer")
                        if (confirmedPlatform != ReceiverCapabilities.ANDROID || received in listOf("dolphin", "switch", "retroarch")) {
                            confirmedMode = received
                        }
                        callbacks.onModeChanged(confirmedMode, msg.optString("by") == "pc")
                    }
                    // Obsolete half/side fields are deliberately ignored.
                    "pad" -> callbacks.onPadChanged(PadPreference.effective(confirmedMode, msg.optString("pad", "gamepad")),
                        msg.optInt("player", 0).takeIf { it in 1..4 })
                    "nunchuk" -> callbacks.onNunchukChanged(msg.optBoolean("own", false))
                    "screen_only" -> callbacks.onScreenOnlyChanged(msg.optBoolean("on", false))
                    "game" -> callbacks.onGameChanged(RetroGame.parse(msg))
                    "hotkey" -> Unit // eco de la tecla rápida de RetroArch: informativo
                    "notice" -> msg.optString("text").takeIf { it.isNotBlank() }?.let(callbacks::onNotice)
                    else -> Unit // mensaje desconocido: se ignora
                }
            }
        } catch (e: Exception) {
            if (running) callbacks.onError("io", e.message ?: "error de conexión")
        } finally {
            running = false
            callbacks.onClosed()
        }
    }, "pepomote-control").apply { start() }

    private val pinger = Thread({
        while (running) {
            try {
                Thread.sleep(1000)
                sendJson(JSONObject().put("m", "ping").put("t", System.nanoTime() / 1000))
            } catch (_: InterruptedException) {
                break
            } catch (_: Exception) {
            }
        }
    }, "pepomote-control-ping").apply { isDaemon = true; start() }

    fun sendMode(mode: String, cemuScreenOnly: Boolean? = null) {
        // The receiver may configure Cemu immediately upon mode change, before its echo arrives.
        if (mode == "cemu" && cemuScreenOnly != null) sendScreenOnly(cemuScreenOnly)
        sendJson(JSONObject().put("m", "mode").put("mode", mode))
    }

    /** Restore preferences only after the receiver confirmed their console mode. */
    fun restoreModePreferences(mode: String, pad: String, cemuScreenOnly: Boolean, layout: String? = null) {
        // Apply this before pad: that message can also trigger Cemu profile configuration.
        if (mode == "cemu") sendScreenOnly(cemuScreenOnly)
        sendPad(PadPreference.normalize(mode, pad), layout)
    }

    /**
     * Modo Wii U: "wiimote" (Mando Wii) o "gamepad" (volver a GamePad/Pro);
     * RetroArch: `retropad`, `nes` o `gun`, con `layout` (la plantilla de
     * consola que se enseña, solo para la ventana del receptor) si se sabe.
     */
    fun sendPad(pad: String, layout: String? = null) {
        val msg = JSONObject().put("m", "pad").put("pad", PadPreference.effective(confirmedMode, pad))
        if (layout != null) msg.put("layout", layout)
        sendJson(msg)
    }

    /** Nunchuk en el mismo móvil, encendido o apagado; el receptor lo confirma con el eco. */
    fun sendNunchuk(own: Boolean) {
        sendJson(JSONObject().put("m", "nunchuk").put("own", own))
    }

    /** Modo Wii U: el móvil solo como pantalla táctil, sí o no; el receptor lo confirma con el eco. */
    fun sendScreenOnly(on: Boolean) {
        sendJson(JSONObject().put("m", "screen_only").put("on", on))
    }

    /**
     * Modo RetroArch: tecla rápida (`save_state`, `load_state`, `rewind`…;
     * PROTOCOL.md §3). Las de mantener llevan `down` true al pulsar y false
     * al soltar; las de un toque, solo true. Un receptor antiguo la ignora.
     */
    fun sendHotkey(name: String, down: Boolean) {
        sendJson(JSONObject().put("m", "hotkey").put("name", name).put("down", down))
    }

    /**
     * Modo Wii U: texto para el teclado en pantalla de Cemu (`\n` = Intro,
     * `\b` (U+0008) = borrar; ver [TextInput]). Sin respuesta; un receptor
     * antiguo lo ignora como mensaje desconocido.
     */
    fun sendText(text: String) = sendLine(TextInput.encode(text))

    private fun sendJson(obj: JSONObject) = sendLine(obj.toString())

    /** Una línea JSON al receptor (el `\n` final se pone aquí). */
    private fun sendLine(text: String) {
        try {
            outbound.execute {
                try {
                    writer?.apply {
                        write(text)
                        write("\n")
                        flush()
                    }
                } catch (_: Exception) {
                }
            }
        } catch (_: Exception) {
            // ejecutor ya cerrado
        }
    }

    fun close() {
        running = false
        sendJson(JSONObject().put("m", "bye"))
        outbound.shutdown()
        try {
            outbound.awaitTermination(300, TimeUnit.MILLISECONDS)
        } catch (_: InterruptedException) {
        }
        pinger.interrupt()
        try {
            socket?.close()
        } catch (_: Exception) {
        }
        thread.join(500)
    }

    private companion object {
        /** `ok.modes` contiene "cemu" (un receptor antiguo no manda `modes`). */
        fun supportsMode(ok: JSONObject, mode: String): Boolean {
            // El servidor Android puede anunciar Dolphin, Eden y RetroArch; nunca puntero ni Wii U.
            if (ReceiverCapabilities.platform(ok.optString("platform")) == ReceiverCapabilities.ANDROID &&
                mode !in listOf("dolphin", "switch", "retroarch")) return false
            val modes = ok.optJSONArray("modes") ?: return false
            for (i in 0 until modes.length()) {
                if (modes.optString(i) == mode) return true
            }
            return false
        }

    }
}
