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
import dev.pepotech.pepomote.net.ControlClient
import dev.pepotech.pepomote.net.PairStore
import dev.pepotech.pepomote.net.UdpSender
import dev.pepotech.pepomote.sensor.MotionEngine

/**
 * Mantiene vivo el enlace: canal de control TCP, socket UDP y sensores,
 * con wakelock + WifiLock de baja latencia para que Android no lo duerma.
 */
class LinkForegroundService : Service() {

    companion object {
        private const val CHANNEL_ID = "link"
        private const val NOTIF_ID = 1

        fun start(context: Context) {
            context.startForegroundService(Intent(context, LinkForegroundService::class.java))
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, LinkForegroundService::class.java))
        }
    }

    private var control: ControlClient? = null
    private var udp: UdpSender? = null
    private var motion: MotionEngine? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
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

        acquireLocks()
        LinkState.publish(UiLink.Connecting)
        ButtonState.reset()

        control = ControlClient(
            host = pairing.host,
            port = pairing.port,
            token = pairing.token,
            deviceName = Build.MODEL ?: "Android",
            deviceModel = "${Build.MANUFACTURER} ${Build.MODEL}",
            callbacks = object : ControlClient.Callbacks {
                override fun onOk(sessionId: Int, udpPort: Int, mode: String) {
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
                    LinkState.publish(UiLink.Connected(pairing.pcName, mode, null, 0f))
                    LinkState.pendingMode?.let { m ->
                        LinkState.pendingMode = null
                        control?.sendMode(m)
                    }
                    updateNotification("Conectado a ${pairing.pcName}")
                }

                override fun onError(code: String, msg: String) {
                    LinkState.publish(UiLink.Failed(code, msg))
                    stopSelf()
                }

                override fun onModeChanged(mode: String) {
                    LinkState.updateConnected { it.copy(mode = mode) }
                }

                override fun onClosed() {
                    if (LinkState.flow.value !is UiLink.Failed) {
                        LinkState.publish(UiLink.Disconnected)
                    }
                    stopSelf()
                }
            }
        )
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        LinkState.sendMode = null
        motion?.stop()
        udp?.close()
        control?.close()
        motion = null
        udp = null
        control = null
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
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle("PepoMote")
            .setContentText(text)
            .setOngoing(true)
            .setContentIntent(pi)
            .build()
    }

    private fun updateNotification(text: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.notify(NOTIF_ID, buildNotification(text))
    }
}
