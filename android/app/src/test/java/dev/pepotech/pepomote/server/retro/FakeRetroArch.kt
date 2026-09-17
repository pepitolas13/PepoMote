package dev.pepotech.pepomote.server.retro

import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.channels.DatagramChannel
import java.util.ArrayDeque
import java.util.concurrent.locks.LockSupport

/**
 * RetroArch falso para los tests (espejo de `desktop/e2e/e2e_retroarch.py`):
 * escucha en los puertos del mando en red (UN datagrama por jugador y
 * fotograma, como `input_driver.c`) y en la interfaz de comandos (`VERSION`
 * contesta la versión; `GET_STATUS` se cuenta, porque cierra una 1.22.2 real;
 * las teclas rápidas se apuntan con su fotograma; `SHOW_MSG` se guarda).
 */
class FakeRetroArch(val ports: RetroPorts, private val fps: Double = 60.0, users: Int = 4) : AutoCloseable {
    private val cmd = DatagramChannel.open().apply { configureBlocking(false); bind(InetSocketAddress(LOOPBACK, ports.cmd)) }
    private val pads = (0 until users).map { DatagramChannel.open().apply { configureBlocking(false); bind(InetSocketAddress(LOOPBACK, ports.base + it)) } }
    private val pending = List(users) { ArrayDeque<ByteArray>() }
    private val lock = Any()
    val buttons = IntArray(users)
    val axes = Array(users) { IntArray(4) }
    /** (fotograma, comando) que no son sondas. */
    val commands = mutableListOf<Pair<Int, String>>()
    val osd = mutableListOf<String>()
    /** Fotogramas en los que ya esperaba un segundo datagrama (cola: lo que el enlace evita). */
    @Volatile var doubles = 0
    /** Como en Windows: un fotograma sin datagrama vacía el mando. */
    @Volatile var resetOnEmpty = false
    /** Dejar de contestar (RetroArch detrás o cerrado). */
    @Volatile var mute = false
    @Volatile var version = "1.22.2"
    @Volatile var frames = 0
    @Volatile var probes = 0
    @Volatile var statusRequests = 0
    @Volatile private var stopped = false
    private val thread = Thread(::run, "fake-retroarch").apply { isDaemon = true }

    fun start(): FakeRetroArch { thread.start(); return this }

    fun buttons(user: Int): Int = synchronized(lock) { buttons[user] }
    fun axes(user: Int): List<Int> = synchronized(lock) { axes[user].toList() }
    fun commands(): List<Pair<Int, String>> = synchronized(lock) { commands.toList() }
    fun osd(): List<String> = synchronized(lock) { osd.toList() }
    fun clearState() = synchronized(lock) { buttons.fill(0); axes.forEach { it.fill(0) } }

    private fun frame() {
        synchronized(lock) {
            val buf = ByteBuffer.allocate(2048)
            while (true) {
                buf.clear()
                val from = try { cmd.receive(buf) } catch (_: IOException) { null } ?: break
                if (mute) continue
                buf.flip()
                val text = String(buf.array(), 0, buf.limit(), Charsets.UTF_8)
                for (raw in text.split('\n')) {
                    val tok = raw.trim()
                    if (tok.isEmpty()) continue
                    when {
                        tok == "VERSION" -> { probes++; reply(from, "$version\n") }
                        tok == "GET_STATUS" -> { probes++; statusRequests++; reply(from, "GET_STATUS PLAYING snes9x,Test Game.sfc\n") }
                        tok.startsWith("SHOW_MSG ") -> osd += tok.substring(9)
                        else -> commands += frames to tok
                    }
                }
            }
            for ((u, s) in pads.withIndex()) {
                val queue = pending[u]
                while (true) {
                    buf.clear()
                    if ((try { s.receive(buf) } catch (_: IOException) { null }) == null) break
                    buf.flip()
                    queue.addLast(ByteArray(buf.limit()).also { buf.get(it) })
                }
                val data = queue.pollFirst()
                if (data != null && data.size == 20) {
                    val b = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
                    b.getInt(); val device = b.getInt(); val index = b.getInt(); val ident = b.getInt(); val state = b.getShort().toInt()
                    if (device == 1 && ident < 16) {
                        buttons[u] = buttons[u] and (1 shl ident).inv()
                        if (state != 0) buttons[u] = buttons[u] or (1 shl ident)
                    } else if (device == 5 && ident < 2 && index < 2) {
                        axes[u][index * 2 + ident] = state
                    }
                    if (queue.isNotEmpty()) doubles++
                } else if (data != null) {
                    buttons[u] = 0; axes[u].fill(0)
                } else if (resetOnEmpty) {
                    buttons[u] = 0; axes[u].fill(0)
                }
            }
            frames++
        }
    }

    private fun reply(to: java.net.SocketAddress, text: String) {
        try { cmd.send(ByteBuffer.wrap(text.toByteArray(Charsets.UTF_8)), to) } catch (_: IOException) { }
    }

    private fun run() {
        val period = (1_000_000_000L / fps).toLong()
        var next = System.nanoTime()
        while (!stopped) {
            frame()
            next += period
            val delay = next - System.nanoTime()
            if (delay > 0) LockSupport.parkNanos(delay) else next = System.nanoTime()
        }
    }

    override fun close() {
        stopped = true
        thread.join(1000)
        runCatching { cmd.close() }
        pads.forEach { runCatching { it.close() } }
    }

    companion object {
        private val LOOPBACK: InetAddress = InetAddress.getByName("127.0.0.1")

        /** Puertos libres: base + 0..3 para el mando y otro para los comandos. */
        fun freePorts(): RetroPorts {
            repeat(50) {
                val base = 40000 + (Math.random() * 20000).toInt()
                val cmdPort = base + 10
                val channels = mutableListOf<DatagramChannel>()
                try {
                    for (p in listOf(base, base + 1, base + 2, base + 3, cmdPort)) channels += DatagramChannel.open().apply { bind(InetSocketAddress(LOOPBACK, p)) }
                    return RetroPorts(base, cmdPort)
                } catch (_: IOException) {
                } finally { channels.forEach { runCatching { it.close() } } }
            }
            error("no free port range")
        }

        fun await(timeoutMs: Long = 3000, message: String = "condition", condition: () -> Boolean) {
            val deadline = System.nanoTime() + timeoutMs * 1_000_000
            while (System.nanoTime() < deadline) {
                if (condition()) return
                Thread.sleep(5)
            }
            throw AssertionError(message)
        }
    }
}
