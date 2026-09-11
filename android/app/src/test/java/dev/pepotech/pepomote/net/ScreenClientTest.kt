package dev.pepotech.pepomote.net

import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket

/**
 * Cliente del canal de pantalla contra un receptor falso en 127.0.0.1: la
 * línea `screen`, el ok, las tramas (keepalive, estado, imagen), la
 * confirmación de un byte tras pintar y las reconexiones. El decodificador
 * es falso (copia la carga): BitmapFactory no existe en la JVM.
 */
class ScreenClientTest {

    /** «Decodifica» copiando los bytes; null si empiezan por 0 (imagen rota). */
    private class CopyDecoder : ScreenDecoder<ByteArray> {
        override fun decode(data: ByteArray, off: Int, len: Int): ByteArray? =
            if (len > 0 && data[off] == 0.toByte()) null else data.copyOfRange(off, off + len)
    }

    private class FakeReceiver : AutoCloseable {
        private val server = ServerSocket(0, 4, InetAddress.getLoopbackAddress()).apply { soTimeout = 5000 }
        val port: Int get() = server.localPort
        var socket: Socket? = null
        lateinit var input: InputStream
        lateinit var output: OutputStream

        /** Espera la conexión del móvil y devuelve su primera línea (sin `\n`). */
        fun accept(): String {
            socket?.close()
            val s = server.accept().apply { soTimeout = 5000 }
            socket = s
            input = s.getInputStream()
            output = s.getOutputStream()
            val sb = StringBuilder()
            while (true) {
                val b = input.read()
                if (b < 0 || b == '\n'.code) return sb.toString()
                sb.append(b.toChar())
            }
        }

        fun send(vararg chunks: ByteArray) {
            chunks.forEach { output.write(it) }
            output.flush()
        }

        fun ok() = send("{\"m\":\"screen\",\"ok\":true}\n".toByteArray())

        override fun close() {
            socket?.close()
            server.close()
        }
    }

    private fun frame(type: Int, payload: ByteArray): ByteArray {
        val out = ByteArray(ScreenFrames.HEADER_LEN + payload.size)
        "PMPS".toByteArray(Charsets.US_ASCII).copyInto(out)
        out[4] = type.toByte()
        out[5] = (payload.size and 0xFF).toByte()
        out[6] = ((payload.size ushr 8) and 0xFF).toByte()
        out[7] = ((payload.size ushr 16) and 0xFF).toByte()
        out[8] = ((payload.size ushr 24) and 0xFF).toByte()
        payload.copyInto(out, ScreenFrames.HEADER_LEN)
        return out
    }

    private val keepalive = frame(ScreenFrames.TYPE_KEEPALIVE, ByteArray(0))
    private val jpeg = byteArrayOf(
        0xFF.toByte(), 0xD8.toByte(), 0xFF.toByte(), 0xE0.toByte(), 1, 2, 3, 0xFF.toByte(), 0xD9.toByte()
    )

    private fun <T> StateFlow<T>.await(timeoutMs: Long = 3000, pred: (T) -> Boolean): T =
        runBlocking { withTimeout(timeoutMs) { first(pred) } }

    private fun client(
        port: Int,
        fallback: Long = 5000,
        reply: Int = 3000,
        data: Int = 6000,
        retryDrop: Long = 1000,
        retryUnavailable: Long = 5000
    ) = ScreenClient(
        "127.0.0.1", port, 0x12345678, 854, 480, CopyDecoder(),
        shownFallbackMs = fallback,
        replyTimeoutMs = reply,
        dataTimeoutMs = data,
        retryAfterDropMs = retryDrop,
        retryUnavailableMs = retryUnavailable
    )

    @Test
    fun lineaScreenOkImagenYConfirmacionTrasPintar() {
        FakeReceiver().use { rx ->
            val c = client(rx.port)
            c.start()
            try {
                assertEquals("{\"m\":\"screen\",\"session_id\":305419896,\"w\":854,\"h\":480,\"q\":70}", rx.accept())
                rx.ok()
                rx.send(keepalive, frame(ScreenFrames.TYPE_STATUS, "Esperando…".toByteArray()), frame(ScreenFrames.TYPE_IMAGE, jpeg))
                val img = c.image.await { it != null }!!
                assertArrayEquals(jpeg, img.bitmap)
                assertEquals(1, img.seq)
                // El estado recibido antes se conserva (la UI lo enseña cuando no hay imagen)
                assertEquals("Esperando…", c.status.value)
                // Ni el keepalive ni el estado se confirman, y la imagen aún no está pintada
                Thread.sleep(50)
                assertEquals(0, rx.input.available())
                c.shown(img.seq)
                assertEquals(1, rx.input.read())
                // Repetir el aviso no repite el byte
                c.shown(img.seq)
                Thread.sleep(50)
                assertEquals(0, rx.input.available())
                // Siguiente imagen (solo tras la confirmación): seq 2
                rx.send(frame(ScreenFrames.TYPE_IMAGE, jpeg))
                val img2 = c.image.await { it != null && it.seq == 2 }!!
                assertArrayEquals(jpeg, img2.bitmap)
                c.shown(img2.seq)
                assertEquals(1, rx.input.read())
            } finally {
                c.close()
            }
            // Cerrar cierra el socket: el receptor ve el fin, y no queda imagen publicada
            assertEquals(-1, rx.input.read())
            assertNull(c.image.value)
            assertEquals(0, c.fps.value)
        }
    }

