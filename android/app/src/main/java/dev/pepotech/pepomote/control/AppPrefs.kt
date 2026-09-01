package dev.pepotech.pepomote.control

import android.content.Context

/** Preferencias simples de la app (aparte del emparejamiento). */
object AppPrefs {
    private const val PREFS = "app"

    fun volDownIsB(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("volDownB", true)

    fun setVolDownIsB(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("volDownB", value).apply()
    }

    fun onboarded(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("onboarded", false)

    fun setOnboarded(context: Context) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("onboarded", true).apply()
    }
}
