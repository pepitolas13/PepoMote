package dev.pepotech.pepomote.server.core

import dev.pepotech.pepomote.control.RetroLayouts
import dev.pepotech.pepomote.server.retro.RetroGameInfo
import dev.pepotech.pepomote.server.retro.RetroHistory
import dev.pepotech.pepomote.server.retro.RetroLink
import dev.pepotech.pepomote.server.retro.RetroLive
import dev.pepotech.pepomote.server.retro.RetroPadKind
import dev.pepotech.pepomote.server.retro.RetroPorts
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.PortUnreachableException
import java.net.SocketAddress
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.nio.channels.SelectionKey
import java.nio.channels.Selector
import java.nio.channels.ServerSocketChannel
import java.nio.channels.SocketChannel
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.ArrayDeque
import java.util.Collections

data class ReceiverConfig(
    val name: String, val token: String, val pairCode: String, val port: Int = 26761, val dsuPort: Int = 26760,
    /** Puertos del mando en red y de los comandos de red de RetroArch (sus valores de serie). */
    val retroPorts: RetroPorts = RetroPorts(),
    /** Avisos que se mandan a los móviles y a RetroArch (el servicio los pasa traducidos). */
    val texts: ReceiverTexts = ReceiverTexts(),
) {
    init {
        require(name.isNotBlank() && name.length <= 128)
        require(token.isNotBlank() && token.length <= 256)
        require(pairCode.length in 4..8 && pairCode.all { it in '0'..'9' })
        require(port in 0..65535 && dsuPort in 0..65535)
        require(port == 0 || port != dsuPort)
        require(retroPorts.cmd in 1..65535 && retroPorts.base in 1..65532)
    }
}

/** Textos del receptor (en inglés por defecto; ServerForegroundService los traduce). */
data class ReceiverTexts(
    val modesOnly: String = "This Android receiver supports Dolphin, Eden and RetroArch only",
    val playerOne: String = "Only Player 1 can change the receiver mode",
    val textUnavailable: String = "Text input is unavailable on this Android receiver",
    val screenUnavailable: String = "Screen streaming is unavailable on this Android receiver",
    val retroNoGun: String = "The light gun needs the PC receiver: this Android keeps your current RetroArch pad",
    val retroOpened: String = "RetroArch is answering: RetroArch mode",
    /** `%s` = nombre del modo al que se vuelve. */
    val retroBack: String = "An emulator asked for the controllers: %s mode",
    val retroOsdReady: String = "PepoMote: phone controller connected",
    val dolphinName: String = "Dolphin",
    val edenName: String = "Eden",
) {
    fun modeName(mode: ReceiverMode): String = when (mode) {
        ReceiverMode.Dolphin -> dolphinName
        ReceiverMode.Eden -> edenName
        ReceiverMode.RetroArch -> "RetroArch"
    }
}

/** Qué se sabe de RetroArch (enlace y juego cargado) para el panel. */
data class RetroStatus(
    val reachable: Boolean = false,
    val degraded: Boolean = false,
    val version: String? = null,
    val pollsPerSec: Float = 0f,
    val game: RetroGameInfo? = null,
)

data class ReceiverPeerSnapshot(
    val id: String, val slot: Int, val name: String, val ownNunchuk: Boolean,
    val batteryPct: Int, val inputHz: Double, val lastInputAgeMs: Long?, val framesReceived: Long,
    /** Modo RetroArch: mando (`retropad`/`nes`) y plantilla de consola que enseña el móvil. */
    val retroPad: String? = null, val retroLayout: String? = null,
)

data class ReceiverSnapshot(
    val running: Boolean, val mode: ReceiverMode, val port: Int, val dsuPort: Int,
    val peers: List<ReceiverPeerSnapshot>, val dsuClients: Int, val error: String? = null,
    val retro: RetroStatus = RetroStatus(),
)

/**
 * Android-independent PMP receiver. TCP is nonblocking with bounded queues; input goes
 * straight from the PMP UDP thread to loopback DSU without a frame queue. Callbacks run
 * outside the state lock, on the calling/control thread, and should return promptly.
 */
class ReceiverCore(private val config: ReceiverConfig, private val onState: (ReceiverSnapshot) -> Unit = {}) : AutoCloseable {
    private val lock = Any()
    private val random = SecureRandom()
    private val tokenHash = hash(config.token)
    private val codeHash = hash(config.pairCode)
    private var current: Run? = null
    private var mode = ReceiverMode.Dolphin
    private var port = config.port
    private var dsuPort = config.dsuPort
    private var error: String? = null
    /** RetroArch: el juego cargado (lo pone el servicio) y lo que dice el enlace. */
    private var retroGame: RetroGameInfo? = null
    private var retroLive = RetroLive()
    private var retroLostAt = 0L
    /** Último modo que no era RetroArch: adonde se vuelve cuando un emulador pide los mandos. */
    private var lastNonRetroMode = ReceiverMode.Dolphin

