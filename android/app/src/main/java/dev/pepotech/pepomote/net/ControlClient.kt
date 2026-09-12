package dev.pepotech.pepomote.net

import dev.pepotech.pepomote.control.TextInput
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
    private val callbacks: Callbacks
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
        val name: String = ""
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
        fun onPadChanged(pad: String)

        /** Aviso transitorio del receptor (banner ~6 s). */
        fun onNotice(text: String)
        fun onClosed()
    }

    @Volatile
    private var running = true
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
                    .put("name", deviceName)
                    .put("model", deviceModel)
                    // Ausente = wiimote (receptores anteriores no lo conocen)
                    .apply { if (role == "nunchuk") put("role", role) }
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
                    "ok" -> callbacks.onOk(
                        Ok(
                            sessionId = msg.getInt("session_id"),
                            udpPort = msg.optInt("udp_port", port),
                            mode = msg.optString("mode", "pointer"),
                            slot = msg.optInt("slot", 0),
                            role = msg.optString("role", role),
                            player = msg.optInt("player", 0),
                            supportsCemu = supportsCemu(msg),
                            pad = msg.optString("pad", "gamepad"),
                            name = msg.optString("name", "")
                        )
                    )

                    "err" -> {
                        callbacks.onError(msg.optString("code"), msg.optString("msg"))
                        return@Thread
                    }

                    "ping" -> sendJson(JSONObject().put("m", "pong").put("t", msg.opt("t")))
                    "pong" -> Unit
                    "mode" -> callbacks.onModeChanged(msg.optString("mode", "pointer"), msg.optString("by") == "pc")
                    "pad" -> callbacks.onPadChanged(msg.optString("pad", "gamepad"))
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

    fun sendMode(mode: String) {
        sendJson(JSONObject().put("m", "mode").put("mode", mode))
    }

    /** Modo Wii U: "wiimote" (Mando Wii) o "gamepad" (volver a GamePad/Pro). */
    fun sendPad(pad: String) {
        sendJson(JSONObject().put("m", "pad").put("pad", pad))
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
        fun supportsCemu(ok: JSONObject): Boolean {
            val modes = ok.optJSONArray("modes") ?: return false
            for (i in 0 until modes.length()) {
                if (modes.optString(i) == "cemu") return true
            }
            return false
        }
    }
}
