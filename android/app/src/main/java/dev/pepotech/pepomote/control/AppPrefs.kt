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

    /**
     * Nunchuk en el mismo móvil (modo Dolphin): el mando lleva su propio
     * Nunchuk y en apaisado sale el trazado con stick, C y Z. Encendido por
     * defecto, como el Wiimote emulado de Dolphin.
     */
    fun ownNunchuk(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("ownNunchuk", true)

    fun setOwnNunchuk(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("ownNunchuk", value).apply()
    }

    /** GamePad de Wii U sin pantalla táctil (ni doble pantalla): los botones crecen (Ajustes). */
    fun gamePadNoScreen(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("gamePadNoScreen", false)

    fun setGamePadNoScreen(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("gamePadNoScreen", value).apply()
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

    // --- aviso de versión nueva (UpdateCheck / UpdateNotice)

    /** Consultar GitHub una vez al día si hay versión nueva (Ajustes). */
    fun updateCheckEnabled(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("updateCheck", true)

    fun setUpdateCheckEnabled(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("updateCheck", value).apply()
    }

    /** Versión anunciada que se ocultó ("" = ninguna). */
    fun updateDismissed(context: Context): String =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getString("updateDismissed", "") ?: ""

    fun setUpdateDismissed(context: Context, version: String) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putString("updateDismissed", version).apply()
    }

    /** Última consulta (ms UNIX, 0 = nunca) y última versión publicada vista. */
    fun updateLastMs(context: Context): Long =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getLong("updateLastMs", 0L)

    fun updateLatest(context: Context): String =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getString("updateLatest", "") ?: ""

    fun setUpdateResult(context: Context, nowMs: Long, latest: String) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putLong("updateLastMs", nowMs).putString("updateLatest", latest).apply()
    }
}
