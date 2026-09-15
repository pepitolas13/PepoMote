package dev.pepotech.pepomote.control

import java.util.ArrayDeque

/**
 * Protects button pulses at the packet boundary. Each bit has its own queue:
 * holding A long enough for a game frame never blocks a new B press.
 * Deadlines start when a changed state is actually sent, not when it was queued.
 * All methods belong to the serialized sender thread.
 */
internal class ButtonDelivery(
    private val minPressNs: Long = 70_000_000L,
    private val minGapNs: Long = 20_000_000L,
) {
    private class Bit {
        val pending = ArrayDeque<Boolean>()
        var sentAtNs: Long? = null
    }

    private val bits = arrayOfNulls<Bit>(32)
    private var requested = 0
    private var sent = 0
    private var resetPending = false

    fun accept(buttons: Int) {
        var changed = requested xor buttons
        while (changed != 0) {
            val index = Integer.numberOfTrailingZeros(changed)
            val bit = 1 shl index
            val state = bits[index] ?: Bit().also { bits[index] = it }
            state.pending.addLast(buttons and bit != 0)
            changed = changed xor bit
        }
        requested = buttons
    }

    /** A screen/mode exit cancels queued pulses and bypasses the minimum hold. */
    fun reset() {
        bits.fill(null)
        requested = 0
        resetPending = sent != 0
    }

    /** Earliest useful send; null means there is no pending change. */
    fun nextChangeAtNs(): Long? {
        if (resetPending) return 0L
        var next: Long? = null
        bits.forEachIndexed { index, state ->
            if (state != null && state.pending.isNotEmpty()) {
                val due = dueAtNs(index, state)
                if (next == null || due < next!!) next = due
            }
        }
        return next
    }

    /** Select at most one edge per bit; unchanged packets keep their hold clock. */
    fun buttonsForPacket(nowNs: Long): Int {
        if (resetPending) return 0
        var buttons = sent
        bits.forEachIndexed { index, state ->
            if (state != null && state.pending.isNotEmpty() && dueAtNs(index, state) <= nowNs) {
                val bit = 1 shl index
                buttons = if (state.pending.first) buttons or bit else buttons and bit.inv()
            }
        }
        return buttons
    }

    fun packetSent(buttons: Int, nowNs: Long) {
        if (resetPending) {
            sent = buttons
            resetPending = false
            return
        }
        var changed = sent xor buttons
        while (changed != 0) {
            val index = Integer.numberOfTrailingZeros(changed)
            val bit = 1 shl index
            val state = requireNotNull(bits[index])
            state.pending.removeFirst()
            state.sentAtNs = nowNs
            changed = changed xor bit
        }
        sent = buttons
    }

    private fun dueAtNs(index: Int, state: Bit): Long {
        val last = state.sentAtNs ?: return 0L
        return last + if (sent and (1 shl index) != 0) minPressNs else minGapNs
    }
}