    private class Run(val server: ServerSocketChannel, val selector: Selector, val pmp: DatagramSocket, val dsu: DatagramSocket, val retro: RetroLink) {
        val connections = LinkedHashMap<SocketChannel, Peer>()
        val sessions = HashMap<Int, Peer>()
        val clients = LinkedHashMap<SocketAddress, Subscription>()
        val failures = LinkedHashMap<InetAddress, FailureWindow>()
        val threads = ArrayList<Thread>(3)
        val discoveryRate = Rate(20)
        val dsuRate = Rate(160)
        var counter = 0
        var eventDirty = false
        var lastSnapshot = System.nanoTime()
    }

    private class Peer(val channel: SocketChannel, val address: InetAddress, val created: Long) {
        val line = ByteArrayOutputStream()
        val outbound = ArrayDeque<ByteBuffer>()
        val commandRate = Rate(60)
        val inputRate = Rate(1000)
        val pingRate = Rate(4)
        var key: SelectionKey? = null
        var closeAfterWrite = false
        var lastWriteProgress = created
        var lineStarted = created
        var lastTraffic = created
        var lastPing = created
        var session: Int? = null
        var slot = -1
        var name = ""
        var ownNunchuk = false
        var input: ReceiverInput? = null
        var lastInput = 0L
        var previousSeq: Int? = null
        var previousRecenter: Int? = null
        var pulseUntil = 0L
        var dirty = false
        var sentConnected = false
        var sentNeutral = true
        var sentTouch = false
        var lastOutput = 0L
        var frames = 0L
        var rateFrames = 0L
        var rateStarted = created
        var inputHz = 0.0
        var retroPad = RetroPadKind.RetroPad
        var retroLayout: String? = null
        /** RetroArch: ya se soltó todo tras el último paquete (entrada caducada). */
        var retroReleased = true
    }

    private class Subscription(var pendingRetroReturn: Boolean) { val expires = LongArray(4) }
    private class FailureWindow(val started: Long, var attempts: Int = 0)
    private class Rate(private val maximum: Int) {
        private var started = 0L
        private var count = 0
        fun allow(now: Long): Boolean {
            if (now - started >= SECOND) { started = now; count = 0 }
            if (count >= maximum) return false
            count++
            return true
        }
    }

    /** Binding failures are reported in snapshot.error and thrown, with all partial binds closed. */
    @Throws(IOException::class)
    fun start() {
        var failure: IOException? = null
        synchronized(lock) {
            if (current != null) return
            var server: ServerSocketChannel? = null
            var selector: Selector? = null
            var pmp: DatagramSocket? = null
            var dsu: DatagramSocket? = null
            var retro: RetroLink? = null
            try {
                server = ServerSocketChannel.open()
                server.configureBlocking(false)
                server.socket().reuseAddress = true
                server.bind(InetSocketAddress(config.port), MAX_CONNECTIONS)
                val actualPort = (server.localAddress as InetSocketAddress).port
                pmp = DatagramSocket(null)
                pmp.reuseAddress = false
                pmp.bind(InetSocketAddress(actualPort))
                dsu = DatagramSocket(null)
                dsu.reuseAddress = false
                dsu.bind(InetSocketAddress(LOOPBACK, config.dsuPort))
                selector = Selector.open()
                server.register(selector, SelectionKey.OP_ACCEPT)
                retro = RetroLink(config.retroPorts, config.texts.retroOsdReady) { live -> retroLiveChanged(live) }
                retro.start()
                val run = Run(server, selector, pmp, dsu, retro)
                port = actualPort
                dsuPort = dsu.localPort
                error = null
                current = run
                launch(run, "control") { controlLoop(run) }
                launch(run, "input") { inputLoop(run) }
                launch(run, "dsu") { dsuLoop(run) }
            } catch (e: Exception) {
                runCatching { server?.close() }; runCatching { selector?.close() }
                pmp?.close(); dsu?.close()
                runCatching { retro?.close() }
                current = null
                error = e.message ?: "Receiver could not bind its ports"
                failure = if (e is IOException) e else IOException(error, e)
            }
        }
        publish(snapshot())
        failure?.let { throw it }
    }

    override fun close() = stop()

    private fun stop(expectedRun: Run? = null) {
        val stopped = synchronized(lock) {
            val run = current ?: return
            if (expectedRun != null && expectedRun !== run) return
            for (peer in run.sessions.values.toList()) emitLocked(run, peer, System.nanoTime(), forceDisconnected = true)
            current = null
            for (peer in run.connections.values.toList()) runCatching { peer.channel.close() }
            run.connections.clear(); run.sessions.clear(); run.clients.clear()
            runCatching { run.server.close() }
            run.pmp.close(); run.dsu.close()
            run.selector.wakeup()
            runCatching { run.selector.close() }
            run
        }
        // The link joins its own thread and that thread takes our lock in its callback: close it unlocked.
        stopped.retro.close()
        // A callback may close us from the control thread; never join the calling thread.
        for (thread in stopped.threads) if (thread !== Thread.currentThread()) {
            try { thread.join(1000) } catch (_: InterruptedException) { Thread.currentThread().interrupt() }
        }
        publish(snapshot())
    }

