package dev.pepotech.pepomote.service

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.util.Log
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import dev.pepotech.pepomote.MainActivity
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.control.RetroLayouts
import dev.pepotech.pepomote.control.LocaleHelper
import dev.pepotech.pepomote.control.UiSounds
import dev.pepotech.pepomote.net.ControlClient
import dev.pepotech.pepomote.net.Discovery
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.net.Pairing
import dev.pepotech.pepomote.net.UdpSender
import dev.pepotech.pepomote.sensor.MotionEngine
import dev.pepotech.pepomote.sensor.MotionSource
import dev.pepotech.pepomote.sensor.SenderKind
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.flow.mapNotNull
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
        /** Arrancado con startForegroundService (tile): hay que llamar a startForeground sí o sí. */
        private const val EXTRA_FOREGROUND = "foreground"

        /** El servicio vivo (mismo proceso): la actividad le dice si la app se ve. */
        @Volatile
        private var current: LinkForegroundService? = null

        /** La app está en pantalla (MainActivity entre onStart y onStop). */
        @Volatile
        private var appVisible = false

        /**
         * MainActivity: la app se ve (onStart) o deja de verse (onStop). Con
         * la app en pantalla el proceso ya es prioritario por la Activity y
         * el servicio va sin notificación; al irse a segundo plano sube a
         * primer plano (con notificación) para que Android no lo duerma.
         */
        fun setAppVisible(visible: Boolean) {
            appVisible = visible
            current?.applyVisibility()
        }
        private const val MAX_ATTEMPTS = 3

        /**
         * `role`: wiimote (mando) o nunchuk (móvil de la otra mano). Va en el
         * hello y lo conservan los reintentos; un start() con otro rol rehace
         * el enlace entero.
         */
        /**
         * Arranca (o rehace) el enlace. Desde la app (visible) es un servicio
         * normal: sin notificación mientras se vea; desde el tile
         * (`background`) o si la app ya no está en primer plano, en primer
         * plano con su notificación.
         */
        fun start(context: Context, role: String = LinkState.ROLE_WIIMOTE, background: Boolean = false) {
            dev.pepotech.pepomote.server.ServerForegroundService.stop(context)
            // "Conectando" YA, antes de que el servicio llegue a arrancar: si
            // el intento anterior acabó en Failed, la pantalla del mando aún
            // lo veía y rebotaba al inicio repitiendo el error viejo.
            LinkState.role = role
            LinkState.publish(UiLink.Connecting)
            val intent = Intent(context, LinkForegroundService::class.java).putExtra(EXTRA_ROLE, role)
            if (background) {
                context.startForegroundService(intent.putExtra(EXTRA_FOREGROUND, true))
            } else {
                try {
                    context.startService(intent)
                } catch (_: IllegalStateException) {
                    // la app no está en primer plano (Android 8+): en primer plano
                    context.startForegroundService(intent.putExtra(EXTRA_FOREGROUND, true))
                }
            }
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

    /** El modo pedido se repone al reconectar; cada consola conserva su mando en AppPrefs. */
    private var lastMode: String? = null
    private var sessionId: Int? = null

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

    /** En primer plano (con notificación) ahora mismo. */
    private var foregrounded = false

    /** Texto de la notificación (se publica solo en primer plano; si no, queda para la próxima subida). */
    private var notifText = ""

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        current = this
    }

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(LocaleHelper.wrap(newBase))
    }

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
        notifText = getString(R.string.notif_connecting, pairing.pcName)
        // Arrancado con startForegroundService: startForeground es obligatorio
        // (luego, si la app se ve, se baja al momento); si no, según se vea la app
        if (intent?.getBooleanExtra(EXTRA_FOREGROUND, false) == true) goForeground()
        applyVisibility()

        // Un start() con el enlace ya vivo (QR nuevo desde Ajustes, Reconectar)
        // reemplaza el enlace entero: antes se apilaban sensores y sockets del
        // anterior, que seguían enviando al PC viejo.
        teardownLink()
        cancelReconnect()
        phase = Phase.Initial
        lastMode = null
        sessionId = null
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

    /** Sube a primer plano con la notificación (idempotente). */
    private fun goForeground() {
        if (foregrounded) return
        try {
            ServiceCompat.startForeground(
                this, NOTIF_ID, buildNotification(notifText),
                if (Build.VERSION.SDK_INT >= 29)
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0
            )
            foregrounded = true
        } catch (e: Exception) {
            // Android 12+ puede vetarlo fuera del periodo de gracia: el enlace
            // sigue como servicio normal (con la app a la vista no hace falta)
            Log.w("PepoMote", "startForeground rechazado: ${e.message}")
        }
    }

    /**
     * Con la app en pantalla, fuera la notificación (el servicio sigue vivo,
     * el proceso es prioritario por la Activity); sin ella, a primer plano.
     */
    private fun applyVisibility() {
        if (appVisible) {
            if (foregrounded) {
                ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
                foregrounded = false
            }
        } else {
            goForeground()
        }
    }

    private fun connect(pairing: Pairing) {
        var confirmedPairing = pairing
        attempt++
        val gen = ++generation
        control = ControlClient(
            host = pairing.host,
            port = pairing.port,
            token = pairing.token,
            deviceName = Build.MODEL ?: "Android",
            deviceModel = "${Build.MANUFACTURER} ${Build.MODEL}",
            role = role,
            ownNunchuk = AppPrefs.ownNunchuk(this),
            // Stored Wii U preference, also while Switch is active. This does not open ScreenLink.
            screenOnly = AppPrefs.gamePadFullScreen(this),
            callbacks = object : ControlClient.Callbacks {
                override fun onOk(ok: ControlClient.Ok) {
                    // El handshake crea una sola pareja motor/socket por conexión.
                    if (gen != generation || phase == Phase.Live) return
                    sessionId = ok.sessionId
                    phase = Phase.Live
                    reconnectAttempt = 0
                    val nunchuk = ok.role == LinkState.ROLE_NUNCHUK
                    val pcName = ok.name.takeIf { it.isNotBlank() } ?: pairing.pcName
                    confirmedPairing = pairing.copy(pcName = pcName, platform = ok.platform,
                        token = ok.pairToken ?: pairing.token)
                    if (confirmedPairing != pairing) PairStore.confirm(this@LinkForegroundService, pairing, confirmedPairing)
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
                    // Inclinación (acelerómetro) solo si el receptor la entiende:
                    // un receptor anterior o el servidor Android descartarían el bit
                    engine.tilt = ok.supportsTilt && MotionSource.refresh(this@LinkForegroundService)
                    LinkState.setTilt = { on -> engine.tilt = ok.supportsTilt && on }
                    // Antes de publicar Connected: la pantalla GamePad lo busca al entrar
                    LinkState.motion = engine
                    engine.start()
                    LinkState.sendMode = { m ->
                        ButtonState.reset()
                        val selected = (LinkState.flow.value as? UiLink.Connected)?.let { Route.selectMode(m, it) } ?: m
                        lastMode = selected
                        control?.sendMode(selected, cemuScreenOnly = AppPrefs.gamePadFullScreen(this@LinkForegroundService))
                    }
                    // Guardar la elección de Wii U; Switch siempre usa Pro Controller.
                    LinkState.sendPad = { p ->
                        ButtonState.reset()
                        (LinkState.flow.value as? UiLink.Connected)?.mode?.let {
                            val pad = PadPreference.normalize(it, p)
                            AppPrefs.setPad(this@LinkForegroundService, it, pad)
                            control?.sendPad(pad)
                        }
                    }
                    LinkState.sendText = { t -> if (ok.textInput) control?.sendText(t) }
                    LinkState.sendNunchuk = { own -> control?.sendNunchuk(own) }
                    LinkState.sendScreenOnly = { on ->
                        if ((LinkState.flow.value as? UiLink.Connected)?.mode == LinkState.MODE_CEMU) control?.sendScreenOnly(on)
                    }
                    LinkState.sendHotkey = { name, down ->
                        if ((LinkState.flow.value as? UiLink.Connected)?.mode == LinkState.MODE_RETROARCH) control?.sendHotkey(name, down)
                    }
                    // RetroArch: la plantilla de consola elegida a mano, y el gancho para
                    // avisar al receptor cuando cambie la efectiva
                    LinkState.loadRetroLayouts(this@LinkForegroundService)
                    LinkState.sendLayout = { syncRetroLayout() }
                    LinkState.publish(
                        UiLink.Connected(
                            pcName, ok.mode, null, 0f, ok.slot,
                            role = ok.role,
                            // Receptor sin "player" en el ok: el jugador es el slot
                            player = if (ok.player > 0) ok.player else ok.slot + 1,
                            supportsCemu = ok.supportsCemu,
                            pad = ok.pad,
                            ownNunchuk = ok.nunchuk == "own",
                            screenOnly = ok.screenOnly,
                            supportsSwitch = ok.supportsSwitch,
                            platform = ok.platform,
                            textInput = ok.textInput,
                            supportsTilt = ok.supportsTilt,
                            supportsRetroArch = ok.supportsRetroArch
                        )
                    )
                    val requestedMode = LinkState.pendingMode
                    requestedMode?.let { m ->
                        LinkState.pendingMode = null
                        val selected = Route.selectMode(m, LinkState.flow.value as UiLink.Connected)
                        if (selected != m) LinkState.clearIntent()
                        lastMode = selected
                        control?.sendMode(selected, cemuScreenOnly = AppPrefs.gamePadFullScreen(this@LinkForegroundService))
                    }
                    // Reponer solo la elección del modo confirmado, después de su eco si estaba pendiente.
                    if (!nunchuk && requestedMode == null && (ok.mode == LinkState.MODE_CEMU && ok.supportsCemu || ok.mode == LinkState.MODE_SWITCH && ok.supportsSwitch || ok.mode == LinkState.MODE_RETROARCH && ok.supportsRetroArch)) {
                        restorePrefs(ok.mode)
                    }
                    // Doble pantalla: la pantalla GamePad abre el canal cuando
                    // toca; va al mismo puerto que este control. Un Nunchuk no tiene.
                    if (!nunchuk && ok.mode == LinkState.MODE_CEMU) ScreenLink.bind(pairing.host, pairing.port, ok.sessionId)
                    UiSounds.init(this@LinkForegroundService)
                    UiSounds.connect()
                    // "Pulsar la diana" automáticamente al conectar: recentra
                    // y centra el cursor con los primeros paquetes ya fluyendo
                    mainHandler.postDelayed({ ButtonState.bumpRecenter() }, 300)
                    updateNotification(
                        if (nunchuk) getString(R.string.notif_nunchuk_connected, pcName)
                        else getString(R.string.status_connected_to, pcName)
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
                        dropped(confirmedPairing)
                        return
                    }
                    // Fallos de red transitorios (el primer intento tras el
                    // escaneo suele pillar la radio saliendo de la cámara):
                    // reintenta antes de rendirse.
                    if (attempt < MAX_ATTEMPTS) {
                        // Este cliente ya está muerto: su onClosed no debe
                        // parar el servicio mientras el reintento vive
                        val retryGen = ++generation
                        updateNotification(getString(R.string.notif_retrying, attempt, MAX_ATTEMPTS))
                        Thread({
                            val next = relocate(confirmedPairing)
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
                    val before = LinkState.flow.value as? UiLink.Connected
                    if (before?.platform == "android") lastMode = mode
                    if (before?.mode != mode) ButtonState.reset()
                    LinkState.updateConnected {
                        it.copy(mode = mode, pad = PadPreference.effective(mode, it.pad))
                    }
                    LinkState.resolveIntent(this@LinkForegroundService, mode, byPc)
                    if (role == LinkState.ROLE_WIIMOTE) {
                        if (mode == LinkState.MODE_CEMU && before?.supportsCemu == true || mode == LinkState.MODE_SWITCH && before?.supportsSwitch == true || mode == LinkState.MODE_RETROARCH && before?.supportsRetroArch == true) {
                            restorePrefs(mode)
                        }
                        if (mode == LinkState.MODE_CEMU) {
                            sessionId?.let { ScreenLink.bind(pairing.host, pairing.port, it) }
                        } else ScreenLink.unbind()
                    }
                }

                override fun onPadChanged(pad: String, player: Int?) {
                    if (gen != generation) return
                    val before = LinkState.flow.value as? UiLink.Connected
                    if (before?.pad != pad || player != null && before.player != player) ButtonState.reset()
                    LinkState.updateConnected { it.copy(pad = PadPreference.effective(it.mode, pad), player = player ?: it.player) }
                    syncRetroLayout()
                }

                override fun onGameChanged(game: RetroGame?) {
                    if (gen != generation) return
                    LinkState.updateConnected { it.copy(game = game) }
                    syncRetroLayout()
                }

                override fun onNunchukChanged(own: Boolean) {
                    if (gen != generation) return
                    LinkState.updateConnected { it.copy(ownNunchuk = own) }
                }

                override fun onScreenOnlyChanged(on: Boolean) {
                    if (gen != generation) return
                    LinkState.updateConnected { it.copy(screenOnly = on) }
                }

                override fun onNotice(text: String) {
                    if (gen != generation) return
                    // Ajuste «Avisos del PC en pantalla»: apagado, ni banner
                    if (AppPrefs.receiverNotices(this@LinkForegroundService)) LinkState.publishNotice(text)
                }

                override fun onClosed() {
                    if (gen != generation) return
                    if (phase != Phase.Initial && LinkState.flow.value !is UiLink.Failed) {
                        // El PC cerró (reinicio, red caída): se rehace sola
                        dropped(confirmedPairing)
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
            LinkState.publish(UiLink.Failed("io", getString(R.string.lost_connection, pairing.pcName)))
            stopSelf()
            return
        }
        reconnectAttempt++
        LinkState.publish(UiLink.Reconnecting(pairing.pcName, reconnectAttempt))
        // Lo que el usuario había pedido se vuelve a pedir en cuanto llegue el ok
        lastMode?.takeIf { it != LinkState.MODE_POINTER }?.let { LinkState.requestMode(it) }
        updateNotification(getString(R.string.status_reconnecting, pairing.pcName))
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
        // En cuanto contesta el PC con ese nombre desde otro sitio, vale: no
        // hace falta esperar al final del sondeo
        val found = try {
            runBlocking {
                Discovery.scan(1200)
                    .mapNotNull { seen ->
                        seen.firstOrNull { it.name == pairing.pcName && (it.host != pairing.host || it.tcpPort != pairing.port) }
                    }
                    .firstOrNull()
            }
        } catch (_: Exception) {
            null
        } ?: return pairing
        val moved = pairing.copy(host = found.host, port = found.tcpPort)
        PairStore.save(this, moved)
        return moved
    }

    /** RetroArch: última plantilla de consola avisada al receptor (`pad.layout`). */
    private var lastSentLayout: String? = null

    /** Repone el mando del modo confirmado (y, en RetroArch, la plantilla que toca). */
    private fun restorePrefs(mode: String) {
        val pad = AppPrefs.pad(this, mode)
        val layout = if (mode == LinkState.MODE_RETROARCH) LinkState.wouldBeRetroLayout(LinkState.flow.value as? UiLink.Connected) else null
        lastSentLayout = if (mode == LinkState.MODE_RETROARCH && pad == LinkState.PAD_RETROPAD) layout else null
        control?.restoreModePreferences(mode, pad, cemuScreenOnly = AppPrefs.gamePadFullScreen(this), layout = layout)
    }

    /**
     * RetroArch como RetroPad: la plantilla de consola que se enseña (por el
     * juego que anunció el PC o la elegida a mano) va al receptor en cuanto
     * cambia; si cambia jugando, se suelta todo y se avisa en pantalla.
     */
    private fun syncRetroLayout() {
        val cur = LinkState.flow.value as? UiLink.Connected ?: return
        if (cur.mode != LinkState.MODE_RETROARCH || cur.pad != LinkState.PAD_RETROPAD) {
            lastSentLayout = null
            return
        }
        val eff = LinkState.effectiveRetroLayout(cur)
        if (eff == lastSentLayout) return
        if (lastSentLayout != null) {
            ButtonState.reset()
            LinkState.publishNotice(getString(R.string.retro_layout_changed, RetroLayouts.byId(eff)?.name ?: eff))
        }
        lastSentLayout = eff
        control?.sendPad(LinkState.PAD_RETROPAD, eff)
    }

    /** Cierra el enlace actual (si lo hay) e invalida sus callbacks. */
    private fun teardownLink() {
        generation++
        LinkState.sendLayout = null
        lastSentLayout = null
        LinkState.sendMode = null
        LinkState.sendPad = null
        LinkState.sendText = null
        LinkState.sendNunchuk = null
        LinkState.sendScreenOnly = null
        LinkState.sendHotkey = null
        LinkState.setTilt = null
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
        if (current === this) current = null
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
            NotificationChannel(CHANNEL_ID, getString(R.string.notif_channel), NotificationManager.IMPORTANCE_LOW)
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
            // Sin `ongoing`: en Android 13+ se puede deslizar (el servicio sigue);
            // en 12 o menos el sistema no deja quitar la de un servicio en primer
            // plano, pero ya solo existe con la app fuera de pantalla
            .setContentIntent(pi)
            .addAction(0, getString(R.string.notif_disconnect), stopPi)
            .build()
    }

    private fun updateNotification(text: String) {
        notifText = text
        if (!foregrounded) return // con la app a la vista no hay notificación
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.notify(NOTIF_ID, buildNotification(text))
    }
}
