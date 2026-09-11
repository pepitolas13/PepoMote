package dev.pepotech.pepomote.net

import java.io.InputStream

/**
 * Tramas del canal de pantalla del GamePad (PROTOCOL.md §4.4): tras la línea
 * de respuesta del receptor todo son tramas binarias
 * `"PMPS"` · tipo u8 · longitud u32 LE · carga. Acumula lo que llega por el
 * socket (trozos de cualquier tamaño) y devuelve tramas completas de una en
 * una; si el magic no cuadra (o la longitud es absurda) descarta bytes hasta
 * el siguiente candidato y sigue (resync). Puro, sin Android, y sin reservar
 * memoria por trama: un único buffer que crece hasta la trama más grande
 * vista y se reutiliza. Testeado en ScreenFramesTest.
 */
class ScreenFrames(private val maxPayload: Int = MAX_PAYLOAD) {

    companion object {
        /** tipo 1: imagen JPEG completa (baseline). */
        const val TYPE_IMAGE = 1

        /** tipo 2: estado en texto UTF-8, para enseñar mientras no hay imagen. */
        const val TYPE_STATUS = 2

        /** tipo 3: keepalive sin carga (cada 2 s si no hay otra cosa que mandar). */
        const val TYPE_KEEPALIVE = 3

        /** magic (4) + tipo (1) + longitud (4). */
        const val HEADER_LEN = 9

        /** Confirmación del móvil: un byte tras decodificar y pintar cada imagen. */
        const val ACK: Byte = 0x01

        /** Tope de carga: una imagen 854×480 en JPEG anda por 30-100 KB; mucho más es basura. */
        const val MAX_PAYLOAD = 4 shl 20

        /** Cuánto se intenta leer del socket de una vez. */
        private const val CHUNK = 64 shl 10
        private const val INITIAL = 256 shl 10
        private val MAGIC = "PMPS".toByteArray(Charsets.US_ASCII)
        private val EMPTY = ByteArray(0)
    }

    /**
     * Trama completa. La carga son los [length] bytes de [data] a partir de
     * [offset]: es el buffer interno (no se copia nada), válido solo hasta la
     * siguiente llamada a [next], [push] o [fill].
     */
    class Frame internal constructor() {
        var type = 0
            internal set
        var data: ByteArray = EMPTY
            internal set
        var offset = 0
            internal set
        var length = 0
            internal set

        /** Copia de la carga (textos, tests). */
        fun payload(): ByteArray = data.copyOfRange(offset, offset + length)
    }

    private var buf = ByteArray(INITIAL)

    /** Bytes válidos en [buf]. */
    private var count = 0

    /** Inicio de lo aún no consumido. */
    private var pos = 0
    private val frame = Frame()
    private var synced = true

    /** Veces que se perdió la sincronía (bytes inesperados donde tocaba un magic). */
    var resyncs = 0
        private set

    /** Bytes descartados en total por esas resincronizaciones. */
    var dropped = 0L
        private set

    /** Bytes acumulados aún sin consumir. */
    val pending: Int
        get() = count - pos

    /** Añade bytes recibidos. */
    fun push(src: ByteArray, off: Int = 0, len: Int = src.size - off) {
        ensureRoom(len)
        System.arraycopy(src, off, buf, count, len)
        count += len
    }

    /**
     * Lee del socket lo que haya (bloquea hasta que llegue algo) directamente
     * al buffer. Devuelve los bytes leídos, o -1 si el otro lado cerró.
     */
    fun fill(input: InputStream): Int {
        ensureRoom(CHUNK)
        val n = input.read(buf, count, buf.size - count)
        if (n > 0) count += n
        return n
    }

    /**
     * Línea de texto completa (hasta `\n`, sin él ni un `\r` final), o null
     * si aún no ha llegado entera. Para la respuesta al `screen`; lo que
     * venga detrás en el mismo trozo queda para [next].
     */
    fun takeLine(): String? {
        for (i in pos until count) {
            if (buf[i] != '\n'.code.toByte()) continue
            var end = i
            if (end > pos && buf[end - 1] == '\r'.code.toByte()) end--
            val line = String(buf, pos, end - pos, Charsets.UTF_8)
            pos = i + 1
            return line
        }
        return null
    }

    /** Siguiente trama completa, o null si faltan bytes (toca [fill]/[push]). */
    fun next(): Frame? {
        while (true) {
            if (count - pos < HEADER_LEN) return null
            if (!magicAt(pos)) {
                resync()
                continue
            }
            val type = buf[pos + 4].toInt() and 0xFF
            val length = readU32(pos + 5)
            if (length > maxPayload) {
                // Un "PMPS" casual dentro de basura: seguir buscando
                resync()
                continue
            }
            val len = length.toInt()
            if (count - pos - HEADER_LEN < len) return null
            frame.type = type
            frame.data = buf
            frame.offset = pos + HEADER_LEN
            frame.length = len
            pos += HEADER_LEN + len
            synced = true
            return frame
        }
    }

    /** Vacía lo acumulado (conexión nueva). */
    fun reset() {
        count = 0
        pos = 0
        synced = true
    }

    private fun magicAt(i: Int): Boolean =
        buf[i] == MAGIC[0] && buf[i + 1] == MAGIC[1] && buf[i + 2] == MAGIC[2] && buf[i + 3] == MAGIC[3]

    private fun readU32(i: Int): Long =
        (buf[i].toLong() and 0xFF) or
            ((buf[i + 1].toLong() and 0xFF) shl 8) or
            ((buf[i + 2].toLong() and 0xFF) shl 16) or
            ((buf[i + 3].toLong() and 0xFF) shl 24)

    /** Salta hasta la siguiente 'P' después de [pos] (o hasta el final si no hay). */
    private fun resync() {
        if (synced) {
            synced = false
            resyncs++
        }
        var i = pos + 1
        while (i < count && buf[i] != MAGIC[0]) i++
        dropped += i - pos
        pos = i
    }

    /** Sitio para [n] bytes más al final: compacta lo pendiente y, si aún no cabe, crece. */
    private fun ensureRoom(n: Int) {
        if (buf.size - count >= n) return
        if (pos > 0) {
            System.arraycopy(buf, pos, buf, 0, count - pos)
            count -= pos
            pos = 0
            if (buf.size - count >= n) return
        }
        var size = buf.size
        while (size - count < n) size *= 2
        buf = buf.copyOf(size)
    }
}
