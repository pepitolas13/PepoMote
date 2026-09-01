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
        private const val MAX_ATTEMPTS = 3

        fun start(context: Context) {
            // "Conectando" YA, antes de que el servicio llegue a arrancar: si
            // el intento anterior acabó en Failed, la pantalla del mando aún
            // lo veía y rebotaba al inicio repitiendo el error viejo.
            LinkState.publish(UiLink.Connecting)
            context.startForegroundService(Intent(context, LinkForegroundService::class.java))
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, LinkForegroundService::class.java))
        }
    }

    private val mainHandler = android.os.Handler(android.os.Looper.getMainLooper())
    private var attempt = 0

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
            callbacks = object : ControlClient.Callbacks {
                override fun onOk(sessionId: Int, udpPort: Int, mode: String, slot: Int) {
                    if (gen != generation) return
                    val sender = UdpSender(pairing.host, udpPort, sessionId) { rtt ->
                        LinkState.updateConnected { it.copy(rttMs = rtt) }
                    }
                    udp = sender
                    val engine = MotionEngine(this@LinkForegroundService, sessionId) { packet ->
                        sender.send(packet)
                    }
                    motion = engine
                    engine.start()
                    LinkState.sendMode = { m -> control?.sendMode(m) }
                    LinkState.publish(UiLink.Connected(pairing.pcName, mode, null, 0f, slot))
                    LinkState.pendingMode?.let { m ->
                        LinkState.pendingMode = null
                        control?.sendMode(m)
                    }
                    UiSounds.init(this@LinkForegroundService)
                    UiSounds.connect()
                    // "Pulsar la diana" automáticamente al conectar: recentra
                    // y centra el cursor con los primeros paquetes ya fluyendo
                    mainHandler.postDelayed({ ButtonState.bumpRecenter() }, 300)
                    updateNotification("Conectado a ${pairing.pcName}")
                }

                override fun onError(code: String, msg: String) {
                    if (gen != generation) return
                    // Fallos de red transitorios (el primer intento tras el
                    // escaneo suele pillar la radio saliendo de la cámara):
                    // reintenta antes de rendirse.
                    if (code == "io" && attempt < MAX_ATTEMPTS) {
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

                override fun onModeChanged(mode: String) {
                    if (gen != generation) return
                    LinkState.updateConnected { it.copy(mode = mode) }
                }

                override fun onClosed() {
                    if (gen != generation) return
                    if (LinkState.flow.value !is UiLink.Failed) {
                        LinkState.publish(UiLink.Disconnected)
                    }
                    stopSelf()
                }
            }
        )
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
        teardownLink()
        wakeLock?.release()
        wifiLock?.release()
        if (LinkState.flow.value is UiLink.Connecting || LinkState.flow.value is UiLink.Connected) {
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
            .setSmallIcon(R.drawable.ic_launcher_foreground)
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
