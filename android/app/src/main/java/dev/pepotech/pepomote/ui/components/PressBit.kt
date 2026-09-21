package dev.pepotech.pepomote.ui.components

import android.view.HapticFeedbackConstants
import android.view.View
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.drag
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalView
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.control.Haptics
import dev.pepotech.pepomote.control.PressEvent
import dev.pepotech.pepomote.control.PressHit
import dev.pepotech.pepomote.control.PressMode
import dev.pepotech.pepomote.control.PressZone
import dev.pepotech.pepomote.control.SlideTracker
import dev.pepotech.pepomote.control.UiSounds

/**
 * Los botones pulsables de una pantalla: dónde está cada uno (para saber cuál
 * hay bajo un dedo) y cómo se pulsa y se suelta. Lo monta la pantalla en su
 * caja raíz y lo comparten sus botones por [LocalPressRegistry].
 *
 * Solo lleva botones momentáneos (`pressBit`): ni stick, ni cruceta, ni
 * chips, ni diana, ni pantalla táctil, ni las tiras de Precisión/Acercar.
 */
class PressRegistry(private val view: View? = null) {
    private class Entry(val zone: PressZone, val onDown: (Boolean) -> Unit, val pop: Boolean)

    private val entries = LinkedHashMap<Int, Entry>()
    private val down = HashSet<Int>()
    private var zones: List<PressZone> = emptyList()

    /**
     * Con la capa apagada no se pulsa deslizando: el GamePad la apaga
     * mientras el receptor no confirma el modo (los controles están inertes)
     * y en pantalla completa (no hay botones, solo la pantalla de Cemu).
     */
    var enabled by mutableStateOf(true)

    /** Un botón dice dónde está (cada vez que se coloca). */
    fun place(bit: Int, zone: PressZone, onDown: (Boolean) -> Unit, pop: Boolean) {
        entries[bit] = Entry(zone, onDown, pop)
        zones = entries.values.map { it.zone }
    }

    /** El botón se va de la pantalla: ni se pulsa ni se queda pulsado. */
    fun remove(bit: Int) {
        release(bit)
        entries.remove(bit)
        zones = entries.values.map { it.zone }
    }

    fun zones(): List<PressZone> = zones

    /** Pulsa: color, bit, vibración y sonido (solo en el flanco de bajada). */
    fun press(bit: Int) {
        val entry = entries[bit] ?: return
        if (!down.add(bit)) return
        entry.onDown(true)
        ButtonState.set(bit, true)
        Haptics.tap(view, HapticFeedbackConstants.KEYBOARD_TAP)
        if (entry.pop) UiSounds.pop() else UiSounds.blip()
    }

    fun release(bit: Int) {
        if (!down.remove(bit)) return
        entries[bit]?.onDown?.invoke(false)
        ButtonState.set(bit, false)
    }

    /** Nada queda pulsado (se sale de la pantalla, se cancela el gesto). */
    fun releaseAll() {
        for (bit in down.toList()) release(bit)
    }

    internal fun dispatch(events: List<PressEvent>) {
        for (event in events) when (event) {
            is PressEvent.Press -> press(event.bit)
            is PressEvent.Release -> release(event.bit)
        }
    }
}

/** El registro de la pantalla, si la pantalla lo ha montado. */
val LocalPressRegistry = staticCompositionLocalOf<PressRegistry?> { null }

/** El registro de esta pantalla, con la vista para la vibración. */
@Composable
fun rememberPressRegistry(): PressRegistry {
    val view = LocalView.current
    return remember(view) { PressRegistry(view) }
}

/**
 * Botón momentáneo: mantiene [bit] mientras está pulsado, y avisa a
 * [onDown] para el color. Tres formas de pulsar, según los ajustes
 * ([PressMode]):
 *
 * - «Pulsar deslizando»: el botón no coge el gesto (así el `down` llega sin
 *   consumir a la capa de la pantalla, `slideCanvas`), solo dice dónde está.
 * - «Mantener al salir del botón» (de serie): el gesto sigue al dedo hasta
 *   que lo levanta, como las tiras de Precisión/Acercar; el `finally` suelta
 *   el bit también si se cancela.
 * - Ninguno de los dos: lo de Compose de siempre (`tryAwaitRelease`), que
 *   suelta al salirse de los límites.
 *
 * [circular]: botón redondo (se acierta por el círculo). [pop]: el sonido
 * gordo de A. El `onGloballyPositioned` va ANTES de cualquier
 * `graphicsLayer { scale }`: la zona es la del botón en reposo, no la del
 * botón encogido al pulsarlo.
 */
