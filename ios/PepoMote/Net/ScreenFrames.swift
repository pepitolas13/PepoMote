import Foundation

/// Tramas del canal de pantalla del GamePad (PROTOCOL.md §4.4): tras la línea
/// de respuesta del receptor todo son tramas binarias
/// `"PMPS"` · tipo u8 · longitud u32 LE · carga. Acumula lo que llega por el
/// socket (trozos de cualquier tamaño) y devuelve tramas completas de una en
/// una; si el magic no cuadra (o la longitud es absurda) descarta bytes hasta
/// el siguiente candidato y sigue (resync). Un único buffer que crece hasta la
/// trama más grande vista y se reutiliza. Testeado en ScreenFramesTests.
final class ScreenFrames {
    /// tipo 1: imagen JPEG completa (baseline).
    static let typeImage = 1
    /// tipo 2: estado en texto UTF-8, para enseñar mientras no hay imagen.
    static let typeStatus = 2
    /// tipo 3: keepalive sin carga (cada 2 s si no hay otra cosa que mandar).
    static let typeKeepalive = 3
    /// magic (4) + tipo (1) + longitud (4).
    static let headerLen = 9
    /// Confirmación del móvil: un byte tras decodificar y pintar cada imagen.
    static let ack: UInt8 = 0x01
    /// Tope de carga: una imagen 854×480 en JPEG anda por 30-100 KB; mucho más es basura.
    static let maxPayload = 4 << 20

    private static let magic: [UInt8] = Array("PMPS".utf8)

    /// Trama completa (la carga es una copia).
    struct Frame {
        let type: Int
        let payload: Data
    }

    private var buf: [UInt8]
    /// Bytes válidos en `buf`.
    private var count = 0
    /// Inicio de lo aún no consumido.
    private var pos = 0
    private var synced = true
    private let maxPayload: Int

    /// Veces que se perdió la sincronía (bytes inesperados donde tocaba un magic).
    private(set) var resyncs = 0
    /// Bytes descartados en total por esas resincronizaciones.
    private(set) var dropped = 0

    init(maxPayload: Int = ScreenFrames.maxPayload, initial: Int = 256 << 10) {
        self.maxPayload = maxPayload
        buf = [UInt8](repeating: 0, count: initial)
    }

    /// Bytes acumulados aún sin consumir.
    var pending: Int { count - pos }

    /// Añade bytes recibidos.
    func push(_ src: [UInt8], _ off: Int = 0, _ len: Int? = nil) {
        let n = len ?? (src.count - off)
        if n <= 0 { return }
        ensureRoom(n)
        buf.withUnsafeMutableBufferPointer { dst in
            src.withUnsafeBufferPointer { s in
                (dst.baseAddress! + count).update(from: s.baseAddress! + off, count: n)
            }
        }
        count += n
    }

    func push(_ data: Data) {
        push([UInt8](data))
    }

    /// Línea de texto completa (hasta `\n`, sin él ni un `\r` final), o nil si
    /// aún no ha llegado entera. Lo que venga detrás queda para `next`.
    func takeLine() -> String? {
        var i = pos
        while i < count {
            if buf[i] == 0x0A {
                var end = i
                if end > pos, buf[end - 1] == 0x0D { end -= 1 }
                let line = String(decoding: buf[pos..<end], as: UTF8.self)
                pos = i + 1
                return line
            }
            i += 1
        }
        return nil
    }

    /// Siguiente trama completa, o nil si faltan bytes.
    func next() -> Frame? {
        while true {
            if count - pos < ScreenFrames.headerLen { return nil }
            if !magicAt(pos) {
                resync()
                continue
            }
            let type = Int(buf[pos + 4])
            let length = UInt32(buf[pos + 5]) | (UInt32(buf[pos + 6]) << 8) | (UInt32(buf[pos + 7]) << 16) | (UInt32(buf[pos + 8]) << 24)
            if length > UInt32(maxPayload) {
                // Un "PMPS" casual dentro de basura: seguir buscando
                resync()
                continue
            }
            let len = Int(length)
            if count - pos - ScreenFrames.headerLen < len { return nil }
            let start = pos + ScreenFrames.headerLen
            let payload = Data(buf[start..<(start + len)])
            pos += ScreenFrames.headerLen + len
            synced = true
            return Frame(type: type, payload: payload)
        }
    }

    /// Vacía lo acumulado (conexión nueva).
    func reset() {
        count = 0
        pos = 0
        synced = true
    }

    private func magicAt(_ i: Int) -> Bool {
        buf[i] == ScreenFrames.magic[0] && buf[i + 1] == ScreenFrames.magic[1] &&
            buf[i + 2] == ScreenFrames.magic[2] && buf[i + 3] == ScreenFrames.magic[3]
    }

    /// Salta hasta la siguiente 'P' después de `pos` (o hasta el final si no hay).
    private func resync() {
        if synced {
            synced = false
            resyncs += 1
        }
        var i = pos + 1
        while i < count, buf[i] != ScreenFrames.magic[0] { i += 1 }
        dropped += i - pos
        pos = i
    }

    /// Sitio para `n` bytes más al final: compacta lo pendiente y, si aún no cabe, crece.
    private func ensureRoom(_ n: Int) {
        if buf.count - count >= n { return }
        if pos > 0 {
            let remaining = count - pos
            if remaining > 0 {
                buf.withUnsafeMutableBufferPointer { p in
                    p.baseAddress!.update(from: p.baseAddress! + pos, count: remaining)
                }
            }
            count = remaining
            pos = 0
            if buf.count - count >= n { return }
        }
        var size = buf.count
        while size - count < n { size *= 2 }
        buf.append(contentsOf: [UInt8](repeating: 0, count: size - buf.count))
    }
}