    fun setMode(mode: ReceiverMode) {
        synchronized(lock) {
            if (this.mode == mode) return
            val run = current
            if (run == null) this.mode = mode else changeModeLocked(run, mode, byReceiver = true)
        }
        // UI changes need not wait for the periodic statistics callback.
        publish(snapshot())
    }

    fun snapshot(): ReceiverSnapshot = synchronized(lock) { snapshotLocked(System.nanoTime()) }

    private fun snapshotLocked(now: Long): ReceiverSnapshot {
        val run = current
        val peers = run?.sessions?.values?.sortedBy { it.slot }?.map {
            ReceiverPeerSnapshot(
                java.lang.Integer.toUnsignedString(it.session!!), it.slot, it.name, it.ownNunchuk,
                it.input?.batteryPct ?: 0, if (it.lastInput == 0L || now - it.lastInput >= SECOND) 0.0 else it.inputHz,
                if (it.lastInput == 0L) null else ((now - it.lastInput) / 1_000_000).coerceAtLeast(0), it.frames,
                retroPad = if (mode == ReceiverMode.RetroArch) it.retroPad.wire else null,
                retroLayout = if (mode == ReceiverMode.RetroArch) it.retroLayout else null,
            )
        } ?: emptyList()
        return ReceiverSnapshot(run != null, mode, port, dsuPort, Collections.unmodifiableList(peers),
            run?.clients?.values?.count { it.expires.any { deadline -> deadline > now } } ?: 0, error,
            RetroStatus(retroLive.reachable, retroLive.degraded, retroLive.version, retroLive.pollsPerSec, retroGame))
    }

    private fun launch(run: Run, label: String, work: () -> Unit) {
        val thread = Thread({
            try { work() } catch (e: Exception) {
                val active = synchronized(lock) {
                    if (current !== run) false else { error = e.message ?: "Receiver $label stopped"; true }
                }
                // A late failure from a stopped run cannot close a newly started run.
                if (active) stop(run)
            }
        }, "pepomote-receiver-$label").apply { isDaemon = true }
        run.threads += thread
        thread.start()
    }

    private fun active(run: Run): Boolean = synchronized(lock) { current === run }
    private fun publish(state: ReceiverSnapshot) { runCatching { onState(state) } }

    private fun controlLoop(run: Run) {
        val readBuffer = ByteBuffer.allocate(2048)
        while (active(run)) {
            run.selector.select(25)
            val keys = run.selector.selectedKeys().iterator()
            while (keys.hasNext()) {
                val key = keys.next(); keys.remove()
                if (!key.isValid) continue
                if (key.isAcceptable) { accept(run); continue }
                val peer = key.attachment() as Peer
                try {
                    if (key.isReadable) readControl(run, peer, readBuffer)
                    if (key.isValid && key.isWritable) synchronized(lock) { if (current === run) flushLocked(run, peer) }
                } catch (_: Exception) {
                    synchronized(lock) { if (current === run) disconnectLocked(run, peer) }
                }
            }
            val notice = synchronized(lock) {
                if (current !== run) null else {
                    val now = System.nanoTime()
                    sweepLocked(run, now)
                    if (run.eventDirty || now - run.lastSnapshot >= SECOND / 2) {
                        run.eventDirty = false; run.lastSnapshot = now; snapshotLocked(now)
                    } else null
                }
            }
            if (notice != null) publish(notice)
        }
    }

    private fun accept(run: Run) {
        // Bound the amount of accept work in one selector turn.
        repeat(MAX_CONNECTIONS) {
            val channel = run.server.accept() ?: return
            synchronized(lock) {
                if (current !== run || run.connections.size >= MAX_CONNECTIONS) { channel.close(); return@synchronized }
                channel.configureBlocking(false)
                channel.socket().tcpNoDelay = true
                val peer = Peer(channel, (channel.remoteAddress as InetSocketAddress).address, System.nanoTime())
                peer.key = channel.register(run.selector, SelectionKey.OP_READ, peer)
                run.connections[channel] = peer
            }
        }
    }

