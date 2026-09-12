package dev.pepotech.pepomote.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import dev.pepotech.pepomote.control.TextInput
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlinx.coroutines.delay
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

/**
 * Botón pequeño «Teclado» de las cabeceras en modo Wii U (GamePad, Pro y
 * Mando de Wii): abre [KeyboardDialog]. Mismo aire que los chips de modo.
 */
@Composable
fun KeyboardButton(compact: Boolean = false, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val current by rememberUpdatedState(onClick)
    val shape = RoundedCornerShape(18.dp)
    Box(
        modifier = modifier
            .background(PepoColors.Card, shape)
            .border(1.5.dp, PepoColors.CardBorder, shape)
            .pointerInput(Unit) {
                detectTapGestures(onTap = { current() })
            }
            .padding(
                horizontal = if (compact) 12.dp else 16.dp,
                vertical = if (compact) 6.dp else 8.dp
            )
    ) {
        Text(
            stringResource(R.string.keyboard),
            style = MaterialTheme.typography.bodyMedium.copy(color = PepoColors.Text),
            maxLines = 1
        )
    }
}

/**
 * Diálogo «Teclado» del modo Wii U: el teclado en pantalla de Cemu solo
 * acepta teclas del PC, así que el texto se escribe aquí (teclado del
 * sistema, con foco al abrir) y se manda por el canal de control:
 * - Borrar: un carácter menos en Cemu (el campo no cambia);
 * - Escribir: manda el campo tal cual y lo vacía (el diálogo sigue);
 * - Aceptar (o la tecla de enviar del teclado): manda el campo + Intro y
 *   cierra; con el campo vacío manda solo Intro (confirmar);
 * - Cerrar / atrás / tocar fuera: cierra sin mandar nada.
 * Qué se manda en cada caso lo decide [TextInput]. No gira la pantalla y los
 * paquetes INPUT siguen saliendo mientras está abierto.
 */
@Composable
fun KeyboardDialog(onSend: (String) -> Unit, onClose: () -> Unit) {
    var field by remember { mutableStateOf("") }
    val focusRequester = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    val shape = RoundedCornerShape(24.dp)

    fun apply(action: TextInput.Action) {
        action.send?.let(onSend)
        field = action.field
        if (action.close) onClose()
    }

    // Foco y teclado al abrir: la ventana del diálogo tarda un instante en tenerlo
    LaunchedEffect(Unit) {
        delay(80)
        focusRequester.requestFocus()
        keyboard?.show()
    }

    Dialog(onDismissRequest = onClose) {
        Column(
            modifier = Modifier
                .widthIn(max = 440.dp)
                .background(PepoColors.Card, shape)
                .border(1.5.dp, PepoColors.CardBorder, shape)
                .padding(horizontal = 18.dp, vertical = 14.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Text(stringResource(R.string.kb_title), style = MaterialTheme.typography.titleMedium)
            Text(
                stringResource(R.string.kb_help),
                style = MaterialTheme.typography.bodyMedium
            )
            OutlinedTextField(
                value = field,
                onValueChange = { field = it },
                modifier = Modifier
                    .fillMaxWidth()
                    .focusRequester(focusRequester),
                singleLine = true,
                placeholder = { Text(stringResource(R.string.kb_placeholder), style = MaterialTheme.typography.bodyMedium) },
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                keyboardActions = KeyboardActions(onSend = { apply(TextInput.accept(field)) }),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = PepoColors.Blue,
                    unfocusedBorderColor = PepoColors.CardBorder,
                    cursorColor = PepoColors.Blue
                )
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                TextButton(onClick = { apply(TextInput.delete(field)) }) {
                    Text(stringResource(R.string.kb_delete), color = PepoColors.Text)
                }
                TextButton(onClick = { apply(TextInput.write(field)) }) {
                    Text(stringResource(R.string.kb_write), color = PepoColors.Text)
                }
                Spacer(Modifier.weight(1f))
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                Spacer(Modifier.weight(1f))
                TextButton(onClick = { apply(TextInput.close(field)) }) {
                    Text(stringResource(R.string.kb_close), color = PepoColors.TextDim)
                }
                Button(
                    onClick = { apply(TextInput.accept(field)) },
                    colors = ButtonDefaults.buttonColors(
                        containerColor = PepoColors.Blue,
                        contentColor = PepoColors.OnAccent
                    )
                ) {
                    Text(stringResource(R.string.kb_accept), color = PepoColors.OnAccent)
                }
            }
        }
    }
}
