import Foundation

/// Codec del protocolo PMP v1. Fuente de verdad: protocol/PROTOCOL.md.
/// Paridad garantizada por los vectores dorados (PmpCodecTests ↔ tests de
/// Rust y de Kotlin). Todo little-endian.
enum PmpCodec {
    static let magic: UInt32 = 0x3150_4D50 // "PMP1" LE
    static let typeInput: UInt8 = 0x01
    static let typePing: UInt8 = 0x02
    static let typePong: UInt8 = 0x03
    static let inputLen = 72
    /// INPUT con el bloque de extensión Wii U (flagExt): bytes 72-79.
    static let inputExtLen = 80
    static let pingLen = 20

    static let discover = Data("PMPDISCOVER1".utf8)
    static let herePrefix = "PMPHERE1 "

    /// flags bit0: el quaternion es válido.
    static let flagQuatValid: UInt8 = 1
    /// flags bit1: bytes 6-7 llevan el stick (Nunchuk o izquierdo del GamePad).
    static let flagStickValid: UInt8 = 2
    /// flags bit2: el paquete mide 80 bytes y trae el bloque Wii U.
    static let flagExt: UInt8 = 4
    /// flags bit3: hay un dedo en la pantalla táctil del GamePad.
    static let flagTouch: UInt8 = 8

    /// INPUT de 72 bytes, u 80 si `flags` lleva `flagExt`.
    static func encodeInput(
        sessionId: UInt32,
        seq: UInt32,
        tSensorUs: UInt64,
        quat: [Float], // w, x, y, z
        gyro: [Float], // rad/s
        accel: [Float], // m/s²
        buttons: UInt32,
        recenterCount: Int,
        batteryPct: Int,
        touchScrollDy: Int,
        flags: UInt8 = 0,
        stickX: Int = 0,
        stickY: Int = 0,
        stick2X: Int = 0,
        stick2Y: Int = 0,
        touchX: Int = 0,
        touchY: Int = 0
    ) -> Data {
        let ext = flags & flagExt != 0
        var d = Data(capacity: ext ? inputExtLen : inputLen)
        d.appendLE(magic)
        d.append(typeInput)
        d.append(flags)
        d.appendI8(stickX)
        d.appendI8(stickY)
        d.appendLE(sessionId)
        d.appendLE(seq)
        d.appendLE(tSensorUs)
        for i in 0..<4 { d.appendF32(quat[i]) }
        for i in 0..<3 { d.appendF32(gyro[i]) }
        for i in 0..<3 { d.appendF32(accel[i]) }
        d.appendLE(buttons)
        d.append(UInt8(recenterCount & 0xFF))
        d.append(UInt8(min(max(batteryPct, 0), 100)))
        d.appendLE(Int16(clamping: touchScrollDy))
        if ext {
            d.appendI8(stick2X)
            d.appendI8(stick2Y)
            d.appendLE(UInt16(min(max(touchX, 0), 0xFFFF)))
            d.appendLE(UInt16(min(max(touchY, 0), 0xFFFF)))
            d.appendLE(UInt16(0)) // 78-79 reservados
        }
        return d
    }

    static func encodePing(sessionId: UInt32, tUs: UInt64) -> Data { pingLike(typePing, sessionId, tUs) }
    static func encodePong(sessionId: UInt32, tUs: UInt64) -> Data { pingLike(typePong, sessionId, tUs) }

    private static func pingLike(_ type: UInt8, _ sessionId: UInt32, _ tUs: UInt64) -> Data {
        var d = Data(capacity: pingLen)
        d.appendLE(magic)
        d.append(type)
        d.append(0)
        d.appendLE(UInt16(0))
        d.appendLE(sessionId)
        d.appendLE(tUs)
        return d
    }

    /// Tipo del paquete, o nil si no es PMP.
    static func packetType(_ data: Data) -> UInt8? {
        guard data.count >= 12, readU32(data, 0) == magic else { return nil }
        return data[data.startIndex + 4]
    }

    /// `t_us` de un PING/PONG.
    static func pingT(_ data: Data) -> UInt64 { readU64(data, 12) }

    /// `session_id` de un PING/PONG.
    static func pingSession(_ data: Data) -> UInt32 { readU32(data, 8) }

    static func readU32(_ d: Data, _ off: Int) -> UInt32 {
        let s = d.startIndex + off
        return UInt32(d[s]) | (UInt32(d[s + 1]) << 8) | (UInt32(d[s + 2]) << 16) | (UInt32(d[s + 3]) << 24)
    }

    static func readU64(_ d: Data, _ off: Int) -> UInt64 {
        var v: UInt64 = 0
        let s = d.startIndex + off
        for i in 0..<8 { v |= UInt64(d[s + i]) << (8 * UInt64(i)) }
        return v
    }
}

extension Data {
    mutating func appendLE<T: FixedWidthInteger>(_ value: T) {
        var v = value.littleEndian
        Swift.withUnsafeBytes(of: &v) { append(contentsOf: $0) }
    }

    mutating func appendF32(_ value: Float) {
        appendLE(value.bitPattern)
    }

    /// i8 recortado a −127..127 (como los sticks del protocolo).
    mutating func appendI8(_ value: Int) {
        append(UInt8(bitPattern: Int8(Swift.min(Swift.max(value, -127), 127))))
    }

    /// Hex en minúsculas (tests y trazas).
    var hexString: String {
        map { String(format: "%02x", $0) }.joined()
    }
}
