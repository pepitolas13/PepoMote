package dev.pepotech.pepomote.control

import android.content.Context
import android.content.res.Configuration
import java.util.Locale

/**
 * Idioma de la app: el del sistema (inglés si lo es; español para todo lo
 * demás, que es el idioma por defecto de los recursos) o el elegido con el
 * botón ES/EN del inicio. Se aplica envolviendo el contexto de la actividad
 * y del servicio (`attachBaseContext`).
 */
object LocaleHelper {
    fun wrap(base: Context): Context {
        val code = AppPrefs.lang(base)
        if (code == AppPrefs.LANG_SYSTEM) return base
        val locale = Locale(code)
        Locale.setDefault(locale)
        val config = Configuration(base.resources.configuration)
        config.setLocale(locale)
        return base.createConfigurationContext(config)
    }

    /** Código activo de verdad (solo hay español e inglés: lo que no es inglés se ve en español). */
    fun activeCode(context: Context): String =
        if (context.resources.configuration.locales[0].language == "en") "en" else "es"

    /** El otro idioma, para el botón. */
    fun otherCode(context: Context): String = if (activeCode(context) == "en") "es" else "en"

    fun name(code: String): String = if (code == "en") "English" else "Español"
}
