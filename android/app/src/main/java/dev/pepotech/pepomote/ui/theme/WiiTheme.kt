package dev.pepotech.pepomote.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.pepotech.pepomote.R

/**
 * Paleta de la app. Dos: «PepoWhite» (la de siempre, inspiración Wii, cero
 * assets ajenos) y «PepoDark» (el mismo azul y los mismos acentos sobre
 * grafito). La activa la elige [PepoMoteTheme] según el sistema.
 */
@Immutable
data class PepoPalette(
    val background: Color,
    val card: Color,
    val cardBorder: Color,
    val text: Color,
    val textDim: Color,
    val blue: Color,
    val blueHover: Color,
    val glow: Color,
    val ok: Color,
    val warn: Color,
    val error: Color,
    val isDark: Boolean,
    /** Texto sobre el azul (o cualquier acento): blanco en los dos temas. */
    val onAccent: Color = Color.White
)

val LightPepo = PepoPalette(
    background = Color(0xFFF4F6F7),
    card = Color(0xFFFFFFFF),
    cardBorder = Color(0xFFE3E8EB),
    text = Color(0xFF3B4750),
    textDim = Color(0xFF7C8A94),
    blue = Color(0xFF3FA9F5),
    blueHover = Color(0xFF2B98E8),
    glow = Color(0xFFAEE2FF),
    ok = Color(0xFF7BC94C),
    warn = Color(0xFFF5A83C),
    error = Color(0xFFE85C5C),
    isDark = false
)

val DarkPepo = PepoPalette(
    background = Color(0xFF14181C),
    card = Color(0xFF1F252B),
    cardBorder = Color(0xFF2C343B),
    text = Color(0xFFE6EBEF),
    textDim = Color(0xFF8E9BA6),
    blue = LightPepo.blue,
    blueHover = LightPepo.blueHover,
    glow = Color(0xFF1D3D52),
    ok = LightPepo.ok,
    warn = LightPepo.warn,
    error = LightPepo.error,
    isDark = true
)

val LocalPepoColors = staticCompositionLocalOf { LightPepo }

/**
 * Los colores de la paleta activa, leídos en composición (`PepoColors.Text`…).
 * Dentro de un `Canvas`/`drawBehind` (que no es composición) léelos antes y
 * pásalos como valor.
 */
object PepoColors {
    val Background: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.background
    val Card: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.card
    val CardBorder: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.cardBorder
    val Text: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.text
    val TextDim: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.textDim
    val Blue: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.blue
    val BlueHover: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.blueHover
    val Glow: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.glow
    val Ok: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.ok
    val Warn: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.warn
    val Error: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.error
    val OnAccent: Color @Composable @ReadOnlyComposable get() = LocalPepoColors.current.onAccent
}

@OptIn(ExperimentalTextApi::class)
private val nunito = FontFamily(
    Font(
        R.font.nunito,
        weight = FontWeight.Normal,
        variationSettings = FontVariation.Settings(FontVariation.weight(400))
    ),
    Font(
        R.font.nunito,
        weight = FontWeight.Bold,
        variationSettings = FontVariation.Settings(FontVariation.weight(700))
    ),
    Font(
        R.font.nunito,
        weight = FontWeight.Black,
        variationSettings = FontVariation.Settings(FontVariation.weight(900))
    )
)

private fun pepoTypography(c: PepoPalette) = Typography(
    displayLarge = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Black, fontSize = 40.sp, color = c.text),
    headlineMedium = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Black, fontSize = 26.sp, color = c.text),
    titleLarge = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Bold, fontSize = 20.sp, color = c.text),
    titleMedium = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Bold, fontSize = 17.sp, color = c.text),
    bodyLarge = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Normal, fontSize = 16.sp, color = c.text),
    bodyMedium = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Normal, fontSize = 14.sp, color = c.textDim),
    labelLarge = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = c.text)
)

private val pepoShapes = Shapes(
    small = RoundedCornerShape(14.dp),
    medium = RoundedCornerShape(24.dp),
    large = RoundedCornerShape(32.dp)
)

private fun pepoColorScheme(c: PepoPalette): ColorScheme {
    val base = if (c.isDark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = c.blue,
        onPrimary = c.onAccent,
        secondary = c.glow,
        onSecondary = c.text,
        background = c.background,
        onBackground = c.text,
        surface = c.card,
        onSurface = c.text,
        surfaceVariant = c.card,
        onSurfaceVariant = c.textDim,
        outline = c.cardBorder,
        error = c.error,
        onError = c.onAccent
    )
}

/** El tema de la app: sigue al sistema (claro u oscuro). */
@Composable
fun PepoMoteTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val palette = if (dark) DarkPepo else LightPepo
    CompositionLocalProvider(LocalPepoColors provides palette) {
        MaterialTheme(
            colorScheme = pepoColorScheme(palette),
            typography = pepoTypography(palette),
            shapes = pepoShapes,
            content = content
        )
    }
}
