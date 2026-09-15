package dev.pepotech.pepomote.server.core

import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.zip.CRC32

enum class ReceiverMode(val wire: String) { Dolphin("dolphin"), Eden("switch") }

internal data class ReceiverInput(
    val sessionId: Int, val seq: Int, val tSensorUs: Long,
    val gyroX: Float, val gyroY: Float, val gyroZ: Float,
    val accelX: Float, val accelY: Float, val accelZ: Float,
    val buttons: Int, val recenterCount: Int, val batteryPct: Int,
    val stickX: Int, val stickY: Int, val stickRX: Int, val stickRY: Int,
    val flags: Int,
)

internal object ReceiverProtocol {
    private const val PMP_MAGIC = 0x31504d50

    fun parseInput(bytes: ByteArray): ReceiverInput? {
        if (bytes.size != 72 && bytes.size != 80) return null
        val b = buffer(bytes)
        if (b.getInt(0) != PMP_MAGIC || bytes[4].toInt() != 1) return null
        val flags = bytes[5].toInt() and 255
        val ext = flags and 4 != 0
        if (flags and 0xf0 != 0 || ext != (bytes.size == 80) || (flags and 8 != 0 && !ext)) return null
        if (bytes[6] == (-128).toByte() || bytes[7] == (-128).toByte()) return null
        if (ext && (bytes[72] == (-128).toByte() || bytes[73] == (-128).toByte() || b.getShort(78).toInt() != 0)) return null
        for (offset in 24..60 step 4) if (!b.getFloat(offset).isFinite()) return null
        // Finite wire values must remain finite after conversion to degrees/s.
        for (offset in 40..48 step 4) if (!(b.getFloat(offset) * (180f / Math.PI.toFloat())).isFinite()) return null
        val battery = bytes[69].toInt() and 255
        val buttons = b.getInt(64)
        if (battery > 100 || buttons and 0xc0000000.toInt() != 0) return null
        val stickValid = flags and 2 != 0
        return ReceiverInput(
            b.getInt(8), b.getInt(12), b.getLong(16),
            b.getFloat(40), b.getFloat(44), b.getFloat(48),
            b.getFloat(52), b.getFloat(56), b.getFloat(60),
            buttons, bytes[68].toInt() and 255, battery,
            if (stickValid) bytes[6].toInt() else 0, if (stickValid) bytes[7].toInt() else 0,
            if (ext) bytes[72].toInt() else 0, if (ext) bytes[73].toInt() else 0, flags,
        )
    }

    /** Signed subtraction provides the protocol's forward half-window, including wrap. */
    fun newerSequence(seq: Int, previous: Int): Boolean = seq - previous > 0

    fun pingSession(bytes: ByteArray): Int? = if (bytes.size == 20 &&
        buffer(bytes).getInt(0) == PMP_MAGIC && bytes[4].toInt() in 2..3 &&
        bytes[5] == 0.toByte() && bytes[6] == 0.toByte() && bytes[7] == 0.toByte()
    ) buffer(bytes).getInt(8) else null

    private fun buffer(bytes: ByteArray) = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
}

internal object DsuCodec {
    const val VERSION = 0x100000
    const val PORT_INFO = 0x100001
    const val PAD_DATA = 0x100002
    data class Request(val type: Int, val slots: Int, val ports: List<Int> = emptyList())

    fun parseRequest(bytes: ByteArray): Request? {
        if (bytes.size !in 20..128) return null
        val b = buffer(bytes)
        if (b.getInt(0) != 0x43555344 || b.getShort(4).toInt() != 1001 ||
            (b.getShort(6).toInt() and 65535) != bytes.size - 16) return null
        val crc = CRC32().apply {
            update(bytes, 0, 8)
            update(ByteArray(4))
            update(bytes, 12, bytes.size - 12)
        }.value.toInt()
        if (b.getInt(8) != crc) return null
        return when (val type = b.getInt(16)) {
            VERSION -> if (bytes.size == 20) Request(type, 0) else null
            PORT_INFO -> {
                if (bytes.size < 24) return null
                val count = b.getInt(20)
                if (count !in 1..4 || bytes.size != 24 + count) return null
                val ports = (0 until count).map { bytes[24 + it].toInt() and 255 }
                if (ports.any { it !in 0..3 }) null else Request(type, 0, ports)
            }
            PAD_DATA -> {
                if (bytes.size != 28) return null
                val flags = bytes[20].toInt() and 255
                if (flags and 3 != flags) return null
                if (flags == 0) return Request(type, 15)
                var mask = 0
                val id = bytes[21].toInt() and 255
                if (flags and 1 != 0 && id in 0..3) mask = mask or (1 shl id)
                if (flags and 2 != 0) {
                    for (slot in 0..3) if (bytes.copyOfRange(22, 28).contentEquals(mac(slot))) mask = mask or (1 shl slot)
                }
                Request(type, mask)
            }
            else -> null
        }
    }