    private fun readControl(run: Run, peer: Peer, buffer: ByteBuffer) {
        buffer.clear()
        val count = peer.channel.read(buffer)
        if (count < 0) { synchronized(lock) { if (current === run) disconnectLocked(run, peer) }; return }
        buffer.flip()
        synchronized(lock) {
            if (current !== run || !run.connections.containsKey(peer.channel) || peer.closeAfterWrite) return
            while (buffer.hasRemaining() && !peer.closeAfterWrite && run.connections.containsKey(peer.channel)) {
                val value = buffer.get().toInt() and 255
                if (value == 10) {
                    val bytes = peer.line.toByteArray(); peer.line.reset()
                    if (bytes.isEmpty()) continue
                    val text = try {
                        Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
                            .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()
                    } catch (_: Exception) { rejectLocked(run, peer, "bad_message", "Control messages must be UTF-8"); return }
                    handleControlLocked(run, peer, text)
                } else {
                    if (peer.line.size() >= MAX_LINE) { rejectLocked(run, peer, "too_large", "Control message is too large"); return }
                    if (peer.line.size() == 0) peer.lineStarted = System.nanoTime()
                    peer.line.write(value)
                }
            }
        }
    }

    private fun handleControlLocked(run: Run, peer: Peer, text: String) {
        val now = System.nanoTime()
        if (!peer.commandRate.allow(now)) { rejectLocked(run, peer, "rate_limited", "Too many control messages"); return }
        val message = try {
            if (!boundedJsonDepth(text)) throw IllegalArgumentException()
            JSONObject(text)
        } catch (_: Exception) { rejectLocked(run, peer, "bad_message", "Expected a JSON control message"); return }
        if (peer.session == null) { authenticateLocked(run, peer, message, now); return }
        peer.lastTraffic = now
        when (message.optString("m")) {
            "bye" -> disconnectLocked(run, peer)
            "ping" -> enqueueLocked(run, peer, JSONObject().put("m", "pong").put("t", message.opt("t")))
            "pong" -> Unit
            "mode" -> {
                val requested = ReceiverMode.entries.firstOrNull { it.wire == message.optString("mode") }
                if (requested == null) {
                    enqueueLocked(run, peer, modeMessage())
                    noticeLocked(run, peer, config.texts.modesOnly)
                } else if (peer.slot != 0) {
                    enqueueLocked(run, peer, modeMessage())
                    noticeLocked(run, peer, config.texts.playerOne)
                } else if (requested == mode) enqueueLocked(run, peer, modeMessage())
                else changeModeLocked(run, requested, byReceiver = false)
            }
            "pad" -> {
                if (mode == ReceiverMode.RetroArch) {
                    // RetroPad o Mando Wii de lado, y la plantilla de consola que enseña (solo para el panel);
                    // la pistola necesita un ratón que mover: en este Android no la hay
                    val kind = RetroPadKind.parse(message.optString("pad"))
                    val layout = layoutId(message)
                    if (peer.retroLayout != layout) { peer.retroLayout = layout; run.eventDirty = true }
                    if (kind == RetroPadKind.Gun) noticeLocked(run, peer, config.texts.retroNoGun)
                    else if (kind != null && kind != peer.retroPad) {
                        run.retro.release(peer.slot)
                        peer.retroReleased = true
                        peer.retroPad = kind
                        run.eventDirty = true
                    }
                }
                enqueueLocked(run, peer, padMessage(peer))
            }
            "hotkey" -> {
                // Tecla rápida de RetroArch por nombre (PROTOCOL.md §3); las de mantener llevan down true/false
                val name = message.optString("name")
                val down = message.optBoolean("down", true)
                val ok = mode == ReceiverMode.RetroArch && run.retro.hotkey(peer.slot, name, down)
                enqueueLocked(run, peer, JSONObject().put("m", "hotkey").put("name", name).put("down", down).put("ok", ok))
            }
            "nunchuk" -> {
                val own = message.optBoolean("own", false)
                if (peer.ownNunchuk != own) {
                    peer.ownNunchuk = own
                    neutralizeLocked(run, peer, now)
                    run.eventDirty = true
                }
                enqueueLocked(run, peer, JSONObject().put("m", "nunchuk").put("own", own))
            }
            "screen_only" -> enqueueLocked(run, peer, JSONObject().put("m", "screen_only").put("on", false))
            "text" -> noticeLocked(run, peer, config.texts.textUnavailable)
            "screen" -> noticeLocked(run, peer, config.texts.screenUnavailable)
            // Unknown additive messages are ignored, per PMP v1.
        }
    }

