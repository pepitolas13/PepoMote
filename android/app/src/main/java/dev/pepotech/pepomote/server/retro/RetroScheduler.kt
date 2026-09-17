package dev.pepotech.pepomote.server.retro

import dev.pepotech.pepomote.server.core.ReceiverInput
import java.util.ArrayDeque

/**
 * Planificador puro (sin sockets ni relojes): qué datagrama toca por jugador
 * en cada fotograma, igual que `Scheduler` en `desktop/src/retroarch/link.rs`.
 *
 * Con el hueco de cada fotograma, por jugador, en este orden: el flanco de
 * botón más antiguo pendiente (una pulsación breve son dos fotogramas y no se
 * pierde), el eje que más se ha alejado de lo último enviado, y si no hay nada
 * nuevo, un refresco rotatorio de los controles que no están a cero.
 */
class RetroScheduler(private val players: Int = 4) {
    /** Tope de flancos de botón en cola por jugador: por encima se compacta a un flanco por botón. */
    private val maxEvents = 64

    private inner class User {
        var present = false
        var desired = PadState()
        var known = PadState()
        val events = ArrayDeque<Pair<Int, Boolean>>()
        var dirtyAxes = 0
        var cursor = 0

        fun queueButton(id: Int, down: Boolean) {
            val last = events.descendingIterator().asSequence().firstOrNull { it.first == id }
            if (last != null && last.second == down) return
            events.addLast(id to down)
            if (events.size > maxEvents) {
                events.clear()
                for (b in 0 until 16) if (desired.button(b) != known.button(b)) events.addLast(b to desired.button(b))
            }
        }

        fun push(state: PadState) {
            val changed = state.buttons xor desired.buttons
            var buttons = desired.buttons
            for (id in 0 until 16) {
                if (changed and (1 shl id) != 0) {
                    val down = state.buttons and (1 shl id) != 0
                    buttons = if (down) buttons or (1 shl id) else buttons and (1 shl id).inv()
                    // La compactación debe ver también el flanco que llena la cola.
                    desired = PadState(buttons, desired.axes)
                    queueButton(id, down)
                }
            }
            desired = PadState(buttons, state.axes)
            for (a in 0 until 4) {
                dirtyAxes = if (state.axes[a] != known.axes[a]) dirtyAxes or (1 shl a) else dirtyAxes and (1 shl a).inv()
            }
        }

        /** RetroArch acaba de arrancar (o se perdió de vista): parte de cero. */
        fun resync() {
            known = PadState()
            events.clear()
            for (id in 0 until 16) if (desired.button(id)) events.addLast(id to true)
            dirtyAxes = 0
            for (a in 0 until 4) if (desired.axes[a] != 0) dirtyAxes = dirtyAxes or (1 shl a)
        }

        fun release() {
            // Al caducar o salir, se descartan pulsaciones antiguas y solo se suelta lo ya enviado.
            events.clear()
            desired = known
            push(PadState())
        }

        /** Hay algo que mandar o que mantener vivo. */
        fun active(): Boolean = present || events.isNotEmpty() || dirtyAxes != 0 || !known.isZero

        /** El datagrama de este fotograma (y lo que RetroArch tendrá tras leerlo). */
        fun next(): Pair<Control, Int>? {
            events.pollFirst()?.let { (id, down) ->
                val buttons = if (down) known.buttons or (1 shl id) else known.buttons and (1 shl id).inv()
                known = PadState(buttons, known.axes)
                return Control.button(id) to (if (down) 1 else 0)
            }
            if (dirtyAxes != 0) {
                val a = (0 until 4).filter { dirtyAxes and (1 shl it) != 0 }
                    .maxByOrNull { Math.abs(desired.axes[it] - known.axes[it]) } ?: 0
                dirtyAxes = dirtyAxes and (1 shl a).inv()
                val v = desired.axes[a]
                known = PadState(known.buttons, known.axes.toMutableList().also { it[a] = v })
                return Control.axis(a) to v
            }
            repeat(RetroProtocol.CONTROLS) {
                val c = Control(cursor)
                cursor = (cursor + 1) % RetroProtocol.CONTROLS
                val v = RetroProtocol.value(known, c)
                if (v != 0) return c to v
            }
            return null
        }
    }

