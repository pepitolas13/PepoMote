package dev.pepotech.pepomote.server

import android.annotation.SuppressLint
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import dev.pepotech.pepomote.MainActivity
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.control.LocaleHelper
import dev.pepotech.pepomote.server.core.ReceiverCore
import dev.pepotech.pepomote.server.core.ReceiverConfig
import dev.pepotech.pepomote.server.core.ReceiverMode
import dev.pepotech.pepomote.server.core.ReceiverSnapshot
import dev.pepotech.pepomote.server.core.ReceiverTexts
import dev.pepotech.pepomote.server.retro.RetroArchApp
import dev.pepotech.pepomote.server.retro.RetroFolder
import dev.pepotech.pepomote.server.retro.RetroGameWatch
import dev.pepotech.pepomote.service.LinkForegroundService
import java.net.BindException
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

/** Owns the receiver while an emulator is in the foreground. No sensor dependency. */
open class ServerForegroundService : Service() {
    companion object {
        const val ACTION_OPEN = "dev.pepotech.pepomote.OPEN_SERVER"
        private const val ACTION_STOP = "dev.pepotech.pepomote.STOP_SERVER"
        private const val CHANNEL = "android_receiver"
        private const val NOTIFICATION = 3
        // Serial teardown/start even when Android replaces the Service instance.
        private val worker = Executors.newSingleThreadScheduledExecutor { r ->
            Thread(r, "PepoMote-server-lifecycle").apply { isDaemon = true }
        }
        @Volatile private var current: ServerForegroundService? = null

        fun start(context: Context) {
            LinkForegroundService.stop(context)
            context.startForegroundService(Intent(context, ServerForegroundService::class.java))
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, ServerForegroundService::class.java))
        }

        fun setMode(mode: ReceiverMode) {
            val owner = current ?: return
            worker.execute { if (owner.active) owner.core?.setMode(mode) }
        }
    }

    private val main = Handler(Looper.getMainLooper())
    @Volatile private var active = false
    private var core: ReceiverCore? = null
    private var poll: ScheduledFuture<*>? = null
    private var wakeLock: PowerManager.WakeLock? = null
    /** RetroArch: el historial de la carpeta enlazada (se relee cada 2 s) y lo último avisado. */
    private var watch: RetroGameWatch? = null
    private var watchTree: Uri? = null
    private var watchTick = 0
    @Volatile private var folderStatus: RetroFolderStatus? = null
    private var verifiedVersion: String? = null
    private var notifiedRetro = false
    private var wifiLock: WifiManager.WifiLock? = null
    private var multicastLock: WifiManager.MulticastLock? = null

    override fun attachBaseContext(newBase: Context) = super.attachBaseContext(LocaleHelper.wrap(newBase))
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        if (active) return START_NOT_STICKY
        current = this
        active = true
        try {
            foreground()
            val config = ServerIdentity.config(this).copy(texts = receiverTexts())
            ServerState.mutable.value = ServerUiState(starting = true, config = config)
            acquireLocks()
            worker.execute {
                if (!active) return@execute
                try {
                    val receiver = createReceiver(config) { state ->
                        main.post {
                            if (active) {
                                retroStateChanged(state)
                                ServerState.mutable.value = ServerState.flow.value.copy(
                                    starting = false, receiver = state, retroFolder = folderStatus,
                                    error = state.error?.let { getString(R.string.server_error_start) })
                                if (state.error != null) stopSelf()
                            }
                        }
                    }
                    core = receiver
                    receiver.start()
                    if (!active) return@execute
                    poll = worker.scheduleWithFixedDelay({
                        if (active) {
                            if (watchTick++ % 2 == 0) pollHistory(receiver)
                            val state = receiver.snapshot()
                            val addresses = ServerNetworks.addresses(this)
                            main.post {
                                if (active) ServerState.mutable.value = ServerState.flow.value.copy(
                                    starting = false, receiver = state, hosts = addresses.local, vpnHosts = addresses.vpn,
                                    retroFolder = folderStatus)
                            }
                        }
                    }, 0, 1, TimeUnit.SECONDS)
                } catch (e: Exception) {
                    main.post { fail(e) }
                }
            }
        } catch (e: Exception) {
            fail(e)
        }
        return START_NOT_STICKY
    }

    protected open fun createReceiver(config: ReceiverConfig, onState: (ReceiverSnapshot) -> Unit): ReceiverCore =
        ReceiverCore(config, onState)

    /** Los avisos del receptor, en el idioma de la app. */
    private fun receiverTexts() = ReceiverTexts(
        modesOnly = getString(R.string.server_notice_modes),
        playerOne = getString(R.string.server_notice_player_one),
        textUnavailable = getString(R.string.server_notice_text),
        screenUnavailable = getString(R.string.server_notice_screen),
        retroNoGun = getString(R.string.server_notice_retro_no_gun),
        retroOpened = getString(R.string.server_notice_retro_opened),
        retroBack = getString(R.string.server_notice_retro_back),
        retroOsdReady = getString(R.string.server_retroarch_osd),
        dolphinName = getString(R.string.mode_dolphin),
        edenName = getString(R.string.mode_eden),
    )

    /**
     * RetroArch: el juego cargado según el historial de la carpeta RetroArch
     * enlazada (hilo del servidor, cada 2 s). Sin carpeta, sin juego. Se
     * relee al cambiar el archivo o cuando RetroArch vuelve a responder.
     */
    private fun pollHistory(receiver: ReceiverCore) {
        val tree = RetroFolder.granted(this)
        if (tree != watchTree) {
            watchTree = tree
            watch = tree?.let { RetroGameWatch(RetroFolder.source(contentResolver, it)) }
            folderStatus = null
            receiver.setGame(null)
        }
        val w = watch ?: return
        if (w.poll(receiver.snapshot().retro.reachable)) receiver.setGame(w.game)
        folderStatus = RetroFolderStatus(w.located)
    }

    /** RetroArch ha respondido: se recuerda (la guía queda hecha) y la notificación lo dice. */
    private fun retroStateChanged(state: ReceiverSnapshot) {
        val version = state.retro.version
        if (state.retro.reachable && version != null && version != verifiedVersion) {
            verifiedVersion = version
            RetroArchApp.markVerified(this, version)
        }
        val retroNow = state.running && state.mode == ReceiverMode.RetroArch && state.retro.reachable
        if (retroNow != notifiedRetro) {
            notifiedRetro = retroNow
            runCatching { getSystemService(NotificationManager::class.java).notify(NOTIFICATION, notification(retroNow)) }
        }
    }

    private fun fail(error: Exception) {
        if (!active) return
        val message = getString(if (error is BindException || error.cause is BindException)
            R.string.server_error_port else R.string.server_error_start)
        ServerState.mutable.value = ServerState.flow.value.copy(starting = false, receiver = null, error = message)
        stopSelf()
    }

    private fun foreground() {
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.channel_server), NotificationManager.IMPORTANCE_LOW))
        ServiceCompat.startForeground(this, NOTIFICATION, notification(retroArch = false),
            if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0)
    }

    /** La notificación fija; con RetroArch delante y respondiendo lo dice (se ve desde RetroArch). */
    private fun notification(retroArch: Boolean): android.app.Notification {
        val open = PendingIntent.getActivity(this, 30,
            Intent(this, MainActivity::class.java).setAction(ACTION_OPEN)
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val stop = PendingIntent.getService(this, 31,
            Intent(this, ServerForegroundService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_pepomote_glyph).setContentTitle(getString(R.string.server_notification_title))
            .setContentText(getString(if (retroArch) R.string.server_notification_retroarch else R.string.server_notification_body))
            .setContentIntent(open)
            .setOngoing(true).setOnlyAlertOnce(true).setCategory(NotificationCompat.CATEGORY_SERVICE)
            .addAction(0, getString(R.string.server_stop), stop).build()
    }

    @Suppress("DEPRECATION")
    @SuppressLint("WakelockTimeout") // The explicit foreground session owns this lock; onDestroy releases it on every exit.
    private fun acquireLocks() {
        wakeLock = getSystemService(PowerManager::class.java).newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK, "PepoMote:server").apply {
            setReferenceCounted(false)
            // Released when the foreground server stops, including failed startup.
            acquire()
        }
        // Some OEMs reject optional Wi-Fi optimisations. Networking still works without them.
        val wifi = applicationContext.getSystemService(WifiManager::class.java) ?: return
        runCatching {
            wifiLock = wifi.createWifiLock(if (Build.VERSION.SDK_INT >= 29)
                WifiManager.WIFI_MODE_FULL_LOW_LATENCY else WifiManager.WIFI_MODE_FULL_HIGH_PERF,
                "PepoMote:server").apply { setReferenceCounted(false); acquire() }
        }
        runCatching {
            multicastLock = wifi.createMulticastLock("PepoMote:server-discovery")
                .apply { setReferenceCounted(false); acquire() }
        }
    }

    override fun onDestroy() {
        active = false
        if (current === this) current = null
        poll?.cancel(false)
        worker.execute {
            poll?.cancel(false)
            poll = null
            core?.close()
            core = null
            watch = null
            watchTree = null
            folderStatus = null
        }
        main.removeCallbacksAndMessages(null)
        runCatching { wakeLock?.takeIf { it.isHeld }?.release() }
        runCatching { wifiLock?.takeIf { it.isHeld }?.release() }
        runCatching { multicastLock?.takeIf { it.isHeld }?.release() }
        ServerState.mutable.value = ServerUiState(error = ServerState.flow.value.error)
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        super.onDestroy()
    }
}
