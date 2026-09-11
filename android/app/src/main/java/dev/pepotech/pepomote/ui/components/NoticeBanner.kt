package dev.pepotech.pepomote.ui.components

import android.os.SystemClock
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.Notice
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlinx.coroutines.delay

/** Cuánto dura en pantalla un aviso del receptor. */
private const val NOTICE_MS = 6000L

/**
 * Aviso transitorio del receptor (`notice`, p. ej. «Cemu está abierto:
 * ciérralo para aplicar la configuración»): se enseña ~6 s desde que llegó,
 * en la pantalla que esté. Un aviso ya caducado no se muestra.
 */
@Composable
fun NoticeBanner(modifier: Modifier = Modifier) {
    val notice by LinkState.notice.collectAsState()
    var shown by remember { mutableStateOf<Notice?>(null) }

    LaunchedEffect(notice) {
        val n = notice ?: return@LaunchedEffect
        val remaining = NOTICE_MS - (SystemClock.elapsedRealtime() - n.atMs)
        if (remaining <= 0) return@LaunchedEffect
        shown = n
        delay(remaining)
        if (shown === n) shown = null
    }

    val n = shown ?: return
    Text(
        n.text,
        modifier = modifier
            .widthIn(max = 440.dp)
            .background(PepoColors.Card, RoundedCornerShape(14.dp))
            .border(1.5.dp, PepoColors.Warn, RoundedCornerShape(14.dp))
            .padding(horizontal = 16.dp, vertical = 10.dp),
        style = MaterialTheme.typography.bodyMedium.copy(color = PepoColors.Text),
        textAlign = TextAlign.Center
    )
}
