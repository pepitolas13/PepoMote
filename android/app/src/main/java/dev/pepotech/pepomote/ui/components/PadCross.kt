package dev.pepotech.pepomote.ui.components

import android.view.HapticFeedbackConstants
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.GenericShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.pepotech.pepomote.control.ButtonState
import dev.pepotech.pepomote.control.DpadModel
import dev.pepotech.pepomote.control.UiSounds
import dev.pepotech.pepomote.ui.theme.PepoColors
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

/**
 * Cruceta de una sola pieza, como la del Mando de Wii: una cruz con las
 * puntas redondeadas y una marca en relieve en cada brazo, sin flechas. El
 * dedo se sigue mientras está apoyado ([DpadModel]): deslizarlo cambia de
 * dirección sin levantarlo, las esquinas entre dos brazos son diagonales y
 * el centro muerto es pequeño. El brazo pulsado se ilumina; cada dirección
 * nueva hace clic (háptica y sonido), como en un mando de verdad. En modo
 * puntero el receptor hace ↑/↓ = flechas del PC y ←/→ = atrás/adelante del
 * navegador. Con [sideways] (mando de lado en un juego) manda los botones
 * del mando girado con el IR a la izquierda ([SidewaysDpad]).
 */
@Composable
fun PadCross(sizeDp: Dp, sideways: Boolean = false) {
    val view = LocalView.current
    var dirs by remember { mutableIntStateOf(0) }
    val glow = PepoColors.Glow
    val mark = PepoColors.CardBorder

    Box(
        modifier = Modifier
            .size(sizeDp)
            .shadow(3.dp, PlusShape, clip = false)
            .background(PepoColors.Card, PlusShape)
            .border(1.5.dp, PepoColors.CardBorder, PlusShape)
            .drawBehind {
                val g = DpadGeometry(size.width / 2f)
                val c = center
                for (dir in 0 until 4) {
                    if (dirs and DpadGeometry.FLAGS[dir] != 0) drawPath(g.arm(c, dir), glow)
                }
                val t = max(2.dp.toPx(), g.a * 0.14f)
                for (dir in 0 until 4) {
                    val (topLeft, markSize) = g.mark(c, dir, t)
                    drawRoundRect(mark, topLeft, markSize, CornerRadius(t / 2f))
                }
            }
            .pointerInput(sideways) {
                var held = 0
                fun release() {
                    for (bit in DpadModel.BUTTON_BITS) if (held and bit != 0) ButtonState.set(bit, false)
                    held = 0
                    dirs = 0
                }
                try {
                    awaitEachGesture {
                        val half = size.width / 2f
                        val c = Offset(half, size.height / 2f)
                        fun apply(pos: Offset) {
                            val d = DpadModel.dirs(pos.x - c.x, pos.y - c.y, half)
                            val bits = DpadModel.buttons(d, sideways)
                            if (bits == held) return
                            for (bit in DpadModel.BUTTON_BITS) {
                                val was = held and bit != 0
                                val now = bits and bit != 0
                                if (was != now) ButtonState.set(bit, now)
                            }
                            // Algo nuevo pulsado (no solo soltado): clic
                            if (bits and held.inv() != 0) {
                                view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                                UiSounds.blip()
                            }
                            held = bits
                            dirs = d
                        }
                        val down = awaitFirstDown()
                        down.consume()
                        apply(down.position)
                        drag(down.id) { change ->
                            change.consume()
                            apply(change.position)
                        }
                        release()
                    }
                } finally {
                    // Cambio de [sideways] o salida de la pantalla con el dedo puesto
                    release()
                }
            }
    )
}

/**
 * Geometría de la cruz (los mismos números que en iOS y en el móvil Linux):
 * media anchura [half] → brazos de media anchura [a] (un tercio), puntas
 * redondeadas con [rOut] y esquinas interiores con [rIn].
 */
internal class DpadGeometry(val half: Float) {
    val a = half / 3f
    val rOut = a * 0.55f
    val rIn = a * 0.28f

    /** Contorno completo: 12 esquinas en sentido horario desde la interior superior izquierda. */
    fun outline(c: Offset): Path {
        val h = half
        val pts = listOf(
            -a to -a, -a to -h, a to -h, a to -a, h to -a, h to a,
            a to a, a to h, -a to h, -a to a, -h to a, -h to -a
        )
        val radii = FloatArray(12) { if (it % 3 == 0) rIn else rOut }
        return roundedPolygon(pts.map { Offset(c.x + it.first, c.y + it.second) }, radii)
    }

    /** Brazo [dir] (0 ↑, 1 →, 2 ↓, 3 ←): de la arista del cuadrado central a la punta. */
    fun arm(c: Offset, dir: Int): Path {
        val up = listOf(-a to -a, -a to -half, a to -half, a to -a)
        val pts = up.map { (x, y) ->
            var p = x to y
            repeat(dir) { p = -p.second to p.first } // 90 grados en sentido horario
            Offset(c.x + p.first, c.y + p.second)
        }
        return roundedPolygon(pts, floatArrayOf(0f, rOut, rOut, 0f))
    }

    /** Marca en relieve del brazo [dir], a lo largo del brazo y cerca de la punta: esquina y tamaño. */
    fun mark(c: Offset, dir: Int, t: Float): Pair<Offset, Size> {
        val len = a * 0.7f
        val at = half * 0.72f
        return when (dir) {
            0 -> Offset(c.x - t / 2f, c.y - at - len / 2f) to Size(t, len)
            1 -> Offset(c.x + at - len / 2f, c.y - t / 2f) to Size(len, t)
            2 -> Offset(c.x - t / 2f, c.y + at - len / 2f) to Size(t, len)
            else -> Offset(c.x - at - len / 2f, c.y - t / 2f) to Size(len, t)
        }
    }

    companion object {
        /** Dirección de [DpadModel] de cada brazo (0 ↑, 1 →, 2 ↓, 3 ←). */
        val FLAGS = intArrayOf(DpadModel.UP, DpadModel.RIGHT, DpadModel.DOWN, DpadModel.LEFT)
    }
}

/** Polígono con cada esquina redondeada (curva cuadrática; radio ≤ la mitad del lado más corto). */
internal fun roundedPolygon(p: List<Offset>, r: FloatArray): Path {
    val path = Path()
    val n = p.size
    for (i in 0 until n) {
        val prev = p[(i + n - 1) % n]
        val cur = p[i]
        val next = p[(i + 1) % n]
        val rr = min(r[i], min(dist(prev, cur), dist(cur, next)) / 2f)
        val start = towards(cur, prev, rr)
        val end = towards(cur, next, rr)
        if (i == 0) path.moveTo(start.x, start.y) else path.lineTo(start.x, start.y)
        path.quadraticTo(cur.x, cur.y, end.x, end.y)
    }
    path.close()
    return path
}

private fun dist(a: Offset, b: Offset): Float = sqrt((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y))

private fun towards(from: Offset, to: Offset, d: Float): Offset {
    val len = dist(from, to)
    return if (len == 0f) from else Offset(from.x + (to.x - from.x) * d / len, from.y + (to.y - from.y) * d / len)
}

/** La cruz como forma de Compose (fondo, borde y sombra). */
internal val PlusShape = GenericShape { size, _ ->
    addPath(DpadGeometry(size.width / 2f).outline(Offset(size.width / 2f, size.height / 2f)))
}
