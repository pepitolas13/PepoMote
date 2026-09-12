package dev.pepotech.pepomote.service

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import dev.pepotech.pepomote.MainActivity
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.control.UiSounds
import dev.pepotech.pepomote.net.ControlClient
import dev.pepotech.pepomote.net.Discovery
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.net.Pairing
import dev.pepotech.pepomote.net.UdpSender
import dev.pepotech.pepomote.sensor.MotionEngine
import dev.pepotech.pepomote.sensor.SenderKind
import kotlinx.coroutines.runBlocking

/**
 * Mantiene vivo el enlace: canal de control TCP, socket UDP y sensores,
 * con wakelock + WifiLock de baja latencia para que Android no lo duerma.
 */
class LinkForegroundService : Service() {

    companion object {
        private const val CHANNEL_ID = "link"
        private const val NOTIF_ID = 1
        private const val ACTION_STOP = "dev.pepotech.pepomote.STOP"
        private const val EXTRA_ROLE = "role"
        private const val MAX_ATTEMPTS = 3

        /**
         * `role`: wiimote (mando) o nunchuk (móvil de la otra mano). Va en el
         * hello y lo conservan los reintentos; un start() con otro rol rehace
         * el enlace entero.
         */
        fun start(context: Context, role: String = LinkState.ROLE_WIIMOTE) {
            // "Conectando" YA, antes de que el servicio llegue a arrancar: si
            // el intento anterior acabó en Failed, la pantalla del mando aún
            // lo veía y rebotaba al inicio repitiendo el error viejo.
            LinkState.role = role
            LinkState.publish(UiLink.Connecting)
            context.startForegroundService(
                Intent(context, LinkForegroundService::class.java).putExtra(EXTRA_ROLE, role)
            )
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, LinkForegroundService::class.java))
        }
    }

    private val mainHandler = android.os.Handler(android.os.Looper.getMainLooper())
    private var attempt = 0
    private var role = LinkState.ROLE_WIIMOTE

    /**
     * Initial: aún no hubo `ok` (los fallos de red se reintentan 3 veces y
     * se rinde). Live: sesión viva. Reconnecting: la sesión se cayó y se
     * rehace sola (espera creciente, hasta dos minutos).
     */
    private enum class Phase { Initial, Live, Reconnecting }

    private var phase = Phase.Initial
    private var droppedAtMs = 0L
    private var reconnectAttempt = 0
    private var reconnectRunnable: Runnable? = null

    /** Último modo y tipo de mando pedidos por la UI: se reponen al reconectar. */
    private var lastMode: String? = null
    private var lastPad: String? = null

    /**
     * Generación del enlace. Cada connect() la sube; los callbacks de un
     * ControlClient anterior (su onClosed al cerrarlo, un reintento programado
     * tras "Salir") comparan su generación y se ignoran si ya no es la viva.
     */
    @Volatile
    private var generation = 0

    private var control: ControlClient? = null
    private var udp: UdpSender? = null
    private var motion: MotionEngine? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        val pairing = PairStore.load(this)
        if (pairing == null) {
            stopSelf()
            return START_NOT_STICKY
        }
        role = intent?.getStringExtra(EXTRA_ROLE) ?: LinkState.ROLE_WIIMOTE
        LinkState.role = role

        createChannel()
        val notif = buildNotification("Conectando con ${pairing.pcName}…")
        ServiceCompat.startForeground(
            this, NOTIF_ID, notif,
            if (Build.VERSION.SDK_INT >= 29)
                ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0
        )

        // Un start() con el enlace ya vivo (QR nuevo desde Ajustes, Reconectar)
        // reemplaza el enlace entero: antes se apilaban sensores y sockets del
        // anterior, que seguían enviando al PC viejo.
        teardownLink()
        cancelReconnect()
        phase = Phase.Initial
        lastMode = null
        lastPad = null
        if (wakeLock?.isHeld != true) acquireLocks()
        LinkState.publish(UiLink.Connecting)
        ButtonState.reset()

        attempt = 0
        connect(pairing)
        return START_NOT_STICKY
    }

    /** Cerrar/deslizar la app de recientes = desconectar. Nada de zombis. */
    override fun onTaskRemoved(rootIntent: Intent?) {
        stopSelf()
        super.onTaskRemoved(rootIntent)
    }

    private fun connect(pairing: Pairing) {
        attempt++
        val gen = ++generation
        control = ControlClient(
            host = pairing.host,
            port = pairing.port,
            token = pairing.token,
            deviceName = Build.MODEL ?: "Android",
            deviceModel = "${Build.MANUFACTURER} ${Build.MODEL}",
            role = role,
            callbacks = object : ControlClient.Callbacks {
                override fun onOk(ok: ControlClient.Ok) {
                    if (gen != generation) return
                    val recovered = phase == Phase.Reconnecting
                    phase = Phase.Live
                    reconnectAttempt = 0
                    val nunchuk = ok.role == LinkState.ROLE_NUNCHUK
                    // El PC se ha renombrado: el emparejamiento se actualiza solo
                    val pcName = ok.name.takeIf { it.isNotBlank() && it != pairing.pcName }?.also {
                        PairStore.save(this@LinkForegroundService, pairing.copy(pcName = it))
                    } ?: pairing.pcName
                    val sender = UdpSender(pairing.host, ok.udpPort, ok.sessionId) { rtt ->
                        LinkState.updateConnected { it.copy(rttMs = rtt) }
                    }
                    udp = sender
                    val engine = MotionEngine(
                        this@LinkForegroundService, ok.sessionId,
                        if (nunchuk) SenderKind.NUNCHUK else SenderKind.WIIMOTE
                    ) { packet ->
                        sender.send(packet)
                    }
                    motion = engine
                    // Antes de publicar Connected: la pantalla GamePad lo busca al entrar
                    LinkState.motion = engine
                    engine.start()
                    LinkState.sendMode = { m ->
                        lastMode = m
                        control?.sendMode(m)
                    }
                    // Cada conexión empieza como GamePad/Pro; lo pedido se
                    // recuerda para reponerlo si hay que reconectar
                    LinkState.sendPad = { p ->
                        lastPad = p
                        control?.sendPad(p)
                    }
                    LinkState.sendText = { t -> control?.sendText(t) }
                    LinkState.publish(
                        UiLink.Connected(
                            pcName, ok.mode, null, 0f, ok.slot,
                            role = ok.role,
                            // Receptor sin "player" en el ok: el jugador es el slot
                            player = if (ok.player > 0) ok.player else ok.slot + 1,
                            supportsCemu = ok.supportsCemu,
                            pad = ok.pad
                        )
                    )
                    LinkState.pendingMode?.let { m ->
                        LinkState.pendingMode = null
                        lastMode = m
                        control?.sendMode(m)
                    }
                    // Tras una caída, el Mando de Wii vuelve a serlo
                    if (recovered && lastPad == LinkState.PAD_WIIMOTE) control?.sendPad(LinkState.PAD_WIIMOTE)
                    // Doble pantalla: la pantalla GamePad abre el canal cuando
                    // toca; va al mismo puerto que este control. Un Nunchuk no tiene.
                    if (!nunchuk) ScreenLink.bind(pairing.host, pairing.port, ok.sessionId)
                    UiSounds.init(this@LinkForegroundService)
                    UiSounds.connect()
                    // "Pulsar la diana" automáticamente al conectar: recentra
                    // y centra el cursor con los primeros paquetes ya fluyendo
                    mainHandler.postDelayed({ ButtonState.bumpRecenter() }, 300)
                    updateNotification(
                        if (nunchuk) "Nunchuk conectado a $pcName"
                        else "Conectado a $pcName"
                    )
                }

                override fun onError(code: String, msg: String) {
                    if (gen != generation) return
                    if (code != "io") {
                        // El PC nos rechaza (token, código, ocupado…): no hay
                        // reintento que valga
                        LinkState.publish(UiLink.Failed(code, msg))
                        stopSelf()
                        return
                    }
                    if (phase != Phase.Initial) {
                        // La sesión estaba viva (o se estaba rehaciendo): se
                        // sigue intentando solo
                        dropped(pairing)
                        return
                    }
                    // Fallos de red transitorios (el primer intento tras el
                    // escaneo suele pillar la radio saliendo de la cámara):
                    // reintenta antes de rendirse.
                    if (attempt < MAX_ATTEMPTS) {
                        // Este cliente ya está muerto: su onClosed no debe
                        // parar el servicio mientras el reintento vive
                        val retryGen = ++generation
                        updateNotification("Reintentando conexión ($attempt/$MAX_ATTEMPTS)…")
                        Thread({
                            val next = relocate(pairing)
                            mainHandler.post { if (retryGen == generation) connect(next) }
                        }, "pepomote-retry").start()
                    } else {
                        LinkState.publish(UiLink.Failed(code, msg))
                        stopSelf()
                    }
                }

                /**
                 * Eco o difusión: el modo del receptor manda (cambia de pantalla
                 * si toca) y consume la intención Wii U pendiente, avisando si
                 * el PC no pudo (sin aviso si lo decidió el PC: su notice ya lo
                 * explica).
                 */
                override fun onModeChanged(mode: String, byPc: Boolean) {
                    if (gen != generation) return
                    LinkState.updateConnected { it.copy(mode = mode) }
                    LinkState.resolveIntent(mode, byPc)
                }

                override fun onPadChanged(pad: String) {
                    if (gen != generation) return
                    LinkState.updateConnected { it.copy(pad = pad) }
                }

                override fun onNotice(text: String) {
                    if (gen != generation) return
                    LinkState.publishNotice(text)
                }

                override fun onClosed() {
                    if (gen != generation) return
                    if (phase != Phase.Initial && LinkState.flow.value !is UiLink.Failed) {
                        // El PC cerró (reinicio, red caída): se rehace sola
                        dropped(pairing)
                        return
                    }
                    if (LinkState.flow.value !is UiLink.Failed) {
                        LinkState.publish(UiLink.Disconnected)
                    }
                    stopSelf()
                }
            }
        )
    }

    /**
     * La sesión se ha caído con el enlace vivo (o un intento de reconexión ha
     * fallado): se vuelve a intentar sola, con espera creciente, hasta rendirse
     * a los dos minutos. Las pantallas del mando se quedan («Reconectando…») y
     * lo pedido (modo Wii U o Dolphin, Mando de Wii) se repone al volver.
     */
    private fun dropped(pairing: Pairing) {
        val now = android.os.SystemClock.elapsedRealtime()
        if (phase != Phase.Reconnecting) {
            droppedAtMs = now
            reconnectAttempt = 0
        }
        phase = Phase.Reconnecting
        teardownLink()
        ButtonState.reset()
        if (Reconnect.giveUp(droppedAtMs, now)) {
            LinkState.publish(UiLink.Failed("io", Reconnect.lostMessage(pairing.pcName)))
            stopSelf()
            return
        }
        reconnectAttempt++
        LinkState.publish(UiLink.Reconnecting(pairing.pcName, reconnectAttempt))
        // Lo que el usuario había pedido se vuelve a pedir en cuanto llegue el ok
        lastMode?.takeIf { it != LinkState.MODE_POINTER }?.let { LinkState.requestMode(it) }
        updateNotification("Reconectando con ${pairing.pcName}…")
        scheduleReconnect(pairing)
    }

    private fun scheduleReconnect(pairing: Pairing) {
        val gen = generation
        val r = Runnable {
            if (gen != generation) return@Runnable
            Thread({
                val next = relocate(pairing)
                mainHandler.post { if (gen == generation) connect(next) }
            }, "pepomote-reconnect").start()
        }
        reconnectRunnable = r
        mainHandler.postDelayed(r, Reconnect.delayMs(reconnectAttempt))
    }

    private fun cancelReconnect() {
        reconnectRunnable?.let { mainHandler.removeCallbacks(it) }
        reconnectRunnable = null
    }

    /**
     * Entre reintentos, busca el PC por nombre en la red: si ha cambiado de IP
     * (DHCP, otra Wi-Fi, autoarranque antes que la red) el emparejamiento se
     * actualiza solo y el usuario no tiene que volver a escanear el QR.
     * El token no cambia: vive en el PC.
     */
    private fun relocate(pairing: Pairing): Pairing {
        val found = try {
            runBlocking { Discovery.scan(1200) }
        } catch (_: Exception) {
            emptyList()
        }.firstOrNull {
            it.name == pairing.pcName && (it.host != pairing.host || it.tcpPort != pairing.port)
        } ?: return pairing
        val moved = pairing.copy(host = found.host, port = found.tcpPort)
        PairStore.save(this, moved)
        return moved
    }

    /** Cierra el enlace actual (si lo hay) e invalida sus callbacks. */
    private fun teardownLink() {
        generation++
        LinkState.sendMode = null
        LinkState.sendPad = null
        LinkState.sendText = null
        LinkState.motion = null
        ScreenLink.unbind() // sin enlace no hay pantalla que recibir
        motion?.stop()
        udp?.close()
        control?.close()
        motion = null
        udp = null
        control = null
    }

    override fun onDestroy() {
        if (LinkState.flow.value is UiLink.Connected) {
            UiSounds.disconnect()
        }
        cancelReconnect()
        teardownLink()
        wakeLock?.release()
        wifiLock?.release()
        if (LinkState.flow.value.alive) {
            LinkState.publish(UiLink.Disconnected)
        }
        super.onDestroy()
    }

    private fun acquireLocks() {
        val pm = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "pepomote:link").apply {
            setReferenceCounted(false)
            acquire(4 * 60 * 60 * 1000L) // tope 4 h por seguridad
        }
        val wm = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        val mode = if (Build.VERSION.SDK_INT >= 29)
            WifiManager.WIFI_MODE_FULL_LOW_LATENCY else WifiManager.WIFI_MODE_FULL_HIGH_PERF
        wifiLock = wm.createWifiLock(mode, "pepomote:wifi").apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    private fun createChannel() {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "Enlace con el PC", NotificationManager.IMPORTANCE_LOW)
        )
    }

    private fun buildNotification(text: String): android.app.Notification {
        val pi = android.app.PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java),
            android.app.PendingIntent.FLAG_IMMUTABLE
        )
        val stopPi = android.app.PendingIntent.getService(
            this, 1,
            Intent(this, LinkForegroundService::class.java).setAction(ACTION_STOP),
            android.app.PendingIntent.FLAG_IMMUTABLE
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_pepomote_glyph)
            .setContentTitle("PepoMote")
            .setContentText(text)
            .setOngoing(true)
            .setContentIntent(pi)
            .addAction(0, "Desconectar", stopPi)
            .build()
    }

    private fun updateNotification(text: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.notify(NOTIF_ID, buildNotification(text))
    }
}
