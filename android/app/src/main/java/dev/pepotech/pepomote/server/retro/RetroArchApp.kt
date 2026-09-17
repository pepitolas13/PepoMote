package dev.pepotech.pepomote.server.retro

import android.content.Context
import android.content.Intent

/**
 * RetroArch instalado en este Android: qué paquete es, cómo abrirlo y si ya
 * respondió alguna vez a PepoMote (confirma los comandos de red; el mando en
 * red se activa aparte y no devuelve confirmación). Los ajustes se cambian desde
 * su propio menú: su configuración vive en `Android/data`, fuera del alcance
 * de cualquier otra app).
 */
object RetroArchApp {
    /** Los `applicationId` oficiales: el de retroarch.com de 64 bits, el de Google Play y el de 32 bits. */
    val PACKAGES = listOf("com.retroarch.aarch64", "com.retroarch", "com.retroarch.ra32")
    const val DOWNLOAD_URL = "https://www.retroarch.com/?page=platforms"
    private const val PREFS = "retroarch_setup"
    private const val KEY_VERSION = "verified_version"

    /** El primer paquete instalado que se puede abrir (hace falta `<queries>` en el manifiesto). */
    fun installedPackage(context: Context): String? =
        PACKAGES.firstOrNull { runCatching { context.packageManager.getLaunchIntentForPackage(it) }.getOrNull() != null }

    fun launchIntent(context: Context): Intent? =
        installedPackage(context)?.let { context.packageManager.getLaunchIntentForPackage(it) }

    /** Versión con la que RetroArch respondió por última vez a este servidor (null = nunca). */
    fun verifiedVersion(context: Context): String? =
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY_VERSION, null)

    fun markVerified(context: Context, version: String?) {
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putString(KEY_VERSION, version?.takeIf { it.isNotBlank() } ?: "?").apply()
    }

    fun forgetVerified(context: Context) {
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().remove(KEY_VERSION).apply()
    }
}
