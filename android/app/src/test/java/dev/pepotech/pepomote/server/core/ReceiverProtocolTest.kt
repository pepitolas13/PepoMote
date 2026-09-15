package dev.pepotech.pepomote.server.core

import dev.pepotech.pepomote.net.PmpCodec
import org.junit.Assert.*
import org.junit.Test
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.zip.CRC32

class ReceiverProtocolTest {
    @Test fun goldenNunchukPacketHasIndependentFieldsAndDolphinAnalogButtons() {
        val wire = hex("504d5031010364ceddccbbaa0a00000000093d00000000000000803f00000000000000000000000000000000000000000000000000000000000000000000184100000600004d0000")
        val input = requireNotNull(ReceiverProtocol.parseInput(wire))
        assertEquals(0xaabbccdd.toInt(), input.sessionId)
        assertEquals(10, input.seq)
        assertEquals(4_000_000L, input.tSensorUs)
        assertEquals(100, input.stickX)
        assertEquals(-50, input.stickY)
        assertEquals(77, input.batteryPct)
        val packet = DsuCodec.padData(3, input, ReceiverMode.Dolphin, true, false, 7)
        checkHeader(packet, 100, 0x100002)
        assertEquals(3, u(packet[20]))
        assertArrayEquals(byteArrayOf(0x50, 0x4d, 0x50, 0x31, 0, 3), packet.copyOfRange(24, 30))
        assertEquals(2, u(packet[21]))
        assertEquals(1, u(packet[31]))
        assertEquals(7, buffer(packet).getInt(32))
        assertEquals(12, u(packet[37]))
        assertArrayEquals(byteArrayOf(228.toByte(), 78, 128.toByte(), 128.toByte()), packet.copyOfRange(40, 44))
        assertArrayEquals(ByteArray(4), packet.copyOfRange(48, 52))
        assertArrayEquals(byteArrayOf(-1, -1, 0, 0), packet.copyOfRange(52, 56))
        assertArrayEquals(ByteArray(12), packet.copyOfRange(56, 68))
        assertEquals(4_000_000L, buffer(packet).getLong(68))
    }

    @Test fun exactAxesAndSwitchScaleAreAppliedWithoutTouchContacts() {
        val input = requireNotNull(ReceiverProtocol.parseInput(input(1, flags = 15,
            buttons = (1 shl 0) or (1 shl 8) or (1 shl 23) or (1 shl 28))))
        val wii = DsuCodec.padData(0, input, ReceiverMode.Dolphin, true, true, 1)
        assertEquals(64, u(wii[37]))
        assertEquals(255, u(wii[49]))
        assertEquals(255, u(wii[39]))
        assertArrayEquals(byteArrayOf(228.toByte(), 78, 128.toByte(), 128.toByte()), wii.copyOfRange(40, 44))
        val sw = DsuCodec.padData(0, input, ReceiverMode.Eden, true, true, 2)
        assertEquals(33, u(sw[37]))
        assertEquals(255, u(sw[50]))
        assertEquals(255, u(sw[38]))
        assertEquals(255, u(sw[39]))
        assertArrayEquals(byteArrayOf(227.toByte(), 77, 97, 247.toByte()), sw.copyOfRange(40, 44))
        assertArrayEquals(ByteArray(12), sw.copyOfRange(56, 68))
        for (packet in listOf(wii, sw)) {
            assertEquals(-1f, buffer(packet).getFloat(76), 0.00001f)
            assertEquals(-3f, buffer(packet).getFloat(80), 0.00001f)
            assertEquals(2f, buffer(packet).getFloat(84), 0.00001f)
        }
        assertEquals(57.29578f, buffer(wii).getFloat(88), 0.0001f)
        assertEquals(171.88734f, buffer(wii).getFloat(92), 0.0001f)
        assertEquals(114.59156f, buffer(wii).getFloat(96), 0.0001f)
        assertEquals(49.65634f, buffer(sw).getFloat(88), 0.0001f)
        val noCapture = DsuCodec.padData(0, input.copy(buttons = 0), ReceiverMode.Eden, true, true, 3)
        assertEquals(0, u(noCapture[39]))
    }

    @Test fun buttonTablesAreIndependentForEverySourceBit() {
        val base = requireNotNull(ReceiverProtocol.parseInput(input(1)))
        val wiiBits = mapOf(0 to 14, 1 to 13, 2 to 4, 3 to 6, 4 to 7, 5 to 5, 6 to 3, 7 to 0, 9 to 15, 10 to 12, 17 to 10, 18 to 11)
        val swBits = mapOf(0 to 13, 1 to 14, 2 to 4, 3 to 6, 4 to 7, 5 to 5, 6 to 3, 7 to 0, 19 to 12, 20 to 15, 21 to 10, 22 to 11, 23 to 8, 24 to 9, 25 to 1, 26 to 2)
        for ((mode, table) in listOf(ReceiverMode.Dolphin to wiiBits, ReceiverMode.Eden to swBits)) {
            for (bit in 0..29) {
                val packet = DsuCodec.padData(0, base.copy(buttons = 1 shl bit), mode, true, false, 1)
                assertEquals("$mode source bit $bit", table[bit]?.let { 1 shl it } ?: 0, u(packet[36]) or (u(packet[37]) shl 8))
                assertEquals(if (bit == 8) 255 else 0, u(packet[38]))
                assertEquals(if (mode == ReceiverMode.Eden && bit == 28) 255 else 0, u(packet[39]))
            }
        }
    }

