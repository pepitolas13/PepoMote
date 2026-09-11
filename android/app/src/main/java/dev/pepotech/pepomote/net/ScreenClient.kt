package dev.pepotech.pepomote.net

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.io.InputStream
import java.io.OutputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.roundToInt

/** Decodifica la carga JPEG de una trama de imagen. En la app, [BitmapDecoder]. */
interface ScreenDecoder<I : Any> {
    /**
     * Imagen decodificada de `data[off, off + len)`, o null si no vale (se
     * confirma igualmente al receptor para no atascar el flujo).
     */
    fun decode(data: ByteArray, off: Int, len: Int): I?
}

/**
 * Canal de pantalla del GamePad (PROTOCOL.md §4.4): conexión TCP propia al
 * puerto PMP del receptor, línea `screen` con la sesión y el tamaño máximo
 * que cabe en la zona táctil, y de vuelta tramas [ScreenFrames] con la
 * pantalla del GamePad de Cemu en JPEG. Todo en un hilo propio: ahí se
 * decodifica (reutilizando bitmaps) y se publica en [image]; la UI avisa con
 * [shown] al pintarla y entonces sale el byte de confirmación (una imagen en
 * vuelo: la latencia no se acumula). Si nadie la pinta en [shownFallbackMs],
 * se confirma igualmente. El texto de estado del receptor (tipo 2) va en
 * [status] y deja de haber imagen; el keepalive (tipo 3) solo cuenta como
 * dato recibido.
 *
 * - Sin respuesta al `screen` en 3 s (o cierre sin contestar, o `err`):
 *   «sin pantalla» (receptor sin canal de pantalla); reintento cada 5 s.
 * - Sin datos en 6 s tras el ok: conexión muerta; se cierra y se reconecta
 *   al segundo, y si tampoco, cada 5 s.
 * - [close] es definitivo: nada vuelve a abrirse ni a publicarse.
 */
