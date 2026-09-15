package dev.pepotech.pepomote.server

import android.content.Context
import android.net.Uri
import android.os.Build
import dev.pepotech.pepomote.server.core.ReceiverConfig
import java.security.SecureRandom
import java.security.MessageDigest
import java.util.Base64

/** The QR credential survives a normal stop; the visible pairing code does not. */
internal object ServerIdentity {
    private val random = SecureRandom()

    @Synchronized
    fun config(context: Context): ReceiverConfig {
        val prefs = context.getSharedPreferences("android_receiver", Context.MODE_PRIVATE)
        val token = prefs.getString("token", null) ?: Base64.getUrlEncoder().withoutPadding()
            .encodeToString(ByteArray(32).also(random::nextBytes)).also {
                check(prefs.edit().putString("token", it).commit()) { "Could not save receiver identity" }
            }
        val code = random.nextInt(1_000_000).toString().padStart(6, '0')
        val suffix = MessageDigest.getInstance("SHA-256").digest(token.toByteArray(Charsets.UTF_8))
            .take(2).joinToString("") { "%02X".format(it.toInt() and 255) }
        return ReceiverConfig("PepoMote · ${Build.MODEL.take(48)} · $suffix", token, code)
    }

    fun pairUrl(config: ReceiverConfig, host: String): String = Uri.Builder()
        .scheme("pepomote").authority("pair")
        .appendQueryParameter("v", "1").appendQueryParameter("host", host)
        .appendQueryParameter("port", config.port.toString()).appendQueryParameter("t", config.token)
        .appendQueryParameter("name", config.name).appendQueryParameter("platform", "android")
        .build().toString()

}
