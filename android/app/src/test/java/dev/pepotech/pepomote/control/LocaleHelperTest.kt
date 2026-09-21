package dev.pepotech.pepomote.control

import android.content.Context
import android.content.res.Configuration
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * El contexto con el idioma elegido a mano (botón ES/EN) no puede clavar la
 * orientación: Android vuelve a aplicar la sobrescritura encima de cada
 * configuración nueva de la actividad (girar el móvil, tema, letra), así
 * que tiene que ser un delta con solo el idioma. Con la copia completa de
 * antes, en un A32 con el idioma elegido la pantalla giraba (giro automático
 * o botón «Girar la pantalla») pero el mando seguía en vertical.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class LocaleHelperTest {
    /** La sobrescritura solo trae el idioma: lo demás, sin definir. */
    @Test fun laSobrescrituraSoloFijaElIdioma() {
        val o = LocaleHelper.override(Locale("en"))
        assertEquals("en", o.locales[0].language)
        assertEquals(Configuration.ORIENTATION_UNDEFINED, o.orientation)
        assertEquals(Configuration.SCREEN_WIDTH_DP_UNDEFINED, o.screenWidthDp)
        assertEquals(Configuration.SCREEN_HEIGHT_DP_UNDEFINED, o.screenHeightDp)
        assertEquals(Configuration.SMALLEST_SCREEN_WIDTH_DP_UNDEFINED, o.smallestScreenWidthDp)
        assertEquals(Configuration.UI_MODE_TYPE_UNDEFINED, o.uiMode)
        assertEquals(Configuration.DENSITY_DPI_UNDEFINED, o.densityDpi)
        assertEquals(0f, o.fontScale, 0f)
    }

    /**
     * Girar el móvil con el idioma elegido: la configuración nueva
     * (apaisada, tema oscuro, letra grande) sigue igual tras aplicarle la
     * sobrescritura como hace Android al recomponer los recursos de la
     * actividad; solo cambia el idioma. Antes volvía a vertical con el
     * ancho, el tema y la letra de cuando se abrió la app.
     */
    @Test fun girarElMovilConElIdiomaElegidoSigueApaisado() {
        val landscape = Configuration().apply {
            setToDefaults()
            setLocale(Locale("es"))
            orientation = Configuration.ORIENTATION_LANDSCAPE
            screenWidthDp = 780
            screenHeightDp = 360
            smallestScreenWidthDp = 360
            uiMode = Configuration.UI_MODE_TYPE_NORMAL or Configuration.UI_MODE_NIGHT_YES
            fontScale = 1.3f
            densityDpi = 420
        }
        val rebased = Configuration(landscape).apply { updateFrom(LocaleHelper.override(Locale("en"))) }
        assertEquals("en", rebased.locales[0].language)
        assertEquals(Configuration.ORIENTATION_LANDSCAPE, rebased.orientation)
        assertEquals(780, rebased.screenWidthDp)
        assertEquals(360, rebased.screenHeightDp)
        assertEquals(360, rebased.smallestScreenWidthDp)
        assertEquals(Configuration.UI_MODE_NIGHT_YES, rebased.uiMode and Configuration.UI_MODE_NIGHT_MASK)
        assertEquals(1.3f, rebased.fontScale, 0f)
        assertEquals(420, rebased.densityDpi)
    }

    /** Con el idioma del sistema no se envuelve nada; con uno elegido, el contexto habla ese idioma y nada más cambia. */
    @Test fun envolverAplicaSoloElIdiomaElegido() {
        val context: Context = RuntimeEnvironment.getApplication()
        AppPrefs.setLang(context, AppPrefs.LANG_SYSTEM)
        assertSame(context, LocaleHelper.wrap(context))

        AppPrefs.setLang(context, "en")
        try {
            val wrapped = LocaleHelper.wrap(context)
            assertNotSame(context, wrapped)
            assertEquals("en", LocaleHelper.activeCode(wrapped))
            assertEquals("en", wrapped.resources.configuration.locales[0].language)
            val system = context.resources.configuration
            val own = wrapped.resources.configuration
            assertEquals(system.orientation, own.orientation)
            assertEquals(system.screenWidthDp, own.screenWidthDp)
            assertEquals(system.screenHeightDp, own.screenHeightDp)
            assertEquals(system.fontScale, own.fontScale, 0f)
            assertEquals(system.uiMode, own.uiMode)
        } finally {
            AppPrefs.setLang(context, AppPrefs.LANG_SYSTEM)
        }
    }
}
