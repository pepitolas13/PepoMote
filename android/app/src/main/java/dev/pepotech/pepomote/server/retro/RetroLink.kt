package dev.pepotech.pepomote.server.retro

import dev.pepotech.pepomote.server.core.ReceiverInput
import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.PortUnreachableException
import java.nio.ByteBuffer
import java.nio.channels.DatagramChannel
import java.nio.channels.SelectionKey
import java.nio.channels.Selector
import java.util.ArrayDeque

/** Puertos de RetroArch (los de sus ajustes de red; en tests, otros). */
data class RetroPorts(val base: Int = RetroProtocol.DEFAULT_BASE_PORT, val cmd: Int = RetroProtocol.DEFAULT_CMD_PORT)

/** Qué se sabe de RetroArch ahora mismo (para el panel y los avisos). */
data class RetroLive(
    /** Responde a la interfaz de comandos (reloj sincronizado). */
    val reachable: Boolean = false,
    /** Hay móviles en modo RetroArch y RetroArch lleva un rato mudo: se manda a ciegas a 60 Hz. */
    val degraded: Boolean = false,
    val version: String? = null,
    val activity: Activity? = null,
    /** Sondeos por segundo que se le ven (≈ fotogramas/s). */
    val pollsPerSec: Float = 0f,
)

/**
 * El enlace vivo con RetroArch en este mismo Android, igual que
 * `desktop/src/retroarch/link.rs`: RetroArch lee UN datagrama del mando en
 * red por jugador y por fotograma, así que aquí no hay temporizador: siempre
 * hay UNA sonda (`VERSION`) en camino por la interfaz de comandos y cada
 * respuesta es un fotograma consumido, es decir, hueco para un datagrama por
 * jugador. Nunca hay más de un datagrama en espera por jugador.
 *
 * En Android RetroArch solo atiende la red mientras está en primer plano;
 * en segundo plano bloquea su bucle. Sin respuestas se sondea cada medio
 * segundo (y cada 2 s sincronizado). Sin proceso que vigilar (Android no
 * deja verlo), el modo degradado (60 Hz a ciegas, solo cambios y un refresco
 * lento) entra cuando hay móviles en modo RetroArch y lleva 3 s mudo: cubre
 * a quien activó el mando en red pero no los comandos de red.
 */
