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
        return base.createConfigurationContext(override(locale))
    }

    /**
     * Sobrescritura que SOLO fija el idioma: un delta con todo lo demás sin
     * definir. Android la vuelve a aplicar, campo a campo y solo los
     * definidos, encima de cada configuración nueva de la actividad (girar
     * el móvil, tema oscuro, tamaño de letra…). Con una copia completa de
     * la configuración del momento (orientación, anchos en dp, modo
     * noche…) esos campos quedaban clavados en lo que había al abrir la
     * app: con el idioma elegido a mano, al girar el móvil la ventana
     * giraba pero los recursos (y `LocalConfiguration`) seguían en
     * vertical, así que el mando no pasaba a apaisado, y el tema del
     * sistema tampoco cambiaba en caliente.
     */
    fun override(locale: Locale): Configuration = Configuration().apply {
        // Un Configuration() ya nace sin definir (fontScale 0); se deja
        // explícito porque con 1 pisaría el tamaño de letra del sistema.
        fontScale = 0f
        setLocale(locale)
    }

    /** Código activo de verdad (solo hay español e inglés: lo que no es inglés se ve en español). */
    fun activeCode(context: Context): String =
        if (context.resources.configuration.locales[0].language == "en") "en" else "es"

    /** El otro idioma, para el botón. */
    fun otherCode(context: Context): String = if (activeCode(context) == "en") "es" else "en"

    fun name(code: String): String = if (code == "en") "English" else "Español"
}
