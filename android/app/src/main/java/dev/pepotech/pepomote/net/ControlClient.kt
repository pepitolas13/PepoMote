package dev.pepotech.pepomote.net

import org.json.JSONObject
import java.io.BufferedReader
import java.io.BufferedWriter
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** Canal de control TCP (PROTOCOL.md §3): hello/ok/err, ping 1 Hz, mode. */
class ControlClient(
    private val host: String,
    private val port: Int,
    private val token: String,
    private val deviceName: String,
    private val deviceModel: String,
    private val callbacks: Callbacks
) {
    interface Callbacks {
        fun onOk(sessionId: Int, udpPort: Int, mode: String, slot: Int)
        fun onError(code: String, msg: String)
        fun onModeChanged(mode: String)
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
            )

            while (running) {
                val line = r.readLine() ?: break
                if (line.isBlank()) continue
                val msg = JSONObject(line)
                when (msg.optString("m")) {
                    "ok" -> callbacks.onOk(
                        msg.getInt("session_id"),
                        msg.optInt("udp_port", port),
                        msg.optString("mode", "pointer"),
                        msg.optInt("slot", 0)
                    )

                    "err" -> {
                        callbacks.onError(msg.optString("code"), msg.optString("msg"))
                        return@Thread
                    }

                    "ping" -> sendJson(JSONObject().put("m", "pong").put("t", msg.opt("t")))
                    "pong" -> Unit
                    "mode" -> callbacks.onModeChanged(msg.optString("mode", "pointer"))
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

    private fun sendJson(obj: JSONObject) {
        val text = obj.toString()
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
}