    @Test
    fun siNadiePintaLaImagenSeConfirmaIgualmente() {
        FakeReceiver().use { rx ->
            val c = client(rx.port, fallback = 100)
            c.start()
            try {
                rx.accept()
                rx.ok()
                rx.send(frame(ScreenFrames.TYPE_IMAGE, jpeg))
                // Sin shown(): a los 100 ms sale el byte igualmente
                assertEquals(1, rx.input.read())
            } finally {
                c.close()
            }
        }
    }

    @Test
    fun imagenQueNoDecodificaSeConfirmaSinPublicar() {
        FakeReceiver().use { rx ->
            val c = client(rx.port)
            c.start()
            try {
                rx.accept()
                rx.ok()
                rx.send(frame(ScreenFrames.TYPE_IMAGE, ByteArray(5))) // empieza por 0: el decodificador la rechaza
                assertEquals(1, rx.input.read())
                assertNull(c.image.value)
            } finally {
                c.close()
            }
        }
    }

    @Test
    fun elEstadoQuitaLaImagen() {
        FakeReceiver().use { rx ->
            val c = client(rx.port)
            c.start()
            try {
                rx.accept()
                rx.ok()
                rx.send(frame(ScreenFrames.TYPE_IMAGE, jpeg))
                val img = c.image.await { it != null }!!
                c.shown(img.seq)
                assertEquals(1, rx.input.read())
                rx.send(frame(ScreenFrames.TYPE_STATUS, "Cemu no está abierto".toByteArray()))
                assertEquals("Cemu no está abierto", c.status.await { it != null })
                assertNull(c.image.value)
            } finally {
                c.close()
            }
        }
    }

    @Test
    fun receptorMudoEsSinPantallaYReintenta() {
        FakeReceiver().use { rx ->
            val c = client(rx.port, reply = 200, retryUnavailable = 200)
            c.start()
            try {
                rx.accept() // y no contesta
                assertEquals(ScreenClient.STATUS_UNAVAILABLE, c.status.await { it != null })
                // Reintento: otra conexión con la misma línea
                assertTrue(rx.accept().startsWith("{\"m\":\"screen\""))
            } finally {
                c.close()
            }
        }
    }

    @Test
    fun errDelReceptorEsSinPantallaConElMotivo() {
        FakeReceiver().use { rx ->
            val c = client(rx.port)
            c.start()
            try {
                rx.accept()
                rx.send("{\"m\":\"err\",\"code\":\"bad_session\",\"msg\":\"sesión desconocida\"}\n".toByteArray())
                assertEquals("Sin pantalla: sesión desconocida", c.status.await { it != null })
            } finally {
                c.close()
            }
        }
    }

    @Test
    fun caidaTrasOkReconecta() {
        FakeReceiver().use { rx ->
            val c = client(rx.port, retryDrop = 200)
            c.start()
            try {
                rx.accept()
                rx.ok()
                rx.send(frame(ScreenFrames.TYPE_STATUS, "hola".toByteArray()))
                assertEquals("hola", c.status.await { it == "hola" })
                rx.socket!!.close() // el receptor se cae
                assertEquals(
                    ScreenClient.STATUS_RECONNECTING,
                    c.status.await { it == ScreenClient.STATUS_RECONNECTING }
                )
                val t0 = System.nanoTime()
                assertTrue(rx.accept().contains("\"session_id\":305419896"))
                assertTrue((System.nanoTime() - t0) / 1_000_000 < 3000)
                rx.ok()
                rx.send(frame(ScreenFrames.TYPE_IMAGE, jpeg))
                val img = c.image.await { it != null }!!
                c.shown(img.seq)
                assertEquals(1, rx.input.read())
            } finally {
                c.close()
            }
        }
    }

    @Test
    fun elKeepaliveMantieneVivaLaConexionYElSilencioLaMata() {
        FakeReceiver().use { rx ->
            val c = client(rx.port, data = 300, retryDrop = 100)
            c.start()
            try {
                rx.accept()
                rx.ok()
                // 600 ms (más que el tope de 300 ms) con keepalives: sigue viva
                repeat(4) {
                    Thread.sleep(150)
                    rx.send(keepalive)
                }
                assertNull(c.status.value)
                assertEquals(0, rx.input.available()) // el keepalive no se confirma
                // Silencio: se da por muerta y reconecta
                assertEquals(
                    ScreenClient.STATUS_RECONNECTING,
                    c.status.await(timeoutMs = 2000) { it == ScreenClient.STATUS_RECONNECTING }
                )
                assertTrue(rx.accept().startsWith("{\"m\":\"screen\""))
            } finally {
                c.close()
            }
        }
    }

    @Test
    fun respuestaJsonSinOrgJson() {
        assertTrue(ScreenClient.replyOk("{\"m\":\"screen\",\"ok\":true}"))
        assertTrue(ScreenClient.replyOk("{ \"ok\" : true , \"m\" : \"screen\" }"))
        assertEquals(false, ScreenClient.replyOk("{\"m\":\"screen\",\"ok\":false}"))
        assertEquals(false, ScreenClient.replyOk("{\"m\":\"err\",\"code\":\"bad_session\",\"msg\":\"no\"}"))
        assertEquals("a \"b\" \\ c\n/", ScreenClient.jsonString("{\"msg\":\"a \\\"b\\\" \\\\ c\\n\\/\"}", "msg"))
        assertEquals("ñ", ScreenClient.jsonString("{\"msg\":\"\\u00f1\"}", "msg"))
        assertNull(ScreenClient.jsonString("{\"m\":\"err\"}", "msg"))
        assertEquals(
            "{\"m\":\"screen\",\"session_id\":4294967295,\"w\":640,\"h\":360,\"q\":70}",
            ScreenClient.helloLine(-1, 640, 360, 70)
        )
    }
}