@Composable
fun Modifier.pressBit(
    bit: Int,
    circular: Boolean = false,
    pop: Boolean = false,
    onDown: (Boolean) -> Unit
): Modifier {
    val view = LocalView.current
    val registry = LocalPressRegistry.current
    val latest by rememberUpdatedState(onDown)
    val sink = remember { { pressed: Boolean -> latest(pressed) } }
    val slide = registry != null && registry.enabled && PressMode.slide
    val sticky = PressMode.sticky

    var modifier = this
    if (registry != null) {
        modifier = modifier.onGloballyPositioned { coords ->
            val p = coords.positionInRoot()
            val size = coords.size
            registry.place(
                bit,
                PressZone(bit, p.x, p.y, p.x + size.width, p.y + size.height, circular),
                sink,
                pop
            )
        }
        DisposableEffect(registry, bit) {
            onDispose { registry.remove(bit) }
        }
    }
    if (slide) return modifier
    return modifier.pointerInput(bit, sticky) {
        if (sticky) {
            awaitEachGesture {
                val start = awaitFirstDown()
                start.consume()
                try {
                    sink(true)
                    ButtonState.set(bit, true)
                    pressFeedback(view, pop)
                    // hasta levantar el dedo: sin umbral de arrastre ni límites
                    drag(start.id) { it.consume() }
                } finally {
                    sink(false)
                    ButtonState.set(bit, false)
                }
            }
        } else {
            detectTapGestures(onPress = {
                sink(true)
                ButtonState.set(bit, true)
                pressFeedback(view, pop)
                tryAwaitRelease()
                sink(false)
                ButtonState.set(bit, false)
            })
        }
    }
}

/**
 * Capa de pulsación de la pantalla, en su caja raíz: mira los dedos después
 * que todos los controles ([PointerEventPass.Final]) y, con «Pulsar
 * deslizando», pulsa y suelta el botón que haya bajo cada uno
 * ([SlideTracker]). Un dedo que se queda otro control (el `down` del stick,
 * la cruceta, los chips, la diana o la pantalla táctil; el arrastre de la
 * tira de scroll) se descarta entero. No consume nada: los demás controles
 * siguen funcionando igual.
 *
 * En un barrido muy rápido los dos botones pueden quedar pulsados a la vez
 * un instante: la retención de 70 ms de [dev.pepotech.pepomote.control.PressLatch]
 * mantiene el primero para que llegue al receptor. Es aposta.
 */
@Composable
fun Modifier.slideCanvas(): Modifier {
    val registry = LocalPressRegistry.current ?: return this
    // Las zonas van en coordenadas de la raíz y los dedos en las de esta
    // caja (que lleva encima los márgenes de la barra de estado y la muesca)
    val origin = remember { RootOrigin() }
    return this
        .onGloballyPositioned { coords ->
            val p = coords.positionInRoot()
            origin.x = p.x
            origin.y = p.y
        }
        .pointerInput(registry) {
            val tracker = SlideTracker { x, y -> PressHit.resolve(registry.zones(), x, y) }
            try {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent(PointerEventPass.Final)
                        val slide = PressMode.slide && registry.enabled
                        val sticky = PressMode.sticky
                        for (change in event.changes) {
                            val id = change.id.value
                            val x = change.position.x + origin.x
                            val y = change.position.y + origin.y
                            val events = when {
                                // Alguien se lo ha quedado: el `down` (stick,
                                // cruceta, chips, diana, táctil) o, ya
                                // arrastrando, la tira de scroll
                                change.isConsumed && change.pressed -> tracker.skip(id)
                                change.pressed && !change.previousPressed ->
                                    tracker.down(id, x, y, slide, sticky)

                                change.pressed -> tracker.move(id, x, y, slide, sticky)
                                else -> tracker.up(id)
                            }
                            registry.dispatch(events)
                        }
                    }
                }
            } finally {
                registry.dispatch(tracker.releaseAll())
            }
        }
}

/**
 * Tarjeta de encima que se queda el dedo. Con «Pulsar deslizando», un dedo que
 * caía en el FONDO de una tarjeta superpuesta (la cabecera desplegada, la
 * pregunta del lado, los avisos) pulsaba el botón que la tarjeta tapa:
 * [slideCanvas] es un nodo de más arriba, ve a todos los dedos en
 * [PointerEventPass.Final] y resuelve las zonas solo por geometría, sin saber
 * qué hay dibujado encima.
 *
 * Aquí el toque se consume en [PointerEventPass.Main], que llega a la tarjeta
 * DESPUÉS que a sus hijos: los chips, «Teclado» y «Salir» de dentro se quedan
 * el suyo como siempre, y lo que no coge nadie lo coge la tarjeta, así
 * [slideCanvas] lo descarta (`tracker.skip`). Sin el ajuste no consume nada:
 * el toque sigue su camino igual que antes.
 */
fun Modifier.pressShield(): Modifier = pointerInput(Unit) {
    awaitPointerEventScope {
        while (true) {
            val event = awaitPointerEvent(PointerEventPass.Main)
            if (!PressMode.slide) continue
            for (change in event.changes) {
                if (!change.isConsumed && change.pressed) change.consume()
            }
        }
    }
}

/** Dónde empieza la caja raíz de la pantalla dentro de la ventana. */
private class RootOrigin(var x: Float = 0f, var y: Float = 0f)

/** Vibración y sonido de una pulsación (solo en el flanco de bajada). */
private fun pressFeedback(view: View, pop: Boolean) {
    Haptics.tap(view, HapticFeedbackConstants.KEYBOARD_TAP)
    if (pop) UiSounds.pop() else UiSounds.blip()
}
