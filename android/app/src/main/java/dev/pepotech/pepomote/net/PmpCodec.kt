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

    /** RUMBLE (receptor -> movil, PROTOCOL.md 4.5): la vibracion que pide el juego. */
    const val TYPE_RUMBLE: Byte = 0x04
    const val INPUT_LEN = 72

    /** INPUT con el bloque de extensión Wii U / Switch (FLAG_EXT): bytes 72-79. */
    const val INPUT_EXT_LEN = 80
    const val PING_LEN = 20
    const val RUMBLE_LEN = 20

    val DISCOVER = "PMPDISCOVER1".toByteArray(Charsets.US_ASCII)
    const val HERE_PREFIX = "PMPHERE1 "

    /** flags bit0: el quaternion es válido (hay GAME_ROTATION_VECTOR). */
    const val FLAG_QUAT_VALID = 1

    /** flags bit1: bytes 6-7 llevan el stick (Nunchuk o izquierdo del GamePad). */
    const val FLAG_STICK_VALID = 2

    /**
     * flags bit2: el paquete mide 80 bytes y los bytes 72-79 llevan el bloque
     * Wii U / Switch (stick derecho + táctil, solo Wii U). Solo se emite con
     * `cemu` o `switch` y su capacidad confirmados por el receptor.
     */
    const val FLAG_EXT = 4

    /** flags bit3: hay un dedo en la pantalla táctil del GamePad (touch_x/y válidos). */
    const val FLAG_TOUCH = 8

    /**
     * flags bit4: apuntado por INCLINACIÓN. El móvil no tiene giroscopio real
     * (o el usuario eligió el acelerómetro): el receptor saca el cursor del
     * acelerómetro (horizontal = roll, vertical = pitch) y no del quaternion
     * ni del gyro, que se siguen enviando tal cual. Solo se emite si el `ok`
     * del receptor trae `"tilt":true` (el servidor Android descarta flags
     * desconocidos).
     */
    const val FLAG_TILT = 16

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
        touchY: Int = 0, // táctil 0..65535, 0 = arriba
        switchPad: Boolean = false
    ): ByteArray {
        val ext = flags and FLAG_EXT != 0
        val buf = ByteBuffer.allocate(if (ext) INPUT_EXT_LEN else INPUT_LEN)
            .order(ByteOrder.LITTLE_ENDIAN)
        buf.putInt(MAGIC)
        buf.put(TYPE_INPUT)
        buf.put(((if (switchPad) flags and FLAG_TOUCH.inv() else flags) and 0xFF).toByte())
        buf.put(stickX.coerceIn(-127, 127).toByte())
        buf.put(stickY.coerceIn(-127, 127).toByte())
        buf.putInt(sessionId)
        buf.putInt(seq)
        buf.putLong(tSensorUs)
        buf.putFloat(quat[0]); buf.putFloat(quat[1]); buf.putFloat(quat[2]); buf.putFloat(quat[3])
        buf.putFloat(gyro[0]); buf.putFloat(gyro[1]); buf.putFloat(gyro[2])
        buf.putFloat(accel[0]); buf.putFloat(accel[1]); buf.putFloat(accel[2])
        buf.putInt(if (switchPad) buttons and (1 shl 27).inv() else buttons)
        buf.put((recenterCount and 0xFF).toByte())
        buf.put((batteryPct.coerceIn(0, 100)).toByte())
        buf.putShort(touchScrollDy.coerceIn(-32768, 32767).toShort())
        if (ext) {
            buf.put(stick2X.coerceIn(-127, 127).toByte())
            buf.put(stick2Y.coerceIn(-127, 127).toByte())
            buf.putShort((if (switchPad) 0 else touchX.coerceIn(0, 0xFFFF)).toShort()) // u16 LE
            buf.putShort((if (switchPad) 0 else touchY.coerceIn(0, 0xFFFF)).toShort())
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

    /**
     * Un RUMBLE decodificado: es ESTADO (el receptor lo repite cada 100 ms
     * mientras vibra), no un evento; `ttlMs` = 0 en el de parada.
     */
    data class Rumble(
        val sessionId: Int,
        val seq: Int,
        /** Motor grande, 0..255. */
        val strong: Int,
        /** Motor pequeno, 0..255. */
        val weak: Int,
        /** Sin otro RUMBLE en este tiempo el movil para solo. */
        val ttlMs: Int
    )

    /**
     * RUMBLE de 20 bytes; null con otra longitud, otro magic u otro tipo (un
     * PING mide lo mismo). Quien filtra la sesion es el que lo recibe.
     */
    fun decodeRumble(data: ByteArray, len: Int): Rumble? {
        if (len != RUMBLE_LEN || packetType(data, len) != TYPE_RUMBLE) return null
        val buf = ByteBuffer.wrap(data, 0, len).order(ByteOrder.LITTLE_ENDIAN)
        return Rumble(
            sessionId = buf.getInt(8),
            seq = buf.getInt(12),
            strong = data[16].toInt() and 0xFF,
            weak = data[17].toInt() and 0xFF,
            ttlMs = ((data[19].toInt() and 0xFF) shl 8) or (data[18].toInt() and 0xFF)
        )
    }
}