    private fun authenticateLocked(run: Run, peer: Peer, message: JSONObject, now: Long) {
        if (message.optString("m") != "hello") { rejectLocked(run, peer, "unsupported", "Send a PMP hello first; screen streaming is unavailable"); return }
        if (message.optInt("pv", -1) != 1) { rejectLocked(run, peer, "bad_version", "Update PepoMote on both devices"); return }
        val window = run.failures[peer.address]?.takeIf { now - it.started < 60 * SECOND }
        if (window != null && window.attempts >= 5) { rejectLocked(run, peer, "rate_limited", "Too many pairing attempts; retry in one minute"); return }
        // Every comparison uses equal-length digests and all three comparisons execute.
        val presentedToken = hash(message.optString("token", ""))
        val presentedCode = hash(message.optString("code", ""))
        val authenticated = MessageDigest.isEqual(tokenHash, presentedToken) or
            MessageDigest.isEqual(codeHash, presentedToken) or MessageDigest.isEqual(codeHash, presentedCode)
        if (!authenticated) {
            if (run.failures.size >= 64 && !run.failures.containsKey(peer.address)) run.failures.remove(run.failures.keys.first())
            val failed = window ?: FailureWindow(now).also { run.failures[peer.address] = it }
            failed.attempts++
            rejectLocked(run, peer, if (message.has("code")) "bad_code" else "bad_token", "Pairing token or displayed code is incorrect")
            return
        }
        run.failures.remove(peer.address)
        if (message.optString("role", "wiimote") != "wiimote") {
            rejectLocked(run, peer, "unsupported_role", "Use a main controller with its own Nunchuk on this Android receiver")
            return
        }
        if (message.optBoolean("probe", false)) {
            enqueueLocked(run, peer, okMessage(peer, 0).put("token", config.token))
            peer.closeAfterWrite = true
            return
        }
        val slot = (0..3).firstOrNull { candidate -> run.sessions.values.none { it.slot == candidate } }
        if (slot == null) { rejectLocked(run, peer, "busy", "All four players are connected"); return }
        var session: Int
        do { session = random.nextInt() } while (session == 0 || run.sessions.containsKey(session))
        peer.slot = slot
        peer.session = session
        peer.name = message.optString("name", "Controller").filter { !it.isISOControl() }.take(80).ifBlank { "Controller" }
        peer.ownNunchuk = message.optString("nunchuk", "none") == "own"
        RetroPadKind.parse(message.optString("pad"))?.takeIf { it != RetroPadKind.Gun }?.let { peer.retroPad = it }
        peer.retroLayout = layoutId(message)
        peer.lastTraffic = now
        run.sessions[session] = peer
        enqueueLocked(run, peer, okMessage(peer, session))
        // Qué juego tiene RetroArch (o null): siempre la primera línea tras `ok`, en cualquier modo
        enqueueLocked(run, peer, RetroHistory.gameMessage(retroGame))
        run.retro.setPresent(slot, mode == ReceiverMode.RetroArch)
        run.eventDirty = true
    }

    private fun okMessage(peer: Peer, session: Int) = JSONObject()
        .put("m", "ok").put("pv", 1).put("session_id", session.toLong() and 0xffffffffL)
        .put("udp_port", port).put("name", config.name).put("platform", "android")
        .put("modes", JSONArray().put("dolphin").put("switch").put("retroarch")).put("mode", mode.wire)
        .put("slot", peer.slot).put("player", peer.slot + 1).put("role", "wiimote")
        .put("pad", effectivePad(peer)).put("half", JSONObject.NULL).put("side", JSONObject.NULL)
        .put("nunchuk", if (peer.ownNunchuk) "own" else "none").put("screen_only", false)
        .put("text_input", false).put("pair_token", config.token)

    private fun effectivePad(peer: Peer) = when (mode) {
        ReceiverMode.Eden -> "pro"
        ReceiverMode.RetroArch -> peer.retroPad.wire
        ReceiverMode.Dolphin -> "wiimote"
    }
    /** `pad.layout` / `hello.layout`: un id de plantilla de consola (protocol/retro-layouts.json) o nada. */
    private fun layoutId(message: JSONObject): String? = message.optString("layout", "").takeIf { it in RetroLayouts.LAYOUT_IDS }
    private fun modeMessage(byReceiver: Boolean = false) = JSONObject().put("m", "mode").put("mode", mode.wire)
        .apply { if (byReceiver) put("by", "pc") }
    private fun padMessage(peer: Peer) = JSONObject().put("m", "pad").put("pad", effectivePad(peer))
        .put("player", peer.slot + 1).put("half", JSONObject.NULL).put("side", JSONObject.NULL)
        .put("layout", if (mode == ReceiverMode.RetroArch) peer.retroLayout ?: JSONObject.NULL else JSONObject.NULL)

    private fun changeModeLocked(run: Run, requested: ReceiverMode, byReceiver: Boolean) {
        mode = requested
        // Solo un emulador que se suscriba después de este cambio puede pedir volver por DSU.
        run.clients.values.forEach { it.pendingRetroReturn = false }
        if (requested != ReceiverMode.RetroArch) lastNonRetroMode = requested
        val now = System.nanoTime()
        for (peer in run.sessions.values.toList()) {
            neutralizeLocked(run, peer, now)
            // El enlace suelta lo que tuviera pulsado quien deja de ser mando de RetroArch
            run.retro.setPresent(peer.slot, requested == ReceiverMode.RetroArch)
            peer.retroReleased = true
            enqueueLocked(run, peer, modeMessage(byReceiver))
            enqueueLocked(run, peer, padMessage(peer))
        }
        run.eventDirty = true
        run.selector.wakeup()
    }