    @Test fun switchGyroKeepsDesktopFloatRoundingOrder() {
        val sample = requireNotNull(ReceiverProtocol.parseInput(input(1))).copy(gyroX = 3f / 37f)
        val packet = DsuCodec.padData(0, sample, ReceiverMode.Eden, true, false, 1)
        // Desktop: f32(f32(radians * (180f / PI_f32)) * (312f / 360f)).
        assertEquals(0x4080d68b, buffer(packet).getInt(88))
    }

    @Test fun strictInputFramingRejectsMalformedPacketsAndNonfiniteNumbers() {
        val valid = input(1)
        assertNotNull(ReceiverProtocol.parseInput(valid))
        assertNull(ReceiverProtocol.parseInput(valid.copyOf(71)))
        assertNull(ReceiverProtocol.parseInput(valid.copyOf(73)))
        assertNull(ReceiverProtocol.parseInput(valid.copyOf(80)))
        assertNull(ReceiverProtocol.parseInput(valid.clone().apply { this[5] = 4 }))
        assertNull(ReceiverProtocol.parseInput(valid.clone().apply { this[5] = 0x10 }))
        assertNull(ReceiverProtocol.parseInput(valid.clone().apply { this[6] = -128 }))
        assertNull(ReceiverProtocol.parseInput(valid.clone().apply { this[69] = 101 }))
        assertNull(ReceiverProtocol.parseInput(valid.clone().apply { buffer(this).putInt(64, Int.MIN_VALUE) }))
        for (offset in 24..60 step 4) {
            for (v in listOf(Float.NaN, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY)) {
                assertNull("nonfinite at $offset", ReceiverProtocol.parseInput(valid.clone().apply { buffer(this).putFloat(offset, v) }))
            }
        }
        assertNull(ReceiverProtocol.parseInput(input(1, flags = 4).apply { this[78] = 1 }))
        assertTrue(ReceiverProtocol.newerSequence(Int.MIN_VALUE, Int.MAX_VALUE))
        assertTrue(ReceiverProtocol.newerSequence(0, -1))
        assertFalse(ReceiverProtocol.newerSequence(9, 9))
        assertFalse(ReceiverProtocol.newerSequence(8, 9))
        assertFalse(ReceiverProtocol.newerSequence(Int.MIN_VALUE, 0))
    }

    @Test fun dsuRequestsRequireMatchingCrcLengthVersionAndSubscription() {
        val request = request(0x100002, byteArrayOf(3, 0, 0x50, 0x4d, 0x50, 0x31, 0, 3))
        assertEquals(9, requireNotNull(DsuCodec.parseRequest(request)).slots)
        assertNull(DsuCodec.parseRequest(request.clone().apply { this[8] = (this[8] + 1).toByte() }))
        assertNull(DsuCodec.parseRequest(request.copyOf(27)))
        assertNull(DsuCodec.parseRequest(request.copyOf(29)))
        assertNull(DsuCodec.parseRequest(request(0x100002, ByteArray(7))))
        assertNull(DsuCodec.parseRequest(request(0x100002, byteArrayOf(4, 0, 0, 0, 0, 0, 0, 0))))
        assertNull(DsuCodec.parseRequest(request(0x100001, byteArrayOf(5, 0, 0, 0, 0, 1, 2, 3, 0))))
        assertNull(DsuCodec.parseRequest(request(0x100001, byteArrayOf(2, 0, 0, 0, 0))))
        assertNotNull(DsuCodec.parseRequest(request(0x100000)))
    }

    companion object {
        fun input(session: Int, seq: Int = 1, flags: Int = 0, buttons: Int = 0, recenter: Int = 0): ByteArray = PmpCodec.encodeInput(
            session, seq, 123_456_789, floatArrayOf(1f, 0f, 0f, 0f), floatArrayOf(1f, 2f, -3f),
            floatArrayOf(9.80665f, 19.6133f, 29.41995f), buttons, recenter, 100, 0, flags, 100, -50, -30, 120, 32768, 16384)
        fun buffer(bytes: ByteArray): ByteBuffer = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        fun u(value: Byte) = value.toInt() and 255
        fun hex(value: String) = value.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
        fun request(type: Int, payload: ByteArray = byteArrayOf()): ByteArray {
            val bytes = ByteBuffer.allocate(20 + payload.size).order(ByteOrder.LITTLE_ENDIAN)
                .put("DSUC".toByteArray()).putShort(1001).putShort((4 + payload.size).toShort())
                .putInt(0).putInt(0x12345678).putInt(type).put(payload).array()
            buffer(bytes).putInt(8, CRC32().apply { update(bytes) }.value.toInt())
            return bytes
        }
        fun checkHeader(bytes: ByteArray, expectedSize: Int, expectedType: Int) {
            assertEquals(expectedSize, bytes.size)
            assertEquals("DSUS", String(bytes, 0, 4))
            assertEquals(1001, buffer(bytes).getShort(4).toInt())
            assertEquals(bytes.size - 16, buffer(bytes).getShort(6).toInt())
            assertEquals(expectedType, buffer(bytes).getInt(16))
            val copy = bytes.clone()
            val actualCrc = buffer(copy).getInt(8)
            buffer(copy).putInt(8, 0)
            assertEquals(CRC32().apply { update(copy) }.value.toInt(), actualCrc)
        }
    }
}
