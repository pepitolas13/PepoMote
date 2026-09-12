package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.ui.components.ChannelCard
import dev.pepotech.pepomote.ui.components.ChannelGlyph
import dev.pepotech.pepomote.ui.components.PulsingDot
import dev.pepotech.pepomote.ui.theme.PepoColors

/** Estado del enlace en el inicio: el punto (apagado, en marcha, encendido) y su texto. */
enum class HomeTone { Off, Busy, On }

data class HomeStatus(val tone: HomeTone, val text: String)

@Composable
fun HomeScreen(
    status: HomeStatus,
    onConnect: () -> Unit,
    onController: () -> Unit,
    onDolphin: () -> Unit,
    onWiiU: () -> Unit,
    onNunchuk: () -> Unit,
    onNewPairing: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .padding(horizontal = 20.dp)
    ) {
        Spacer(Modifier.height(28.dp))
        Text("PepoMote", style = MaterialTheme.typography.displayLarge)
        Text(
            stringResource(R.string.home_subtitle),
            style = MaterialTheme.typography.bodyMedium
        )
        Spacer(Modifier.height(10.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            when (status.tone) {
                HomeTone.On -> Box(
                    Modifier
                        .size(10.dp)
                        .background(PepoColors.Ok, CircleShape)
                )
                HomeTone.Busy -> PulsingDot(PepoColors.Warn)
                HomeTone.Off -> Box(
                    Modifier
                        .size(10.dp)
                        .background(PepoColors.TextDim, CircleShape)
                )
            }
            Spacer(Modifier.width(8.dp))
            Text(status.text, style = MaterialTheme.typography.bodyMedium)
        }
        Spacer(Modifier.height(24.dp))
        // Tres filas de pares: Conectar | Mando, Dolphin | Wii U, Nunchuk | Ajustes
        LazyVerticalGrid(
            columns = GridCells.Fixed(2),
            horizontalArrangement = Arrangement.spacedBy(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            item {
                ChannelCard(
                    title = stringResource(R.string.channel_connect),
                    subtitle = stringResource(R.string.channel_connect_sub),
                    glyph = ChannelGlyph.Qr,
                    accent = PepoColors.Blue,
                    onClick = onConnect
                )
            }
            item {
                ChannelCard(
                    title = stringResource(R.string.channel_controller),
                    subtitle = stringResource(R.string.channel_controller_sub),
                    glyph = ChannelGlyph.Pad,
                    accent = PepoColors.Blue,
                    onClick = onController
                )
            }
            item {
                ChannelCard(
                    title = stringResource(R.string.channel_dolphin),
                    subtitle = stringResource(R.string.channel_dolphin_sub),
                    glyph = ChannelGlyph.Pointer,
                    accent = PepoColors.Ok,
                    onClick = onDolphin
                )
            }
            // Wii U: el móvil como GamePad (o Pro Controller) para Cemu
            item {
                ChannelCard(
                    title = stringResource(R.string.channel_wiiu),
                    subtitle = stringResource(R.string.channel_wiiu_sub),
                    glyph = ChannelGlyph.GamePad,
                    accent = PepoColors.Ok,
                    onClick = onWiiU
                )
            }
            // Con Dolphin: el segundo móvil, en la otra mano
            item {
                ChannelCard(
                    title = stringResource(R.string.channel_nunchuk),
                    subtitle = stringResource(R.string.channel_nunchuk_sub),
                    glyph = ChannelGlyph.Stick,
                    accent = PepoColors.Ok,
                    onClick = onNunchuk
                )
            }
            item {
                ChannelCard(
                    title = stringResource(R.string.channel_settings),
                    subtitle = stringResource(R.string.channel_settings_sub),
                    glyph = ChannelGlyph.Gear,
                    accent = PepoColors.TextDim,
                    onClick = onNewPairing
                )
            }
        }
    }
}
