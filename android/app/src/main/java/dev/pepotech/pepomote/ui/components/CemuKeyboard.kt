package dev.pepotech.pepomote.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.material3.OutlinedButton
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
import android.content.Context
import dev.pepotech.pepomote.control.AppPrefs
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.control.TextInput
import dev.pepotech.pepomote.service.LinkState
import dev.pepotech.pepomote.service.Route
import dev.pepotech.pepomote.service.UiLink
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlinx.coroutines.delay
import androidx.compose.ui.res.stringResource
import dev.pepotech.pepomote.R

/**
 * Lo que pasa al tocar «Teclado», en este orden y por estos motivos. La
 * pantalla solo tiene que abrir el diálogo después.
 *
 * 1. **Nada puede quedar pulsado.** El diálogo se traga el ACTION_UP del dedo
 *    que estuviera sobre un botón, y con «pulsación pegajosa» (activada de
 *    serie) ese botón se quedaría pegado.
 * 2. **La pose se congela ANTES del clic.** Si el móvil se mueve entre el
 *    flanco de bajada y el de subida (70 ms), el PC ve un ARRASTRE: en un
 *    campo de texto eso selecciona en vez de dejar el cursor.
 * 3. **El clic es el bit A de siempre**, con sus 70 ms garantizados por
 *    `PressLatch` (7-18 paquetes). No hace falta ningún mensaje nuevo, va
 *    coordinado con la posición porque viaja en el mismo paquete, y funciona
 *    con cualquier receptor publicado.
 */
fun openKeyboard(context: Context, link: UiLink, press: PressRegistry?) {
    press?.releaseAll()
    ButtonState.reset()
    if (Route.holdsPointerForKeyboard(link)) {
        LinkState.motion?.pointerHold = true
    }
    if (Route.clicksBeforeKeyboard(link, AppPrefs.keyboardClickFirst(context))) {
        ButtonState.set(ButtonState.A, true)
        ButtonState.set(ButtonState.A, false)
    }
}

/**
 * Botón pequeño «Teclado» de las cabeceras en modo Wii U (GamePad, Pro y
 * Mando de Wii) y del hueco de Home en modo puntero: abre [KeyboardDialog].
 * Mismo aire que los chips de modo.
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
 * Diálogo «Teclado»: el texto se escribe aquí (teclado del sistema, con foco
 * al abrir) y se manda por el canal de control.
 * - Borrar: un carácter menos en el PC (el campo no cambia);
 * - Escribir: manda el campo tal cual y lo vacía (el diálogo sigue);
 * - Aceptar (o la tecla de enviar del teclado): manda el campo + Intro y
 *   cierra; con el campo vacío manda solo Intro (confirmar);
 * - Cerrar / atrás / tocar fuera: cierra sin mandar nada.
 *
 * Cuando el texto acaba en una ventana del PC (modo puntero y mando
 * universal: [pointer] o [universalPad]) el botón principal es **Enviar**, que
 * manda el texto TAL CUAL —un Intro de más envía la búsqueda, manda el
 * mensaje del chat a medias o envía el formulario antes de tiempo— y el Intro
 * tiene su propio botón al lado, **Enviar + ⏎**, que es lo que hace falta para
 * buscar. En Wii U, Switch y RetroArch todo sigue igual: el azul es
 * «Aceptar» y manda texto + Intro, que confirma el teclado del juego.
 *
 * Qué se manda en cada caso lo decide [TextInput]. No gira la pantalla y los
 * paquetes INPUT siguen saliendo mientras está abierto (con la pose
 * congelada en modo puntero, para que el cursor no se vaya al escribir).
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun KeyboardDialog(
    onSend: (String) -> Unit,
    onClose: () -> Unit,
    switchPad: Boolean = false,
    retroPad: Boolean = false,
    pointer: Boolean = false,
    universalPad: Boolean = false
) {
    // El texto va a una ventana del PC, no al teclado en pantalla de un
    // emulador ([Route.keyboardOnPc])
    val toPc = pointer || universalPad
    var field by remember { mutableStateOf("") }
    val focusRequester = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    val shape = RoundedCornerShape(24.dp)

    fun apply(action: TextInput.Action) {
        action.send?.let(onSend)
        field = action.field
        if (action.close) onClose()
    }

    /** Lo que hace el botón grande (y la tecla de enviar del teclado). */
    fun primary(text: String) = if (toPc) TextInput.send(text) else TextInput.accept(text)

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
            Text(
                stringResource(
                    if (toPc) R.string.kb_pointer_title
                    else if (retroPad) R.string.kb_retroarch_title
                    else if (switchPad) R.string.kb_switch_title
                    else R.string.kb_title
                ),
                style = MaterialTheme.typography.titleMedium
            )
            Text(
                stringResource(
                    if (universalPad) R.string.kb_pc_help
                    else if (pointer) R.string.kb_pointer_help
                    else if (retroPad) R.string.kb_retroarch_help
                    else if (switchPad) R.string.kb_switch_help
                    else R.string.kb_help
                ),
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
                // Sin autocorrección: aquí se escriben direcciones, correos y
                // nombres de personaje, justo lo que el predictivo estropea
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send, autoCorrectEnabled = false),
                keyboardActions = KeyboardActions(onSend = { apply(primary(field)) }),
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
            // Con la letra del sistema muy grande los tres botones no caben
            // en una línea: bajan de línea en vez de recortarse
            FlowRow(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(4.dp, Alignment.End),
                verticalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                TextButton(onClick = { apply(TextInput.close(field)) }) {
                    Text(stringResource(R.string.kb_close), color = PepoColors.TextDim)
                }
                if (toPc) {
                    // El Intro aparte, y con forma de botón: buscar, ir a una
                    // dirección, mandar el chat. De texto gris no se veía que
                    // se podía pulsar. Estrecho para que quepan los tres
                    OutlinedButton(
                        onClick = { apply(TextInput.accept(field)) },
                        border = BorderStroke(1.5.dp, PepoColors.Blue),
                        colors = ButtonDefaults.outlinedButtonColors(contentColor = PepoColors.Blue),
                        contentPadding = ButtonDefaults.TextButtonContentPadding
                    ) {
                        Text(stringResource(R.string.kb_enter), color = PepoColors.Blue, maxLines = 1)
                    }
                }
                Button(
                    onClick = { apply(primary(field)) },
                    colors = ButtonDefaults.buttonColors(
                        containerColor = PepoColors.Blue,
                        contentColor = PepoColors.OnAccent
                    )
                ) {
                    Text(
                        stringResource(if (toPc) R.string.kb_send else R.string.kb_accept),
                        color = PepoColors.OnAccent
                    )
                }
            }
        }
    }
}