    private fun noticeLocked(run: Run, peer: Peer, text: String) = enqueueLocked(run, peer, JSONObject().put("m", "notice").put("text", text))
    private fun rejectLocked(run: Run, peer: Peer, code: String, message: String) {
        enqueueLocked(run, peer, JSONObject().put("m", "err").put("code", code).put("msg", message))
        peer.closeAfterWrite = true
    }

    private fun enqueueLocked(run: Run, peer: Peer, message: JSONObject) {
        if (!run.connections.containsKey(peer.channel)) return
        if (peer.outbound.size >= MAX_OUTBOUND) { disconnectLocked(run, peer); return }
        if (peer.outbound.isEmpty()) peer.lastWriteProgress = System.nanoTime()
        peer.outbound.add(ByteBuffer.wrap((message.toString() + "\n").toByteArray(Charsets.UTF_8)))
        peer.key?.takeIf { it.isValid }?.interestOps(SelectionKey.OP_READ or SelectionKey.OP_WRITE)
        run.selector.wakeup()
    }

    private fun flushLocked(run: Run, peer: Peer) {
        repeat(MAX_OUTBOUND) {
            val head = peer.outbound.peek() ?: return@repeat
            val sent = peer.channel.write(head)
            if (sent > 0) peer.lastWriteProgress = System.nanoTime()
            if (head.hasRemaining()) return
            peer.outbound.remove()
        }
        if (peer.closeAfterWrite) disconnectLocked(run, peer)
        else peer.key?.takeIf { it.isValid }?.interestOps(SelectionKey.OP_READ)
    }

    private fun disconnectLocked(run: Run, peer: Peer) {
        if (run.connections.remove(peer.channel) == null) return
        peer.session?.let { session ->
            if (run.sessions.remove(session) != null) {
                emitLocked(run, peer, System.nanoTime(), forceDisconnected = true)
                run.retro.setPresent(peer.slot, false)
                run.eventDirty = true
            }
        }
        peer.outbound.clear()
        peer.key?.cancel()
        runCatching { peer.channel.close() }
    }

    private fun inputLoop(run: Run) {
        val bytes = ByteArray(2048)
        while (active(run)) {
            val packet = DatagramPacket(bytes, bytes.size)
            try { run.pmp.receive(packet) } catch (_: PortUnreachableException) { continue }
            val wire = bytes.copyOf(packet.length)
            val now = System.nanoTime()
            if (wire.contentEquals(DISCOVER)) {
                synchronized(lock) {
                    if (current === run && run.discoveryRate.allow(now)) {
                        val response = "PMPHERE1 " + JSONObject().put("pv", 1).put("name", config.name).put("tcp", port).put("platform", "android")
                        send(run.pmp, packet.socketAddress, response.toByteArray(Charsets.UTF_8))
                    }
                }
                continue
            }
            val pingSession = ReceiverProtocol.pingSession(wire)
            if (pingSession != null) {
                synchronized(lock) {
                    if (current !== run) return
                    val peer = run.sessions[pingSession] ?: return@synchronized
                    if (peer.address == packet.address && peer.pingRate.allow(now)) {
                        peer.lastTraffic = now
                        if (wire[4].toInt() == 2) { wire[4] = 3; send(run.pmp, packet.socketAddress, wire) }
                    }
                }
                continue
            }
            val frame = ReceiverProtocol.parseInput(wire) ?: continue
            synchronized(lock) {
                if (current !== run) return
                val peer = run.sessions[frame.sessionId] ?: return@synchronized
                if (peer.address != packet.address || !peer.inputRate.allow(now)) return@synchronized
                if (peer.previousSeq?.let { !ReceiverProtocol.newerSequence(frame.seq, it) } == true) return@synchronized
                peer.previousSeq = frame.seq
                if (peer.previousRecenter != null && peer.previousRecenter != frame.recenterCount) peer.pulseUntil = now + 150_000_000L
                peer.previousRecenter = frame.recenterCount
                peer.input = if (mode == ReceiverMode.Dolphin && !peer.ownNunchuk)
                    frame.copy(buttons = frame.buttons and ((1 shl 17) or (1 shl 18)).inv(), stickX = 0, stickY = 0, stickRX = 0, stickRY = 0)
                else frame
                peer.lastInput = now; peer.lastTraffic = now; peer.frames++; peer.dirty = true
                if (mode == ReceiverMode.RetroArch) {
                    // Al mando en red de RetroArch (el enlace lo dosifica a un datagrama por fotograma)
                    run.retro.pushInput(peer.slot, peer.retroPad, frame)
                    peer.retroReleased = false
                }
                if (now - peer.lastOutput >= OUTPUT_INTERVAL) emitLocked(run, peer, now)
            }
        }
    }