    private val users = Array(players) { User() }
    /** Teclas de mantener sostenidas ahora, por slot (bit = índice en HOTKEYS). */
    private val holds = LongArray(players)
    /** Comandos de un fotograma pendientes de enviar. */
    private val pulses = ArrayDeque<String>()
    /** Últimos bits del INPUT por slot (flancos de Home → menú). */
    private val lastBits = IntArray(players)

    fun setPresent(slot: Int, present: Boolean) {
        val u = users[slot]
        if (u.present && !present) {
            // El móvil se va con algo pulsado: RetroArch lo tendría pulsado para siempre. Soltar todo antes de callar.
            release(slot)
        }
        u.present = present
    }

    fun push(slot: Int, state: PadState) = users[slot].push(state)

    /** Suelta botones, ejes y teclas mantenidas sin desconectar al jugador. */
    fun release(slot: Int) {
        users[slot].release()
        holds[slot] = 0
        lastBits[slot] = 0
    }

    /** Un paquete INPUT del móvil `slot` que hace de mando `kind`. */
    internal fun pushInput(slot: Int, kind: RetroPadKind, p: ReceiverInput) {
        push(slot, RetroMapping.padState(kind, p))
        val last = lastBits[slot]
        if (p.buttons and RetroMapping.BIT_MENU != 0 && last and RetroMapping.BIT_MENU == 0) pulse("menu")
        hold(slot, "fast_forward", kind == RetroPadKind.RetroPad && p.buttons and RetroMapping.BIT_FAST_FORWARD != 0)
        lastBits[slot] = p.buttons
    }

    /** Tecla rápida por nombre del protocolo (mensaje `hotkey`). False si el nombre no existe. */
    fun hotkey(slot: Int, name: String, down: Boolean): Boolean {
        val h = RetroProtocol.hotkey(name) ?: return false
        if (h.hold) hold(slot, name, down) else if (down) pulses.addLast(h.command)
        return true
    }

    private fun hold(slot: Int, name: String, down: Boolean) {
        val i = RetroProtocol.HOTKEYS.indexOfFirst { it.name == name }
        if (i < 0) return
        holds[slot] = if (down) holds[slot] or (1L shl i) else holds[slot] and (1L shl i).inv()
    }

    private fun pulse(name: String) {
        RetroProtocol.hotkey(name)?.let { pulses.addLast(it.command) }
    }

    /** Comandos de un fotograma que esperan (se vacía). */
    fun takePulses(): List<String> {
        val out = pulses.toList()
        pulses.clear()
        return out
    }

    /** Comandos de mantener a repetir en este fotograma (unión de todos los móviles). */
    fun heldCommands(): List<String> {
        val all = holds.fold(0L) { a, b -> a or b }
        return RetroProtocol.HOTKEYS.withIndex().filter { (i, h) -> h.hold && all and (1L shl i) != 0L }.map { it.value.command }
    }

    fun resync() = users.forEach { it.resync() }

    /** Los datagramas de este fotograma: (slot, control, valor). */
    fun frame(): List<Triple<Int, Control, Int>> = users.withIndex().mapNotNull { (slot, u) ->
        if (!u.active()) null else u.next()?.let { (c, v) -> Triple(slot, c, v) }
    }

    /** Solo los cambios pendientes, sin refresco (modo degradado). */
    fun changes(): List<Triple<Int, Control, Int>> = users.withIndex().mapNotNull { (slot, u) ->
        if (u.events.isEmpty() && u.dirtyAxes == 0) null else u.next()?.let { (c, v) -> Triple(slot, c, v) }
    }

    fun anyPresent(): Boolean = users.any { it.present }

    fun known(slot: Int): PadState = users[slot].known

    /** Solo para tests: flancos en cola de un jugador. */
    internal fun queuedEvents(slot: Int): Int = users[slot].events.size

    internal fun isActive(slot: Int): Boolean = users[slot].active()
}
