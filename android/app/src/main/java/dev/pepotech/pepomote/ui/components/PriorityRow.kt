package dev.pepotech.pepomote.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.Placeable
import androidx.compose.ui.layout.layoutId
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * Hueco de una cabecera con prioridad ([PriorityRow]). El orden de declaración
 * es el orden visual (de izquierda a derecha); [priority] dice quién se queda
 * cuando no cabe todo (0 = nunca cae); [end]: pegado al borde derecho;
 * [shrinks]: antes de caer se encoge a lo que quede (texto con puntos
 * suspensivos).
 */
enum class HeaderSlot(val priority: Int, val end: Boolean, val shrinks: Boolean = false) {
    /** Nombre del PC. */
    Pc(priority = 4, end = false),
    /** Estado («J1 · GamePad · 23 ms», «Conectando…»). */
    Status(priority = 3, end = false, shrinks = true),
    /** Ritmo de la doble pantalla. */
    Fps(priority = 5, end = false),
    /** Chips de modo Puntero / Dolphin / Wii U. */
    Chips(priority = 2, end = true),
    /** Botón «Teclado». */
    Keyboard(priority = 1, end = true),
    /** «Salir»: siempre. */
    Exit(priority = 0, end = true)
}

private val Measurable.slot: HeaderSlot
    get() = layoutId as? HeaderSlot ?: error("PriorityRow: cada hijo lleva Modifier.layoutId(HeaderSlot.X)")

/**
 * Fila de cabecera que mide por prioridad y coloca en orden visual, para que
 * «Salir» nunca se salga de la pantalla por estrecho que sea el móvil o grande
 * que sea la letra. (Un `Row` mide a cada hijo con lo que le dejan los
 * anteriores, así que el último, «Salir», era el que se quedaba sin sitio.)
 *
 * Cada hijo lleva `Modifier.layoutId(HeaderSlot.X)`. Se miden de mayor a menor
 * prioridad, cada uno una sola vez, y entran solo si caben enteros en lo que
 * queda: el de prioridad 0 siempre; los [HeaderSlot.shrinks], también
 * recortados si les quedan al menos [minShrunk]. El que no cabe ni se mide, ni
 * se pinta, ni recibe toques.
 *
 * Con [fill] ocupa todo el ancho: los huecos `end` pegados a la derecha y el
 * resto a la izquierda; sin él, todo seguido y tan ancho como el contenido (el
 * padre lo centra). Necesita ancho acotado.
 */
@Composable
fun PriorityRow(
    modifier: Modifier = Modifier,
    spacing: Dp = 8.dp,
    fill: Boolean = true,
    minShrunk: Dp = 60.dp,
    content: @Composable () -> Unit
) {
    Layout(content, modifier) { measurables, constraints ->
        val gap = spacing.roundToPx()
        val minShrunkPx = minShrunk.roundToPx()
        val maxH = constraints.maxHeight
        val avail = constraints.maxWidth
        var left = avail
        val placed = ArrayList<Pair<HeaderSlot, Placeable>>(measurables.size)

        // Medir por prioridad: una sola medida por hijo y solo si cabe (la
        // prueba usa el ancho intrínseco; medir dos veces está prohibido)
        for (m in measurables.sortedBy { it.slot.priority }) {
            val slot = m.slot
            val need = if (placed.isEmpty()) 0 else gap
            val room = left - need
            val fits = when {
                placed.isEmpty() -> true
                slot.shrinks -> room >= minOf(m.maxIntrinsicWidth(maxH), minShrunkPx)
                else -> m.maxIntrinsicWidth(maxH) <= room
            }
            if (!fits) continue
            val p = m.measure(Constraints(maxWidth = room.coerceAtLeast(0), maxHeight = maxH))
            placed += slot to p
            left -= p.width + need
        }

        // Colocar en orden visual (el de declaración de HeaderSlot), centrado
        // en vertical
        val visual = placed.sortedBy { it.first.ordinal }
        val start = visual.filter { !it.first.end }.map { it.second }
        val end = visual.filter { it.first.end }.map { it.second }
        fun List<Placeable>.span() = sumOf { it.width } + gap * (size - 1).coerceAtLeast(0)
        val between = if (start.isNotEmpty() && end.isNotEmpty()) gap else 0
        val used = start.span() + between + end.span()
        val width = if (fill && constraints.hasBoundedWidth) avail else used.coerceIn(constraints.minWidth, avail)
        val tallest = visual.maxOfOrNull { it.second.height } ?: 0
        val height = tallest.coerceIn(constraints.minHeight, maxH)
        layout(width, height) {
            var x = 0
            for (p in start) {
                p.placeRelative(x, (height - p.height) / 2)
                x += p.width + gap
            }
            if (fill) x = width - end.span()
            for (p in end) {
                p.placeRelative(x, (height - p.height) / 2)
                x += p.width + gap
            }
        }
    }
}
