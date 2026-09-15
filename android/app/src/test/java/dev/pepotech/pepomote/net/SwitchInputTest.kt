package dev.pepotech.pepomote.net

import dev.pepotech.pepomote.control.ButtonState
import org.junit.Assert.*
import org.junit.Test
import java.nio.ByteBuffer
import java.nio.ByteOrder

class SwitchInputTest {
    @Test fun switchCarriesBothSticksAndCaptureButNeverTouchOrMicrophone() {
        val packet = PmpCodec.encodeInput(
            sessionId = 1, seq = 1, tSensorUs = 1000,
            quat = floatArrayOf(1f, 0f, 0f, 0f), gyro = FloatArray(3), accel = FloatArray(3),
            buttons = ButtonState.SCREEN or ButtonState.A or ButtonState.MIC,
            recenterCount = 0, batteryPct = 100, touchScrollDy = 0,
            flags = PmpCodec.FLAG_QUAT_VALID or PmpCodec.FLAG_STICK_VALID or PmpCodec.FLAG_EXT or PmpCodec.FLAG_TOUCH,
            stickX = 127, stickY = -127, stick2X = -64, stick2Y = 32,
            touchX = 500, touchY = 600, switchPad = true
        )
        assertEquals(80, packet.size)
        assertEquals(7, packet[5].toInt())
        assertEquals(127, packet[6].toInt())
        assertEquals(-127, packet[7].toInt())
        assertEquals(-64, packet[72].toInt())
        assertEquals(32, packet[73].toInt())
        assertEquals((1 shl 28) or 1, ByteBuffer.wrap(packet, 64, 4).order(ByteOrder.LITTLE_ENDIAN).int)
        assertTrue(packet.copyOfRange(74, 80).all { it == 0.toByte() })
    }
}
