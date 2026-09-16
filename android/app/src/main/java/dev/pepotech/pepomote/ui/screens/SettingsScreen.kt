package dev.pepotech.pepomote.ui.screens

import androidx.annotation.StringRes
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.displayCutoutPadding
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.service.GamePadSide
import dev.pepotech.pepomote.service.LandscapeSide
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.NunchukSide
import dev.pepotech.pepomote.sensor.GyroClass
import dev.pepotech.pepomote.sensor.GyroDetect
import dev.pepotech.pepomote.sensor.MotionSource
import dev.pepotech.pepomote.ui.theme.PepoColors
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

/** Tarjeta del lado de un mando apaisado fijo: Izquierda / Derecha / Según el sensor. */
@Composable
private fun SideCard(@StringRes title: Int, @StringRes sub: Int, side: LandscapeSide, onPick: (LandscapeSide) -> Unit) {
    Card(
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
        border = BorderStroke(1.5.dp, PepoColors.CardBorder),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
    ) {
        Column(Modifier.fillMaxWidth().padding(18.dp)) {
            Text(stringResource(title), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(sub), style = MaterialTheme.typography.bodyMedium)
            Spacer(Modifier.height(12.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for ((label, s) in listOf(
                    R.string.side_left to LandscapeSide.Left,
                    R.string.side_right to LandscapeSide.Right,
                    R.string.side_sensor to LandscapeSide.Sensor
                )) {
                    ModeChip(stringResource(label), selected = side == s) { onPick(s) }
                }
            }
        }
    }
}

/**
 * Sensor del puntero: giroscopio (lo de siempre, recomendado si es real) o
 * acelerómetro (apuntado por inclinación, para móviles sin giroscopio real).
 * Debajo, lo que tiene este móvil, con el nombre del sensor para poder
 * diagnosticar. Sin giroscopio ni rotation vector solo cabe el acelerómetro.
 */
@Composable
private fun MotionSourceCard() {
    val context = LocalContext.current
    val gyroClass = remember { GyroDetect.classify(context) }
    val sensorName = remember { GyroDetect.describe(context) ?: "—" }
    val tilt by MotionSource.tilt.collectAsState()
    LaunchedEffect(Unit) { MotionSource.refresh(context) }
    Card(
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = PepoColors.Card),
        border = BorderStroke(1.5.dp, PepoColors.CardBorder),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp)
    ) {
        Column(Modifier.fillMaxWidth().padding(18.dp)) {
            Text(stringResource(R.string.motion_title), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(R.string.motion_sub), style = MaterialTheme.typography.bodyMedium)
            Spacer(Modifier.height(12.dp))
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ModeChip(
                    stringResource(R.string.motion_gyro),
                    selected = !tilt,
                    enabled = gyroClass != GyroClass.NONE
                ) { MotionSource.choose(context, accel = false) }
                ModeChip(stringResource(R.string.motion_accel), selected = tilt) {
                    MotionSource.choose(context, accel = true)
                }
            }
            Spacer(Modifier.height(10.dp))
            Text(
                when (gyroClass) {
                    GyroClass.REAL -> stringResource(R.string.motion_status_real, sensorName)
                    GyroClass.VIRTUAL -> stringResource(R.string.motion_status_virtual, sensorName)
                    GyroClass.NONE -> stringResource(R.string.motion_status_none)
                },
                style = MaterialTheme.typography.bodyMedium,
                color = if (gyroClass == GyroClass.REAL) PepoColors.TextDim else PepoColors.Warn
            )
        }
    }
}

@Composable
fun SettingsScreen(onNewPairing: () -> Unit, onBack: () -> Unit) {
    val context = LocalContext.current
    var volB by remember { mutableStateOf(AppPrefs.volDownIsB(context)) }
    var sounds by remember { mutableStateOf(AppPrefs.soundsEnabled(context)) }
    var dolphinChips by remember { mutableStateOf(AppPrefs.showDolphinChips(context)) }
    var noScreen by remember { mutableStateOf(AppPrefs.gamePadNoScreen(context)) }
    var fullScreen by remember { mutableStateOf(AppPrefs.gamePadFullScreen(context)) }
    var fullScreenKb by remember { mutableStateOf(AppPrefs.gamePadFullScreenKeyboard(context)) }
    var ownNunchuk by remember { mutableStateOf(AppPrefs.ownNunchuk(context)) }
    val nunchukSide by NunchukSide.saved.collectAsState()
    val gamePadSide by GamePadSide.saved.collectAsState()
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

        // Sensor del puntero: giroscopio o acelerómetro (móviles sin giroscopio real)
        MotionSourceCard()

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

        // Lado del mando + Nunchuk apaisado: se pregunta la primera vez; aquí se cambia
        Spacer(Modifier.height(14.dp))
        SideCard(R.string.side_title, R.string.side_sub, nunchukSide) { NunchukSide.save(context, it) }

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
                        if (it) fullScreen = false
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
        }

        // Pantalla del GamePad a pantalla completa (mando real en el PC), con la
        // subopción del botón de teclado
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
                    Text(stringResource(R.string.fullscreen_title), style = MaterialTheme.typography.titleMedium)
                    Text(
                        stringResource(R.string.fullscreen_sub),
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
                Switch(
                    checked = fullScreen,
                    onCheckedChange = {
                        fullScreen = it
                        AppPrefs.setGamePadFullScreen(context, it)
                        if (it) noScreen = false
                        LinkState.sendScreenOnly?.invoke(it)
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 30.dp, end = 18.dp, bottom = 16.dp)
                    .alpha(if (fullScreen) 1f else 0.5f),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.fullscreen_kb_title), style = MaterialTheme.typography.titleMedium)
                    Text(
                        stringResource(R.string.fullscreen_kb_sub),
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
                Switch(
                    checked = fullScreenKb,
                    enabled = fullScreen,
                    onCheckedChange = {
                        fullScreenKb = it
                        AppPrefs.setGamePadFullScreenKeyboard(context, it)
                    },
                    colors = SwitchDefaults.colors(checkedTrackColor = PepoColors.Blue)
                )
            }
        }

        // Lado del GamePad de Wii U: el suyo, aparte del mando + Nunchuk
        Spacer(Modifier.height(14.dp))
        SideCard(R.string.side_gamepad_title, R.string.side_gamepad_sub, gamePadSide) { GamePadSide.save(context, it) }

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
