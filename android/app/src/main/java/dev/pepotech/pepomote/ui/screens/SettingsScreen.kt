package dev.pepotech.pepomote.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.displayCutoutPadding
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.ui.theme.PepoColors
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

@Composable
fun SettingsScreen(onNewPairing: () -> Unit, onBack: () -> Unit) {
    val context = LocalContext.current
    var volB by remember { mutableStateOf(AppPrefs.volDownIsB(context)) }
    var sounds by remember { mutableStateOf(AppPrefs.soundsEnabled(context)) }
    var dolphinChips by remember { mutableStateOf(AppPrefs.showDolphinChips(context)) }
    var noScreen by remember { mutableStateOf(AppPrefs.gamePadNoScreen(context)) }
    var ownNunchuk by remember { mutableStateOf(AppPrefs.ownNunchuk(context)) }
    var notices by remember { mutableStateOf(AppPrefs.receiverNotices(context)) }
    var updateCheck by remember { mutableStateOf(AppPrefs.updateCheckEnabled(context)) }
    val versionName = remember {
        try {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: "?"
        } catch (_: Exception) {
            "?"
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(PepoColors.Background)
            .statusBarsPadding()
            .displayCutoutPadding()
            .padding(horizontal = 20.dp)
            .verticalScroll(rememberScrollState())
    ) {
        Spacer(Modifier.height(24.dp))
        Text(stringResource(R.string.channel_settings), style = MaterialTheme.typography.headlineMedium)
        Spacer(Modifier.height(20.dp))

        Card(
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
            border = BorderStroke(1.5.dp, PepoColors.CardBorder),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(18.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.vol_title), style = MaterialTheme.typography.titleMedium)
                    Text(
                        stringResource(R.string.vol_sub),
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
                Switch(
                    checked = volB,
                    onCheckedChange = {
                        volB = it
                        AppPrefs.setVolDownIsB(context, it)
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
        }

        Spacer(Modifier.height(14.dp))
        Card(
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
            border = BorderStroke(1.5.dp, PepoColors.CardBorder),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(18.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.sounds_title), style = MaterialTheme.typography.titleMedium)
                    Text(
                        stringResource(R.string.sounds_sub),
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
                Switch(
                    checked = sounds,
                    onCheckedChange = {
                        sounds = it
                        AppPrefs.setSoundsEnabled(context, it)
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
        }

        Spacer(Modifier.height(14.dp))
        Card(
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
            border = BorderStroke(1.5.dp, PepoColors.CardBorder),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(18.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.chips_title), style = MaterialTheme.typography.titleMedium)
                    Text(
                        stringResource(R.string.chips_sub),
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
                Switch(
                    checked = dolphinChips,
                    onCheckedChange = {
                        dolphinChips = it
                        AppPrefs.setShowDolphinChips(context, it)
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
        }

        // Nunchuk en el mismo móvil (Dolphin): stick, C y Z con el móvil de lado
        Spacer(Modifier.height(14.dp))
        Card(
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
            border = BorderStroke(1.5.dp, PepoColors.CardBorder),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(18.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.nunchuk_own_title), style = MaterialTheme.typography.titleMedium)
                    Text(
                        stringResource(R.string.nunchuk_own_sub),
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
                Switch(
                    checked = ownNunchuk,
                    onCheckedChange = {
                        ownNunchuk = it
                        AppPrefs.setOwnNunchuk(context, it)
                        // Con el enlace vivo se aplica ya (el receptor lo confirma con el eco)
                        LinkState.sendNunchuk?.invoke(it)
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
        }

        // GamePad de Wii U sin pantalla táctil: botones más grandes
        Spacer(Modifier.height(14.dp))
        Card(
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
            border = BorderStroke(1.5.dp, PepoColors.CardBorder),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(18.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.noscreen_title), style = MaterialTheme.typography.titleMedium)
                    Text(
                        stringResource(R.string.noscreen_sub),
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
                Switch(
                    checked = noScreen,
                    onCheckedChange = {
                        noScreen = it
                        AppPrefs.setGamePadNoScreen(context, it)
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
        }

        // Avisos del receptor sobre el mando al cambiar de modo
        Spacer(Modifier.height(14.dp))
        Card(
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
            border = BorderStroke(1.5.dp, PepoColors.CardBorder),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(18.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.notices_title), style = MaterialTheme.typography.titleMedium)
                    Text(
                        stringResource(R.string.notices_sub),
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
                Switch(
                    checked = notices,
                    onCheckedChange = {
                        notices = it
                        AppPrefs.setReceiverNotices(context, it)
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
        }

        // Aviso de versión nueva: la única consulta fuera de la red local
        Spacer(Modifier.height(14.dp))
        Card(
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
            border = BorderStroke(1.5.dp, PepoColors.CardBorder),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(18.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.update_title), style = MaterialTheme.typography.titleMedium)
                    Text(
                        stringResource(R.string.update_sub),
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
                Switch(
                    checked = updateCheck,
                    onCheckedChange = {
                        updateCheck = it
                        AppPrefs.setUpdateCheckEnabled(context, it)
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
        }

        Spacer(Modifier.height(14.dp))
        Card(
            onClick = onNewPairing,
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
            border = BorderStroke(1.5.dp, PepoColors.CardBorder),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
        ) {
            Column(Modifier.fillMaxWidth().padding(18.dp)) {
                Text(stringResource(R.string.link_other_pc), style = MaterialTheme.typography.titleMedium)
                Text(stringResource(R.string.link_other_pc_sub), style = MaterialTheme.typography.bodyMedium)
            }
        }

        Spacer(Modifier.height(14.dp))
        Text(
            stringResource(R.string.sens_note),
            style = MaterialTheme.typography.bodyMedium
        )

        Spacer(Modifier.height(24.dp))
        Text(
            stringResource(R.string.about, versionName),
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.align(Alignment.CenterHorizontally)
        )
        TextButton(onClick = onBack, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(R.string.back), color = PepoColors.TextDim)
        }
        Spacer(Modifier.height(16.dp))
    }
}
