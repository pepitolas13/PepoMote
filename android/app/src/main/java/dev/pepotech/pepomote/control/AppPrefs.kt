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

    /** Mostrar el selector Puntero/Dolphin en el mando al entrar por Conectar. */
    fun showDolphinChips(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("showDolphinChips", true)

    fun setShowDolphinChips(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("showDolphinChips", value).apply()
    }

    fun soundsEnabled(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("sounds", true)

    fun setSoundsEnabled(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("sounds", value).apply()
        UiSounds.enabled = value
    }

    const val LANG_SYSTEM = "system"

    /** Idioma: "system", "es" o "en". */
    fun lang(context: Context): String =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getString("lang", LANG_SYSTEM) ?: LANG_SYSTEM

    fun setLang(context: Context, code: String) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putString("lang", code).apply()
    }

    fun onboarded(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("onboarded", false)

    fun setOnboarded(context: Context) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("onboarded", true).apply()
    }
}