    fun version(): ByteArray = finish(ByteBuffer.allocate(6).order(ByteOrder.LITTLE_ENDIAN).putInt(VERSION).putShort(1001).array())

    fun portInfo(slot: Int, connected: Boolean, battery: Int): ByteArray = finish(
        ByteBuffer.allocate(16).order(ByteOrder.LITTLE_ENDIAN).putInt(PORT_INFO).put(info(slot, connected, battery)).put(0).array())

    /** Wire layout and signs deliberately match desktop/src/dsu/{mapping,server}.rs. */
    fun padData(slot: Int, input: ReceiverInput, mode: ReceiverMode, connected: Boolean, recenter: Boolean, counter: Int): ByteArray {
        val sw = mode == ReceiverMode.Eden
        val bits = if (connected) input.buttons else 0
        fun on(bit: Int) = if (bits and (1 shl bit) != 0) 255 else 0
        val b1Sources = if (sw) intArrayOf(7, 25, 26, 6, 2, 5, 3, 4) else intArrayOf(7, -1, -1, 6, 2, 5, 3, 4)
        val b2Sources = if (sw) intArrayOf(23, 24, 21, 22, 19, 0, 1, 20) else intArrayOf(-1, -1, 17, 18, 10, 1, 0, 9)
        fun mask(sources: IntArray) = sources.indices.fold(0) { acc, i -> if (sources[i] >= 0 && on(sources[i]) != 0) acc or (1 shl i) else acc }
        val p = ByteBuffer.allocate(84).order(ByteOrder.LITTLE_ENDIAN)
        p.putInt(PAD_DATA).put(info(slot, connected, input.batteryPct)).put(if (connected) 1 else 0).putInt(counter)
        p.put(mask(b1Sources).toByte()).put(mask(b2Sources).toByte()).put(on(8).toByte())
        p.put((if (sw) on(28) else if (connected && recenter) 255 else 0).toByte())
        val center = if (sw) 127 else 128
        fun axis(value: Int) = (center + if (connected) value else 0).coerceIn(0, 255).toByte()
        p.put(axis(input.stickX)).put(axis(input.stickY)).put(axis(if (sw) input.stickRX else 0)).put(axis(if (sw) input.stickRY else 0))
        for (bit in intArrayOf(4, 3, 5, 2)) p.put(on(bit).toByte())
        for (bit in if (sw) intArrayOf(20, 1, 0, 19) else intArrayOf(9, 0, 1, 10)) p.put(on(bit).toByte())
        for (bit in if (sw) intArrayOf(22, 21, 24, 23) else intArrayOf(18, 17, -1, -1)) p.put((if (bit < 0) 0 else on(bit)).toByte())
        p.put(ByteArray(12)) // Neither Android receiver mode exposes a touch surface.
        p.putLong(input.tSensorUs)
        val radiansToDegrees = 180f / Math.PI.toFloat()
        val profileScale = if (sw) 312f / 360f else 1f
        p.putFloat(if (connected) -input.accelX / 9.80665f else 0f)
        p.putFloat(if (connected) -input.accelZ / 9.80665f else 0f)
        p.putFloat(if (connected) input.accelY / 9.80665f else 0f)
        p.putFloat(if (connected) (input.gyroX * radiansToDegrees) * profileScale else 0f)
        p.putFloat(if (connected) (-input.gyroZ * radiansToDegrees) * profileScale else 0f)
        p.putFloat(if (connected) (input.gyroY * radiansToDegrees) * profileScale else 0f)
        return finish(p.array())
    }

    private fun info(slot: Int, connected: Boolean, battery: Int): ByteArray = ByteArray(11).apply {
        this[0] = slot.toByte()
        if (connected) {
            this[1] = 2; this[2] = 2; this[3] = 2
            mac(slot).copyInto(this, 4)
            this[10] = when (battery) { in 88..100 -> 5; in 63..87 -> 4; in 38..62 -> 3; in 13..37 -> 2; else -> 1 }
        }
    }

    private fun mac(slot: Int) = byteArrayOf(0x50, 0x4d, 0x50, 0x31, 0, slot.toByte())
    private fun buffer(bytes: ByteArray) = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
    private fun finish(payload: ByteArray): ByteArray {
        val out = ByteBuffer.allocate(16 + payload.size).order(ByteOrder.LITTLE_ENDIAN)
            .putInt(0x53555344).putShort(1001).putShort(payload.size.toShort()).putInt(0).putInt(0x50455030).put(payload).array()
        buffer(out).putInt(8, CRC32().apply { update(out) }.value.toInt())
        return out
    }
}
