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
import dev.pepotech.pepomote.ui.theme.PepoColors

@Composable
fun HomeScreen(
    connected: Boolean,
    onConnect: () -> Unit,
    onController: () -> Unit,
    onDolphin: () -> Unit,
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
            Box(
                Modifier
                    .size(10.dp)
                    .background(if (connected) PepoColors.Ok else PepoColors.TextDim, CircleShape)
            )
            Spacer(Modifier.width(8.dp))
            Text(
                stringResource(if (connected) R.string.status_connected else R.string.status_disconnected),
                style = MaterialTheme.typography.bodyMedium
            )
        }
        Spacer(Modifier.height(24.dp))
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