class ScreenClient<I : Any>(
    private val host: String,
    private val port: Int,
    private val sessionId: Int,
    private val maxWidth: Int,
    private val maxHeight: Int,
    private val decoder: ScreenDecoder<I>,
    private val quality: Int = QUALITY,
    private val replyTimeoutMs: Int = REPLY_TIMEOUT_MS,
    private val dataTimeoutMs: Int = DATA_TIMEOUT_MS,
    private val shownFallbackMs: Long = SHOWN_FALLBACK_MS,
    private val retryAfterDropMs: Long = RETRY_AFTER_DROP_MS,
    private val retryUnavailableMs: Long = RETRY_UNAVAILABLE_MS
) {
    companion object {
        /** Resolución nativa de la pantalla del GamePad: nunca se pide más. */
        const val NATIVE_WIDTH = 854
        const val NATIVE_HEIGHT = 480
        const val QUALITY = 70

        const val CONNECT_TIMEOUT_MS = 4000
        const val REPLY_TIMEOUT_MS = 3000
        const val DATA_TIMEOUT_MS = 6000

        /** Si la UI no avisa de que pintó la imagen en este tiempo, se confirma igualmente. */
        const val SHOWN_FALLBACK_MS = 200L
        const val RETRY_AFTER_DROP_MS = 1000L
        const val RETRY_UNAVAILABLE_MS = 5000L

        /** La respuesta es una línea corta; más que esto sin `\n` no es un receptor. */
        private const val MAX_LINE = 4096

        const val STATUS_UNAVAILABLE = "Sin pantalla (el PC no la envía)"
        const val STATUS_RECONNECTING = "Reconectando la pantalla…"

        /** Línea `screen` (PROTOCOL.md §4.4); `session_id` va como u32. */
        internal fun helloLine(sessionId: Int, w: Int, h: Int, q: Int): String =
            "{\"m\":\"screen\",\"session_id\":${sessionId.toLong() and 0xFFFF_FFFFL},\"w\":$w,\"h\":$h,\"q\":$q}"

        /** `{"m":"screen","ok":true}`: el receptor acepta. */
        internal fun replyOk(line: String): Boolean =
            jsonString(line, "m") == "screen" && Regex("\"ok\"\\s*:\\s*true").containsMatchIn(line)

        /**
         * Valor de una clave string de la línea JSON de respuesta (solo tiene
         * `m`, `ok`, `code` y `msg`). Sin org.json: el de Android no existe
         * en la JVM de los tests, y para esto no hace falta más.
         */
        internal fun jsonString(line: String, key: String): String? {
            val m = Regex("\"" + Regex.escape(key) + "\"\\s*:\\s*\"((?:[^\"\\\\]|\\\\.)*)\"").find(line)
                ?: return null
            val raw = m.groupValues[1]
            if ('\\' !in raw) return raw
            val sb = StringBuilder(raw.length)
            var i = 0
            while (i < raw.length) {
                val c = raw[i]
                if (c != '\\' || i + 1 >= raw.length) {
                    sb.append(c)
                    i++
                    continue
                }
                when (val e = raw[i + 1]) {
                    'n' -> sb.append('\n')
                    't' -> sb.append('\t')
                    'r' -> sb.append('\r')
                    'u' -> {
                        val code = if (i + 5 < raw.length) raw.substring(i + 2, i + 6).toIntOrNull(16) else null
                        if (code != null) {
                            sb.append(code.toChar())
                            i += 4
                        }
                    }

                    else -> sb.append(e) // \" \\ \/
                }
                i += 2
            }
            return sb.toString()
        }
    }

    /** Imagen decodificada lista para pintar; [seq] la identifica (el bitmap de debajo se reutiliza). */
    class Image<I>(val seq: Int, val bitmap: I)

    private val _image = MutableStateFlow<Image<I>?>(null)
    val image: StateFlow<Image<I>?> = _image

    /** Última línea de estado (del receptor, o propia: sin pantalla / reconectando). */
    private val _status = MutableStateFlow<String?>(null)
    val status: StateFlow<String?> = _status

    /** Imágenes por segundo (media de 1 s); 0 sin imagen. */
    private val _fps = MutableStateFlow(0)
    val fps: StateFlow<Int> = _fps

    @Volatile
    private var running = true

    @Volatile
    private var socket: Socket? = null

    @Volatile
    private var out: OutputStream? = null

    /** Generación de la conexión: una confirmación de la anterior no se escribe en la nueva. */
    @Volatile
    private var conn = 0

    /** `seq` de la imagen publicada y aún sin confirmar (0 = ninguna). */
    private val pendingAck = AtomicInteger(0)
    private var seq = 0
    private val frames = ScreenFrames()
    private var fpsWindowStartNs = 0L
    private var fpsCount = 0

    /** TODA escritura al socket (las confirmaciones) pasa por este hilo. */
    private val outbound: ScheduledExecutorService = Executors.newSingleThreadScheduledExecutor { r ->
        Thread(r, "pepomote-screen-out").apply { isDaemon = true }
    }

    private val thread = Thread(::run, "pepomote-screen")

    fun start() {
        thread.start()
    }

    /** La UI acaba de pintar la imagen [seq]: ahora sí, confirmación al receptor. */
    fun shown(seq: Int) {
        if (pendingAck.compareAndSet(seq, 0)) ack(conn)
    }

    fun close() {
        running = false
        outbound.shutdownNow()
        try {
            socket?.close()
        } catch (_: Exception) {
        }
        thread.interrupt()
        _image.value = null
        _status.value = null
        _fps.value = 0
    }

    private fun run() {
        try {
            while (running) {
                val gotOk = session()
                if (!running) break
                Thread.sleep(if (gotOk) retryAfterDropMs else retryUnavailableMs)
            }
        } catch (_: InterruptedException) {
        }
    }

    /** Una conexión, hasta que se cierra. Devuelve si el receptor llegó a contestar ok. */
    private fun session(): Boolean {
        var gotOk = false
        val s = Socket()
        socket = s
        try {
            if (!running) return false
            s.tcpNoDelay = true
            s.connect(InetSocketAddress(host, port), CONNECT_TIMEOUT_MS)
            s.soTimeout = replyTimeoutMs
            val input = s.getInputStream()
            val output = s.getOutputStream()
            output.write((helloLine(sessionId, maxWidth, maxHeight, quality) + "\n").toByteArray(Charsets.UTF_8))
            output.flush()
            frames.reset()

            // Respuesta: una línea JSON. Un receptor sin pantalla cierra o calla
            // (soTimeout) y un `err` trae el motivo.
            val line = readReply(input) ?: run {
                unavailable(null)
                return false
            }
            if (!replyOk(line)) {
                unavailable(jsonString(line, "msg"))
                return false
            }
            gotOk = true
            conn++
            pendingAck.set(0)
            out = output
            s.soTimeout = dataTimeoutMs
            _status.value = null

            while (running) {
                val f = frames.next()
                if (f == null) {
                    if (frames.fill(input) < 0) break
                    continue
                }
                when (f.type) {
                    ScreenFrames.TYPE_IMAGE -> onImage(f)
                    ScreenFrames.TYPE_STATUS -> onStatus(String(f.data, f.offset, f.length, Charsets.UTF_8))
                    else -> Unit // keepalive (ya contó como dato al llegar) o tipo futuro
                }
            }
        } catch (_: Exception) {
            // Sin respuesta en 3 s, sin datos en 6 s, conexión rechazada o
            // cerrada: según la fase, «sin pantalla» o «reconectando»
            if (!gotOk) unavailable(null)
        } finally {
            out = null
            try {
                s.close()
            } catch (_: Exception) {
            }
            if (gotOk) dropped()
        }
        return gotOk
    }

    /** Línea de respuesta al `screen`, o null si el receptor cierra o no manda una línea. */
    private fun readReply(input: InputStream): String? {
        while (true) {
            frames.takeLine()?.let { return it }
            if (frames.pending > MAX_LINE || frames.fill(input) < 0) return null
        }
    }

    private fun onImage(f: ScreenFrames.Frame) {
        val bitmap = try {
            decoder.decode(f.data, f.offset, f.length)
        } catch (_: Exception) {
            null
        }
        if (!running) return
        val c = conn
        if (bitmap == null) {
            ack(c) // nada que enseñar: se confirma igual para que siga el flujo
            return
        }
        val n = ++seq
        pendingAck.set(n)
        countImage()
        _image.value = Image(n, bitmap)
        // Si la UI no llega a pintarla, que no se atasque el flujo
        try {
            outbound.schedule(
                Runnable { if (pendingAck.compareAndSet(n, 0)) writeAck(c) },
                shownFallbackMs, TimeUnit.MILLISECONDS
            )
        } catch (_: Exception) {
            // ejecutor ya cerrado
        }
    }

    /** Estado del receptor: solo lo manda cuando no tiene imagen que dar. */
    private fun onStatus(text: String) {
        _image.value = null
        resetFps()
        _status.value = text
    }

    private fun unavailable(msg: String?) {
        if (!running) return
        _image.value = null
        resetFps()
        _status.value = if (msg.isNullOrBlank()) STATUS_UNAVAILABLE else "Sin pantalla: $msg"
    }

    private fun dropped() {
        if (!running) return
        _image.value = null
        resetFps()
        _status.value = STATUS_RECONNECTING
    }

    private fun ack(c: Int) {
        try {
            outbound.execute { writeAck(c) }
        } catch (_: Exception) {
            // ejecutor ya cerrado
        }
    }

    /** Solo en el hilo outbound. */
    private fun writeAck(c: Int) {
        if (c != conn) return
        try {
            out?.apply {
                write(ScreenFrames.ACK.toInt())
                flush()
            }
        } catch (_: Exception) {
        }
    }

    private fun countImage() {
        val now = System.nanoTime()
        if (fpsWindowStartNs == 0L) fpsWindowStartNs = now
        fpsCount++
        val elapsed = now - fpsWindowStartNs
        if (elapsed >= 1_000_000_000L) {
            _fps.value = (fpsCount * 1e9 / elapsed).roundToInt()
            fpsWindowStartNs = now
            fpsCount = 0
        }
    }

    private fun resetFps() {
        fpsWindowStartNs = 0L
        fpsCount = 0
        _fps.value = 0
    }
}
