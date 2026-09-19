package dev.pepotech.pepomote.control

import android.content.Context
import androidx.core.content.edit
import dev.pepotech.pepomote.service.PadPreference
import dev.pepotech.pepomote.service.RetroLayoutChoice

/** Preferencias simples de la app (aparte del emparejamiento). */
object AppPrefs {
    private const val PREFS = "app"

    fun pad(context: Context, mode: String): String {
        val key = PadPreference.key(mode) ?: return PadPreference.normalize(mode, null)
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val saved = prefs.getString(key, null)
        val normalized = PadPreference.normalize(mode, saved)
        if (saved != normalized) prefs.edit { putString(key, normalized) }
        return normalized
    }

    fun setPad(context: Context, mode: String, pad: String) {
        val key = PadPreference.key(mode) ?: return
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit {
            putString(key, PadPreference.normalize(mode, pad))
        }
    }

    /** RetroArch: mando de consola elegido a mano (JSON de [RetroLayoutChoice]). */
    fun retroLayoutChoice(context: Context): RetroLayoutChoice =
        RetroLayoutChoice.decode(context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString("retroLayouts", null))

    fun setRetroLayoutChoice(context: Context, choice: RetroLayoutChoice) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit { putString("retroLayouts", choice.encode()) }
    }

    /**
     * El botón «Teclado» del modo puntero hace antes un clic izquierdo donde
     * apunta el usuario, para dejar el cursor dentro del campo. Apagado, el
     * clic lo da el usuario con A antes de escribir. De serie, encendido.
     */
    fun keyboardClickFirst(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean("kbClickFirst", true)

    fun setKeyboardClickFirst(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("kbClickFirst", value).apply()
    }

    fun volDownIsB(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("volDownB", true)

    fun setVolDownIsB(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("volDownB", value).apply()
    }

    /**
     * Nunchuk en el mismo móvil (modo Dolphin): el mando lleva su propio
     * Nunchuk y en apaisado sale el trazado con stick, C y Z. Apagado de
     * serie (entrar en Dolphin nunca lo enciende solo); se recuerda porque
     * cambiarlo obliga a reabrir Dolphin.
     */
    fun ownNunchuk(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("ownNunchuk", false)

    /**
     * Avisos del receptor en pantalla («Dolphin configurado…», «Cemu está
     * abierto…»): unos segundos sobre el mando al cambiar de modo. Apagados,
     * no se enseñan (los avisos locales de una petición fallida, sí).
     */
    fun receiverNotices(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("receiverNotices", true)

    fun setReceiverNotices(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("receiverNotices", value).apply()
    }

    fun setOwnNunchuk(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("ownNunchuk", value).apply()
    }

    const val MOTION_AUTO = ""
    const val MOTION_GYRO = "gyro"
    const val MOTION_ACCEL = "accel"

    /**
     * Sensor con el que el puntero mueve el cursor: "" (automático: giroscopio
     * si es real, si no acelerómetro), "gyro" o "accel"
     * ([dev.pepotech.pepomote.sensor.MotionSource]).
     */
    fun motionSource(context: Context): String =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getString("motionSource", MOTION_AUTO) ?: MOTION_AUTO

    fun setMotionSource(context: Context, value: String) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putString("motionSource", value).apply()
    }

    /** El aviso de «sin giroscopio real» del modo puntero ya se enseñó (una vez por instalación). */
    fun gyroWarnShown(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("gyroWarnShown", false)

    fun setGyroWarnShown(context: Context) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("gyroWarnShown", true).apply()
    }

    /**
     * Lado de un mando apaisado fijo ([dev.pepotech.pepomote.service.LandscapeSide]):
     * "" hasta que se elige la primera vez; "left", "right" o "sensor"
     * (Ajustes). [key]: "nunchukSide" (mando + Nunchuk) o "gamePadSide" (GamePad).
     */
    fun landscapeSide(context: Context, key: String): String =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getString(key, "") ?: ""

    fun setLandscapeSide(context: Context, key: String, value: String) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putString(key, value).apply()
    }

    /** GamePad de Wii U sin pantalla táctil (ni doble pantalla): los botones crecen (Ajustes). */
    fun gamePadNoScreen(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("gamePadNoScreen", false)

    fun setGamePadNoScreen(context: Context, value: Boolean) {
        val e = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean("gamePadNoScreen", value)
        // excluyente con la pantalla completa
        if (value) e.putBoolean("gamePadFullScreen", false)
        e.apply()
    }

    /**
     * Pantalla del GamePad a pantalla completa: solo la pantalla de Cemu y el
     * táctil, sin sticks ni botones (el mando real va en el PC). Ajustes.
     */
    fun gamePadFullScreen(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("gamePadFullScreen", false)

    fun setGamePadFullScreen(context: Context, value: Boolean) {
        val e = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean("gamePadFullScreen", value)
        // excluyente con «GamePad sin pantalla táctil»
        if (value) e.putBoolean("gamePadNoScreen", false)
        e.apply()
    }

    /** En pantalla completa, botón de teclado arriba a la derecha (Ajustes). */
    fun gamePadFullScreenKeyboard(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("gamePadFullScreenKeyboard", true)

    fun setGamePadFullScreenKeyboard(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("gamePadFullScreenKeyboard", value).apply()
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

    /**
     * «Pulsar deslizando»: el botón por el que pasa el dedo se pulsa; al
     * salir se suelta y se pulsa el siguiente (un dedo que nace en el vacío
     * también pulsa al entrar). Apagado de serie; encendido manda sobre
     * [stickyPress].
     */
    fun slidePress(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("slidePress", false)

    fun setSlidePress(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("slidePress", value).apply()
        PressMode.slide = value
    }

    /**
     * «Mantener al salir del botón»: un botón pulsado sigue pulsado mientras
     * no se levante el dedo, aunque se salga (el «2» de Mario Kart). Encendido
     * de serie: iOS y Linux ya eran así.
     */
    fun stickyPress(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean("stickyPress", true)

    fun setStickyPress(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit().putBoolean("stickyPress", value).apply()
        PressMode.sticky = value
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

    /** Consultar versiones y novedades en GitHub al inicio y cada hora de uso (Ajustes). */
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
