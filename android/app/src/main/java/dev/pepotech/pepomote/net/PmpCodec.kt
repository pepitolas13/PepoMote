package dev.pepotech.pepomote.net

import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Codec del protocolo PMP v1. Fuente de verdad: protocol/PROTOCOL.md.
 * Paridad garantizada por los vectores dorados (PmpCodecTest ↔ tests de Rust).
 */
object PmpCodec {
    const val MAGIC = 0x31504D50 // "PMP1" LE
    const val TYPE_INPUT: Byte = 0x01
    const val TYPE_PING: Byte = 0x02
    const val TYPE_PONG: Byte = 0x03
    const val INPUT_LEN = 72

    /** INPUT con el bloque de extensión Wii U (FLAG_EXT): bytes 72-79. */
    const val INPUT_EXT_LEN = 80
    const val PING_LEN = 20

    val DISCOVER = "PMPDISCOVER1".toByteArray(Charsets.US_ASCII)
    const val HERE_PREFIX = "PMPHERE1 "

    /** flags bit0: el quaternion es válido (hay GAME_ROTATION_VECTOR). */
    const val FLAG_QUAT_VALID = 1

    /** flags bit1: bytes 6-7 llevan el stick (Nunchuk o izquierdo del GamePad). */
    const val FLAG_STICK_VALID = 2

    /**
     * flags bit2: el paquete mide 80 bytes y los bytes 72-79 llevan el bloque
     * Wii U (stick derecho + táctil). Solo se emite con el receptor en modo
     * `cemu` confirmado: un receptor antiguo nunca lo confirma y así nunca
     * recibe 80 bytes.
     */
    const val FLAG_EXT = 4

    /** flags bit3: hay un dedo en la pantalla táctil del GamePad (touch_x/y válidos). */
    const val FLAG_TOUCH = 8

    /**
     * INPUT de 72 bytes, u 80 si `flags` lleva [FLAG_EXT] (entonces van
     * detrás el stick derecho, el táctil y dos bytes reservados a 0).
     */
    fun encodeInput(
        sessionId: Int,
        seq: Int,
        tSensorUs: Long,
        quat: FloatArray, // w, x, y, z
        gyro: FloatArray, // rad/s
        accel: FloatArray, // m/s²
        buttons: Int,
        recenterCount: Int,
        batteryPct: Int,
        touchScrollDy: Int,
        flags: Int = 0,
        stickX: Int = 0, // Nunchuk / stick izq: −127..127, + derecha (0 en un Wiimote)
        stickY: Int = 0, // Nunchuk / stick izq: −127..127, + arriba (0 en un Wiimote)
        stick2X: Int = 0, // stick derecho del GamePad (solo con FLAG_EXT)
        stick2Y: Int = 0,
        touchX: Int = 0, // táctil 0..65535, 0 = borde izquierdo (solo con FLAG_EXT)
        touchY: Int = 0 // táctil 0..65535, 0 = arriba
    ): ByteArray {
        val ext = flags and FLAG_EXT != 0
        val buf = ByteBuffer.allocate(if (ext) INPUT_EXT_LEN else INPUT_LEN)
            .order(ByteOrder.LITTLE_ENDIAN)
        buf.putInt(MAGIC)
        buf.put(TYPE_INPUT)
        buf.put((flags and 0xFF).toByte())
        buf.put(stickX.coerceIn(-127, 127).toByte())
        buf.put(stickY.coerceIn(-127, 127).toByte())
        buf.putInt(sessionId)
        buf.putInt(seq)
        buf.putLong(tSensorUs)
        buf.putFloat(quat[0]); buf.putFloat(quat[1]); buf.putFloat(quat[2]); buf.putFloat(quat[3])
        buf.putFloat(gyro[0]); buf.putFloat(gyro[1]); buf.putFloat(gyro[2])
        buf.putFloat(accel[0]); buf.putFloat(accel[1]); buf.putFloat(accel[2])
        buf.putInt(buttons)
        buf.put((recenterCount and 0xFF).toByte())
        buf.put((batteryPct.coerceIn(0, 100)).toByte())
        buf.putShort(touchScrollDy.coerceIn(-32768, 32767).toShort())
        if (ext) {
            buf.put(stick2X.coerceIn(-127, 127).toByte())
            buf.put(stick2Y.coerceIn(-127, 127).toByte())
            buf.putShort(touchX.coerceIn(0, 0xFFFF).toShort()) // u16 LE
            buf.putShort(touchY.coerceIn(0, 0xFFFF).toShort())
            buf.putShort(0) // 78-79 reservados
        }
        return buf.array()
    }

    fun encodePing(sessionId: Int, tUs: Long): ByteArray = encodePingLike(TYPE_PING, sessionId, tUs)
    fun encodePong(sessionId: Int, tUs: Long): ByteArray = encodePingLike(TYPE_PONG, sessionId, tUs)

    private fun encodePingLike(type: Byte, sessionId: Int, tUs: Long): ByteArray {
        val buf = ByteBuffer.allocate(PING_LEN).order(ByteOrder.LITTLE_ENDIAN)
        buf.putInt(MAGIC)
        buf.put(type)
        buf.put(0)
        buf.putShort(0)
        buf.putInt(sessionId)
        buf.putLong(tUs)
        return buf.array()
    }

    /** Devuelve el tipo del paquete o null si no es PMP. */
    fun packetType(data: ByteArray, len: Int): Byte? {
        if (len < 12) return null
        val buf = ByteBuffer.wrap(data, 0, len).order(ByteOrder.LITTLE_ENDIAN)
        if (buf.int != MAGIC) return null
        return buf.get()
    }

    /** Extrae el t_us de un PING/PONG. */
    fun pingT(data: ByteArray): Long =
        ByteBuffer.wrap(data, 12, 8).order(ByteOrder.LITTLE_ENDIAN).long

    /** Extrae el session_id de un PING/PONG. */
    fun pingSession(data: ByteArray): Int =
        ByteBuffer.wrap(data, 8, 4).order(ByteOrder.LITTLE_ENDIAN).int
}
