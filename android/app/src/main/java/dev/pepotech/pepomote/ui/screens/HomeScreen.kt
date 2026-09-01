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
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.R
import dev.pepotech.pepomote.ui.components.ChannelCard
import dev.pepotech.pepomote.ui.components.ChannelGlyph
import dev.pepotech.pepomote.ui.theme.PepoColors

private data class Channel(
    val title: Int,
    val subtitle: Int,
    val glyph: ChannelGlyph,
    val accent: Color
)

private val channels = listOf(
    Channel(R.string.channel_connect, R.string.channel_connect_sub, ChannelGlyph.Qr, PepoColors.Blue),
    Channel(R.string.channel_controller, R.string.channel_controller_sub, ChannelGlyph.Pad, PepoColors.Blue),
    Channel(R.string.channel_dolphin, R.string.channel_dolphin_sub, ChannelGlyph.Pointer, PepoColors.Ok),
    Channel(R.string.channel_settings, R.string.channel_settings_sub, ChannelGlyph.Gear, PepoColors.TextDim)
)

@Composable
fun HomeScreen() {
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
            Box(
                Modifier
                    .size(10.dp)
                    .background(PepoColors.TextDim, CircleShape)
            )
            Spacer(Modifier.width(8.dp))
            Text(
                stringResource(R.string.status_disconnected),
                style = MaterialTheme.typography.bodyMedium
            )
        }
        Spacer(Modifier.height(24.dp))
        LazyVerticalGrid(
            columns = GridCells.Fixed(2),
            horizontalArrangement = Arrangement.spacedBy(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            items(channels) { ch ->
                ChannelCard(
                    title = stringResource(ch.title),
                    subtitle = stringResource(ch.subtitle),
                    glyph = ch.glyph,
                    accent = ch.accent
                )
            }
        }
    }
}