class RetroLink(
    private val ports: RetroPorts = RetroPorts(),
    private val osdReady: String = "PepoMote: phone controller connected",
    private val onLive: (RetroLive) -> Unit = {},
) : AutoCloseable {
    private val lock = Any()
    private val sched = RetroScheduler()
    private val osd = ArrayDeque<String>()
    @Volatile private var running = false
    private var thread: Thread? = null
    private var selector: Selector? = null
    private var cmd: DatagramChannel? = null
    private var pad: DatagramChannel? = null

    @Volatile var live = RetroLive()
        private set

    /** Abre los sockets (loopback, puertos efímeros) y arranca el hilo. */
    @Throws(IOException::class)
    fun start() {
        synchronized(lock) {
            if (running) return
            val sel = Selector.open()
            val c = DatagramChannel.open()
            val p = DatagramChannel.open()
            try {
                c.configureBlocking(false)
                c.bind(InetSocketAddress(LOOPBACK, 0))
                c.connect(InetSocketAddress(LOOPBACK, ports.cmd))
                c.register(sel, SelectionKey.OP_READ)
                p.bind(InetSocketAddress(LOOPBACK, 0))
            } catch (e: IOException) {
                runCatching { c.close() }; runCatching { p.close() }; runCatching { sel.close() }
                throw e
            }
            selector = sel; cmd = c; pad = p
            running = true
            thread = Thread({ run(sel, c, p) }, "pepomote-retroarch").apply { isDaemon = true; start() }
        }
    }

    override fun close() {
        val t: Thread?
        synchronized(lock) {
            if (!running) return
            running = false
            t = thread
            thread = null
            selector?.wakeup()
        }
        if (t !== Thread.currentThread()) try { t?.join(1500) } catch (_: InterruptedException) { Thread.currentThread().interrupt() }
        synchronized(lock) {
            // El hilo ya no puede volver a pulsar después de estas liberaciones.
            pad?.let { channel -> for (slot in 0..3) if (sched.isActive(slot)) resetPad(channel, slot) }
            runCatching { cmd?.close() }; runCatching { pad?.close() }; runCatching { selector?.close() }
            cmd = null; pad = null; selector = null
        }
    }

    internal fun pushInput(slot: Int, kind: RetroPadKind, input: ReceiverInput) = synchronized(lock) { sched.pushInput(slot, kind, input) }
    fun push(slot: Int, state: PadState) = synchronized(lock) { sched.push(slot, state) }
    fun release(slot: Int) = synchronized(lock) { sched.release(slot) }
    fun setPresent(slot: Int, present: Boolean) = synchronized(lock) { sched.setPresent(slot, present) }
    fun hotkey(slot: Int, name: String, down: Boolean): Boolean = synchronized(lock) { sched.hotkey(slot, name, down) }
    fun anyPresent(): Boolean = synchronized(lock) { sched.anyPresent() }

    /** Aviso en la pantalla de RetroArch (si responde). */
    fun showMessage(text: String) = synchronized(lock) { if (osd.size < 8) osd.addLast(text) }

    /**
     * RetroArch vacía el jugador al recibir un datagrama de tamaño distinto de remote_message.
     * Un byte garantiza el envío UDP y suelta a la vez botones y ejes (input_driver.c).
     */
    private fun resetPad(channel: DatagramChannel, slot: Int) {
        try { channel.send(ByteBuffer.wrap(byteArrayOf(0)), InetSocketAddress(LOOPBACK, ports.base + slot)) }
        catch (_: IOException) { }
    }

    private fun publish(transform: (RetroLive) -> RetroLive) {
        val next = transform(live)
        live = next
        runCatching { onLive(next) }
    }

    private fun run(selector: Selector, cmd: DatagramChannel, pad: DatagramChannel) {
        val buf = ByteBuffer.allocate(4096)
        val padAddr = Array(4) { InetSocketAddress(LOOPBACK, ports.base + it) }
        var outstanding: Long? = null
        var probes = 0
        var statusOk = false
        var lastReply: Long? = null
        var synced = false
        var windowStart = System.nanoTime()
        var windowReplies = 0
        var lastMuteCheck = System.nanoTime() - 10 * SECOND
        var degraded = false
        var nextDegradedTick = System.nanoTime()
        var nextDegradedRefresh = System.nanoTime()

        fun sendCmd(text: String) {
            try { cmd.write(ByteBuffer.wrap(text.toByteArray(Charsets.UTF_8))) } catch (_: IOException) { }
        }
        fun sendPad(slot: Int, c: Control, v: Int) {
            try { pad.send(ByteBuffer.wrap(RetroProtocol.encode(slot, c, v)), padAddr[slot]) } catch (_: IOException) { }
        }
        fun receiveOne(): String? {
            buf.clear()
            val from = try { cmd.receive(buf) } catch (_: PortUnreachableException) { null } catch (_: IOException) { null }
                ?: return null
            buf.flip()
            return String(buf.array(), 0, buf.limit(), Charsets.UTF_8)
        }
        fun frameOut(messages: List<Triple<Int, Control, Int>>, held: List<String>, pulses: List<String>) {
            for ((slot, c, v) in messages) sendPad(slot, c, v)
            for (h in held) sendCmd(h)
            for (p in pulses) sendCmd(p)
        }

        while (running) {
            val now = System.nanoTime()
            // Reloj: siempre una sonda en camino
            val retry = if (synced) PROBE_RETRY else PROBE_RETRY_IDLE
            val sent = outstanding
            if (sent == null || now - sent > retry) {
                val retrying = sent != null
                sendCmd(probeFor(probes, retrying, statusOk))
                if (!retrying) probes++
                outstanding = now
            }
            // ¿Sigue ahí?
            val last = lastReply
            if (synced && last != null && now - last > REACH_TTL) {
                synced = false
                statusOk = false
                publish { it.copy(reachable = false, pollsPerSec = 0f, version = null, activity = null) }
            }
            if (!synced && now - lastMuteCheck >= 2 * SECOND) {
                lastMuteCheck = now
                val muteFor = if (last != null) now - last else now - windowStart
                val want = synchronized(lock) { sched.anyPresent() } && muteFor > DEGRADED_AFTER
                if (want != degraded) {
                    degraded = want
                    publish { it.copy(degraded = degraded) }
                }
            }
            if (degraded && !synced && now >= nextDegradedTick) {
                nextDegradedTick = now + DEGRADED_TICK
                val refresh = now >= nextDegradedRefresh
                if (refresh) nextDegradedRefresh = now + DEGRADED_REFRESH
                val (messages, held, pulses) = synchronized(lock) {
                    Triple(if (refresh) sched.frame() else sched.changes(), sched.heldCommands(), sched.takePulses())
                }
                frameOut(messages, held, pulses)
            }

            try {
                selector.selectedKeys().clear()
                selector.select(if (synced || degraded) 5L else 50L)
            } catch (_: IOException) { if (!running) break else continue }
            if (!running) break
            val first = receiveOne() ?: continue
            val tReply = System.nanoTime()
            // Todo lo que ya estuviera en el socket vino del MISMO sondeo (una sonda
            // repetida que al final sí se contestó): es un fotograma, no dos
            var reply = first
            while (true) reply = receiveOne() ?: break
            outstanding = null
            lastReply = tReply
            windowReplies++
            if (tReply - windowStart >= SECOND) {
                val pps = windowReplies * SECOND.toFloat() / (tReply - windowStart)
                windowStart = tReply
                windowReplies = 0
                publish { it.copy(pollsPerSec = pps) }
            }
            when (val parsed = RetroProtocol.parseReply(reply)) {
                is Reply.Version -> {
                    statusOk = RetroProtocol.statusQuerySafe(parsed.version)
                    if (live.version != parsed.version) publish { it.copy(version = parsed.version) }
                }
                is Reply.Status -> if (live.activity != parsed.activity) publish { it.copy(activity = parsed.activity) }
                is Reply.Other -> Unit
            }
            var resetSlots = emptyList<Int>()
            if (!synced) {
                synced = true
                degraded = false
                val present = synchronized(lock) {
                    // Pasar al fondo en Android conserva el estado: se vacía antes de recomponerlo.
                    resetSlots = (0..3).filter { sched.isActive(it) }
                    sched.resync()
                    sched.anyPresent()
                }
                publish { it.copy(reachable = true, degraded = false) }
                if (present) showMessage(osdReady)
            }

            // Este fotograma: tras el margen, un datagrama por jugador, las teclas
            // mantenidas y los comandos sueltos; luego la siguiente sonda
            if (!running) break
            preciseWait(GUARD)
            val (messages, held, pulses) = synchronized(lock) {
                // El vaciado ocupa este fotograma; los controles se recomponen en los siguientes.
                Triple(if (resetSlots.isEmpty()) sched.frame() else emptyList(), sched.heldCommands(), sched.takePulses())
            }
            for (slot in resetSlots) resetPad(pad, slot)
            frameOut(messages, held, pulses)
            val text = synchronized(lock) { osd.pollFirst() }
            if (text != null) sendCmd("${RetroProtocol.CMD_SHOW_MSG} $text")
        }
    }

    companion object {
        private const val SECOND = 1_000_000_000L
        /** 300 µs entre la respuesta y nuestro envío: que el datagrama no entre en el MISMO sondeo. */
        private const val GUARD = 300_000L
        /** Sincronizado y sin respuesta en este tiempo (RetroArch cargando algo), la sonda se repite. */
        private const val PROBE_RETRY = 2 * SECOND
        /** Sin sincronizar (RetroArch cerrado o detrás): sonda cada medio segundo. */
        private const val PROBE_RETRY_IDLE = 500_000_000L
        /** Sin respuesta en este tiempo, RetroArch ya no está (o no contesta). */
        private const val REACH_TTL = 1_200_000_000L
        private const val DEGRADED_AFTER = 3 * SECOND
        private const val DEGRADED_TICK = 16_667_000L
        private const val DEGRADED_REFRESH = 50_000_000L
        /** Cada cuántas sondas va un GET_STATUS en vez de VERSION (solo con versión segura). */
        internal const val STATUS_EVERY = 30
        private val LOOPBACK: InetAddress = InetAddress.getByName("127.0.0.1")

        /** Una repetición es siempre `VERSION`; `GET_STATUS` solo cada [STATUS_EVERY] y con versión segura. */
        internal fun probeFor(probes: Int, retry: Boolean, statusOk: Boolean): String =
            if (!retry && statusOk && probes % STATUS_EVERY == 0) RetroProtocol.CMD_GET_STATUS else RetroProtocol.CMD_VERSION

        private fun preciseWait(nanos: Long) {
            val until = System.nanoTime() + nanos
            while (System.nanoTime() < until) Thread.yield()
        }
    }
}