    private fun dsuLoop(run: Run) {
        val bytes = ByteArray(2048)
        while (active(run)) {
            val packet = DatagramPacket(bytes, bytes.size)
            try { run.dsu.receive(packet) } catch (_: PortUnreachableException) { continue }
            if (!packet.address.isLoopbackAddress) continue
            val request = DsuCodec.parseRequest(bytes.copyOf(packet.length)) ?: continue
            synchronized(lock) {
                if (current !== run) return
                val now = System.nanoTime()
                if (!run.dsuRate.allow(now)) return@synchronized
                when (request.type) {
                    DsuCodec.VERSION -> send(run.dsu, packet.socketAddress, DsuCodec.version())
                    DsuCodec.PORT_INFO -> for (slot in request.ports) {
                        val peer = run.sessions.values.firstOrNull { it.slot == slot }
                        val connected = peer != null && peer.lastInput != 0L && now - peer.lastInput < SECOND
                        send(run.dsu, packet.socketAddress, DsuCodec.portInfo(slot, connected, peer?.input?.batteryPct ?: 0))
                    }
                    DsuCodec.PAD_DATA -> {
                        if (request.slots == 0) return@synchronized
                        expireSubscriptionsLocked(run, now)
                        if (run.clients.size >= MAX_DSU_CLIENTS && !run.clients.containsKey(packet.socketAddress)) return@synchronized
                        val sub = run.clients.getOrPut(packet.socketAddress) {
                            run.eventDirty = true
                            Subscription(pendingRetroReturn = mode == ReceiverMode.RetroArch)
                        }
                        // Si llegó antes de vencer la espera, sus renovaciones reintentan el cambio.
                        if (sub.pendingRetroReturn) emulatorAskedLocked(run, now)
                        for (slot in 0..3) if (request.slots and (1 shl slot) != 0) {
                            sub.expires[slot] = now + 3 * SECOND
                            val peer = run.sessions.values.firstOrNull { it.slot == slot }
                            if (peer != null) emitLocked(run, peer, now, only = packet.socketAddress)
                            else send(run.dsu, packet.socketAddress, DsuCodec.padData(slot, neutralInput(now / 1000), mode, false, false, ++run.counter))
                        }
                    }
                }
            }
        }
    }

    private fun sweepLocked(run: Run, now: Long) {
        expireSubscriptionsLocked(run, now)
        run.failures.entries.removeAll { now - it.value.started >= 60 * SECOND }
        for (peer in run.connections.values.toList()) {
            if ((peer.session == null && now - peer.created >= 5 * SECOND) ||
                (peer.session != null && now - peer.lastTraffic >= 5 * SECOND) ||
                (peer.line.size() > 0 && now - peer.lineStarted >= 5 * SECOND) ||
                (peer.outbound.isNotEmpty() && now - peer.lastWriteProgress >= 2 * SECOND)) {
                disconnectLocked(run, peer); continue
            }
            if (peer.session == null) continue
            if (mode == ReceiverMode.RetroArch && !peer.retroReleased && peer.lastInput != 0L && now - peer.lastInput >= RELEASE_AFTER) {
                // Sin paquetes: soltar en RetroArch (un botón pulsado seguiría pulsado para siempre)
                run.retro.release(peer.slot)
                peer.retroReleased = true
            }
            if (now - peer.lastPing >= SECOND) {
                peer.lastPing = now
                enqueueLocked(run, peer, JSONObject().put("m", "ping").put("t", now / 1000))
            }
            val connected = peer.lastInput != 0L && now - peer.lastInput < SECOND
            val neutral = peer.lastInput == 0L || now - peer.lastInput >= RELEASE_AFTER
            val touch = mode == ReceiverMode.Dolphin && connected && !neutral && now < peer.pulseUntil
            if ((peer.dirty || peer.sentConnected != connected || peer.sentNeutral != neutral || peer.sentTouch != touch) && now - peer.lastOutput >= OUTPUT_INTERVAL) {
                emitLocked(run, peer, now)
            }
            if (now - peer.rateStarted >= SECOND / 2) {
                peer.inputHz = (peer.frames - peer.rateFrames) * SECOND.toDouble() / (now - peer.rateStarted)
                peer.rateStarted = now; peer.rateFrames = peer.frames
            }
        }
    }

    private fun expireSubscriptionsLocked(run: Run, now: Long) {
        if (run.clients.entries.removeAll { entry -> entry.value.expires.none { it > now } }) run.eventDirty = true
    }

    private fun neutralizeLocked(run: Run, peer: Peer, now: Long) {
        val old = peer.input
        peer.input = neutralInput(old?.tSensorUs ?: now / 1000, old?.batteryPct ?: 0)
        peer.previousRecenter = null; peer.pulseUntil = 0L; peer.dirty = true
        emitLocked(run, peer, now)
    }

