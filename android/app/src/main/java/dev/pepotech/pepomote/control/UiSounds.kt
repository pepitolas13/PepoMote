package dev.pepotech.pepomote.control

import android.content.Context
import android.media.AudioAttributes
import android.media.SoundPool
import dev.pepotech.pepomote.R

/** Sonidos UI sintetizados propios (assets/sounds-src/gen_sounds.py). */
object UiSounds {
    private var pool: SoundPool? = null
    private var blipId = 0
    private var popId = 0
    private var tickId = 0
    private var connectId = 0
    private var disconnectId = 0

    /** Interruptor global (Ajustes). La háptica no se toca, solo el audio. */
    @Volatile
    var enabled = true

    fun init(context: Context) {
        if (pool != null) return
        enabled = AppPrefs.soundsEnabled(context)
        val attrs = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_ASSISTANCE_SONIFICATION)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()
        val p = SoundPool.Builder().setMaxStreams(4).setAudioAttributes(attrs).build()
        val app = context.applicationContext
        blipId = p.load(app, R.raw.ui_blip, 1)
        popId = p.load(app, R.raw.ui_pop, 1)
        tickId = p.load(app, R.raw.ui_tick, 1)
        connectId = p.load(app, R.raw.ui_connect, 1)
        disconnectId = p.load(app, R.raw.ui_disconnect, 1)
        pool = p
    }

    private fun play(id: Int, vol: Float = 0.8f) {
        if (!enabled) return
        pool?.play(id, vol, vol, 1, 0, 1f)
    }

    fun blip() = play(blipId)
    fun pop() = play(popId)
    fun tick() = play(tickId)
    fun connect() = play(connectId)
    fun disconnect() = play(disconnectId, 0.6f)
}
