package dev.pepotech.pepomote.net

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Paridad con los vectores dorados de protocol/vectors/ (los mismos que
 * verifica el receptor Rust). Si esto falla, el codec ha divergido del spec.
 */
class PmpCodecTest {

    private fun vector(name: String): String {
        val stream = javaClass.classLoader!!.getResourceAsStream(name)
            ?: error("No encuentro el vector $name (¿sourceSets de test?)")
        return stream.bufferedReader().readText().filter { !it.isWhitespace() }
    }

    private fun ByteArray.toHex() = joinToString("") { "%02x".format(it) }

    @Test
    fun inputNeutral() {
        val packet = PmpCodec.encodeInput(
            sessionId = 0xAABBCCDD.toInt(),
            seq = 7,
            tSensorUs = 1_000_000,
            quat = floatArrayOf(1f, 0f, 0f, 0f),
            gyro = floatArrayOf(0f, 0f, 0f),
            accel = floatArrayOf(0f, 0f, 0f),
            buttons = 0,
            recenterCount = 0,
            batteryPct = 100,
            touchScrollDy = 0
        )
        assertEquals(vector("input_neutral.hex"), packet.toHex())
    }

    @Test
    fun inputMotion() {
        val packet = PmpCodec.encodeInput(
            sessionId = 0xAABBCCDD.toInt(),
            seq = 8,
            tSensorUs = 2_000_000,
            quat = floatArrayOf(0.5f, -0.5f, 0.5f, -0.5f),
            gyro = floatArrayOf(1f, -1f, 0.5f),
            accel = floatArrayOf(-1f, 2f, -0.5f),
            buttons = 0x41,
            recenterCount = 1,
            batteryPct = 50,
            touchScrollDy = -12
        )
        assertEquals(vector("input_motion.hex"), packet.toHex())
    }

    @Test
    fun inputButtonsAll() {
        val packet = PmpCodec.encodeInput(
            sessionId = 0xAABBCCDD.toInt(),
            seq = 9,
            tSensorUs = 3_000_000,
            quat = floatArrayOf(1f, 0f, 0f, 0f),
            gyro = floatArrayOf(0f, 0f, 0f),
            accel = floatArrayOf(0f, 0f, 0f),
            buttons = 0x0001FFFF,
            recenterCount = 3,
            batteryPct = 87,
            touchScrollDy = -120
        )
        assertEquals(vector("input_buttons_all.hex"), packet.toHex())
    }

    @Test
    fun pingPong() {
        assertEquals(
            vector("ping.hex"),
            PmpCodec.encodePing(0xAABBCCDD.toInt(), 0x0102030405060708L).toHex()
        )
        assertEquals(
            vector("pong.hex"),
            PmpCodec.encodePong(0xAABBCCDD.toInt(), 0x0102030405060708L).toHex()
        )
    }
}