    /** Must hold lock: this orders releases against input, mode changes, and slot reuse. */
    private fun emitLocked(run: Run, peer: Peer, now: Long, forceDisconnected: Boolean = false, only: SocketAddress? = null) {
        val connected = !forceDisconnected && peer.lastInput != 0L && now - peer.lastInput < SECOND
        val neutral = forceDisconnected || peer.lastInput == 0L || now - peer.lastInput >= RELEASE_AFTER
        val raw = peer.input ?: neutralInput(now / 1000)
        // En modo RetroArch los mandos van al mando en red, nunca al DSU
        val sample = if (neutral || mode == ReceiverMode.RetroArch) neutralInput(raw.tSensorUs + (if (peer.lastInput == 0L) 0 else (now - peer.lastInput) / 1000), raw.batteryPct) else raw
        val touch = mode == ReceiverMode.Dolphin && connected && !neutral && now < peer.pulseUntil
        val destinations = if (only != null) listOf(only) else run.clients.entries.filter { it.value.expires[peer.slot] > now }.map { it.key }
        if (destinations.isNotEmpty()) {
            val wire = DsuCodec.padData(peer.slot, sample, mode, connected, touch, ++run.counter)
            for (address in destinations) send(run.dsu, address, wire)
        }
        if (only == null) {
            peer.lastOutput = now; peer.sentConnected = connected; peer.sentNeutral = neutral; peer.sentTouch = touch; peer.dirty = false
        }
    }

    /** El juego que RetroArch tiene cargado (lo lee el servicio de la carpeta RetroArch): se difunde a los móviles. */
    fun setGame(game: RetroGameInfo?) {
        synchronized(lock) {
            if (retroGame == game) return
            retroGame = game
            val run = current ?: return
            val message = RetroHistory.gameMessage(game)
            for (peer in run.sessions.values.toList()) enqueueLocked(run, peer, message)
            run.eventDirty = true
        }
        publish(snapshot())
    }

    /**
     * El enlace con RetroArch cambia de estado (hilo del enlace). Cuando RetroArch
     * empieza a responder (está delante, con la red activada), el receptor pasa
     * solo al modo RetroArch, como el modo automático del PC; solo en el flanco,
     * para que el jugador 1 pueda elegir otro modo con RetroArch abierto.
     */
    private fun retroLiveChanged(live: RetroLive) {
        synchronized(lock) {
            val run = current ?: return
            val before = retroLive
            retroLive = live
            if (!live.reachable && before.reachable) retroLostAt = System.nanoTime()
            if (live.reachable && !before.reachable && mode != ReceiverMode.RetroArch) {
                changeModeLocked(run, ReceiverMode.RetroArch, byReceiver = true)
                for (peer in run.sessions.values.toList()) noticeLocked(run, peer, config.texts.retroOpened)
            }
            run.eventDirty = true
        }
    }

    /**
     * Un emulador nuevo pide los mandos por DSU (Dolphin o Eden acaba de abrirse)
     * mientras RetroArch lleva un rato sin responder: se vuelve al último modo
     * que no era RetroArch.
     */
    private fun emulatorAskedLocked(run: Run, now: Long) {
        if (mode != ReceiverMode.RetroArch || retroLive.reachable) return
        if (retroLostAt != 0L && now - retroLostAt < 2 * SECOND) return
        val back = lastNonRetroMode
        changeModeLocked(run, back, byReceiver = true)
        val text = String.format(config.texts.retroBack, config.texts.modeName(back))
        for (peer in run.sessions.values.toList()) noticeLocked(run, peer, text)
    }

    private fun send(socket: DatagramSocket, target: SocketAddress, bytes: ByteArray) {
        try { socket.send(DatagramPacket(bytes, bytes.size, target)) } catch (_: IOException) { }
    }

    private companion object {
        const val SECOND = 1_000_000_000L
        const val RELEASE_AFTER = 250_000_000L
        const val OUTPUT_INTERVAL = 4_000_000L
        const val MAX_CONNECTIONS = 8
        const val MAX_OUTBOUND = 32
        const val MAX_LINE = 4096
        const val MAX_DSU_CLIENTS = 32
        val LOOPBACK: InetAddress = InetAddress.getByName("127.0.0.1")
        val DISCOVER = "PMPDISCOVER1".toByteArray(Charsets.US_ASCII)
        fun hash(value: String): ByteArray = MessageDigest.getInstance("SHA-256").digest(value.toByteArray(Charsets.UTF_8))
        fun neutralInput(t: Long, battery: Int = 0) = ReceiverInput(0, 0, t, 0f, 0f, 0f, 0f, 0f, 0f, 0, 0, battery, 0, 0, 0, 0, 0)
        fun boundedJsonDepth(text: String): Boolean {
            var depth = 0; var quoted = false; var escaped = false
            for (char in text) {
                if (escaped) { escaped = false; continue }
                if (quoted && char == '\\') { escaped = true; continue }
                if (char == '"') { quoted = !quoted; continue }
                if (!quoted) {
                    if (char == '{' || char == '[') { depth++; if (depth > 8) return false }
                    if (char == '}' || char == ']') depth--
                }
            }
            return true
        }
    }
}
