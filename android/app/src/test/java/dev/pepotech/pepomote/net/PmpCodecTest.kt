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

    /** Emisor Nunchuk: flags bit1, stick en los bytes 6-7, C y Z. */
    @Test
    fun inputNunchuk() {
        val packet = PmpCodec.encodeInput(
            sessionId = 0xAABBCCDD.toInt(),
            seq = 10,
            tSensorUs = 4_000_000,
            quat = floatArrayOf(1f, 0f, 0f, 0f),
            gyro = floatArrayOf(0f, 0f, 0f),
            accel = floatArrayOf(0f, 0f, 9.5f),
            buttons = 0x60000,
            recenterCount = 0,
            batteryPct = 77,
            touchScrollDy = 0,
            flags = PmpCodec.FLAG_QUAT_VALID or PmpCodec.FLAG_STICK_VALID,
            stickX = 100,
            stickY = -50
        )
        assertEquals(vector("input_nunchuk.hex"), packet.toHex())
    }

    /**
     * GamePad de Wii U: 80 bytes con FLAG_EXT, stick izquierdo en 6-7, stick
     * derecho y táctil en 72-77, reservados 78-79 a 0, botones Wii U.
     */
    @Test
    fun inputWiiU() {
        val buttons = 0x1FF80001 // A|X|Y|L|R|ZL|ZR|STICK_L|STICK_R|MIC|SCREEN
        assertEquals(
            buttons,
            (1 shl 0) or (1 shl 19) or (1 shl 20) or (1 shl 21) or (1 shl 22) or (1 shl 23) or
                (1 shl 24) or (1 shl 25) or (1 shl 26) or (1 shl 27) or (1 shl 28)
        )
        val packet = PmpCodec.encodeInput(
            sessionId = 0xAABBCCDD.toInt(),
            seq = 11,
            tSensorUs = 5_000_000,
            quat = floatArrayOf(1f, 0f, 0f, 0f),
            gyro = floatArrayOf(0f, 0f, 0f),
            accel = floatArrayOf(0f, 0f, 9.5f),
            buttons = buttons,
            recenterCount = 2,
            batteryPct = 66,
            touchScrollDy = 0,
            flags = PmpCodec.FLAG_QUAT_VALID or PmpCodec.FLAG_STICK_VALID or
                PmpCodec.FLAG_EXT or PmpCodec.FLAG_TOUCH,
            stickX = 100,
            stickY = -50,
            stick2X = -30,
            stick2Y = 120,
            touchX = 0x8000,
            touchY = 0x4000
        )
        assertEquals(PmpCodec.INPUT_EXT_LEN, packet.size)
        assertEquals(vector("input_wiiu.hex"), packet.toHex())
    }

    /** Sin FLAG_EXT el bloque Wii U no existe: 72 bytes, pase lo que se pase. */
    @Test
    fun extensionSoloConSuFlag() {
        val packet = PmpCodec.encodeInput(
            sessionId = 0xAABBCCDD.toInt(),
            seq = 10,
            tSensorUs = 4_000_000,
            quat = floatArrayOf(1f, 0f, 0f, 0f),
            gyro = floatArrayOf(0f, 0f, 0f),
            accel = floatArrayOf(0f, 0f, 9.5f),
            buttons = 0x60000,
            recenterCount = 0,
            batteryPct = 77,
            touchScrollDy = 0,
            flags = PmpCodec.FLAG_QUAT_VALID or PmpCodec.FLAG_STICK_VALID,
            stickX = 100,
            stickY = -50,
            stick2X = -30,
            stick2Y = 120,
            touchX = 0x8000,
            touchY = 0x4000
        )
        assertEquals(PmpCodec.INPUT_LEN, packet.size)
        assertEquals(vector("input_nunchuk.hex"), packet.toHex())

        // Con FLAG_EXT y el táctil en el tope: u16 sin signo, reservados a 0
        val ext = PmpCodec.encodeInput(
            sessionId = 1, seq = 1, tSensorUs = 1,
            quat = floatArrayOf(1f, 0f, 0f, 0f),
            gyro = floatArrayOf(0f, 0f, 0f),
            accel = floatArrayOf(0f, 0f, 0f),
            buttons = 0, recenterCount = 0, batteryPct = 100, touchScrollDy = 0,
            flags = PmpCodec.FLAG_EXT,
            stick2X = -1, stick2Y = 2, touchX = 65535, touchY = 70000
        )
        assertEquals(PmpCodec.INPUT_EXT_LEN, ext.size)
        assertEquals("ff02ffffffff0000", ext.copyOfRange(72, 80).toHex())
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
