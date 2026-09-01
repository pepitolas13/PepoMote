package dev.pepotech.pepomote.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
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

// Paleta "PepoWhite" — diseño original PepoMote (inspiración Wii, cero assets ajenos)
object PepoColors {
    val Background = Color(0xFFF4F6F7)
    val Card = Color(0xFFFFFFFF)
    val CardBorder = Color(0xFFE3E8EB)
    val Text = Color(0xFF3B4750)
    val TextDim = Color(0xFF7C8A94)
    val Blue = Color(0xFF3FA9F5)
    val BlueHover = Color(0xFF2B98E8)
    val Glow = Color(0xFFAEE2FF)
    val Ok = Color(0xFF7BC94C)
    val Warn = Color(0xFFF5A83C)
    val Error = Color(0xFFE85C5C)
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

private val pepoTypography = Typography(
    displayLarge = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Black, fontSize = 40.sp, color = PepoColors.Text),
    headlineMedium = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Black, fontSize = 26.sp, color = PepoColors.Text),
    titleLarge = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Bold, fontSize = 20.sp, color = PepoColors.Text),
    titleMedium = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Bold, fontSize = 17.sp, color = PepoColors.Text),
    bodyLarge = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Normal, fontSize = 16.sp, color = PepoColors.Text),
    bodyMedium = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Normal, fontSize = 14.sp, color = PepoColors.TextDim),
    labelLarge = TextStyle(fontFamily = nunito, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = PepoColors.Text)
)

private val pepoShapes = Shapes(
    small = RoundedCornerShape(14.dp),
    medium = RoundedCornerShape(24.dp),
    large = RoundedCornerShape(32.dp)
)

private val pepoColorScheme = lightColorScheme(
    primary = PepoColors.Blue,
    onPrimary = Color.White,
    secondary = PepoColors.Glow,
    onSecondary = PepoColors.Text,
    background = PepoColors.Background,
    onBackground = PepoColors.Text,
    surface = PepoColors.Card,
    onSurface = PepoColors.Text,
    surfaceVariant = PepoColors.Card,
    onSurfaceVariant = PepoColors.TextDim,
    outline = PepoColors.CardBorder,
    error = PepoColors.Error,
    onError = Color.White
)

@Composable
fun PepoMoteTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = pepoColorScheme,
        typography = pepoTypography,
        shapes = pepoShapes,
        content = content
    )
}
