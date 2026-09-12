import Foundation
import Network
import UIKit

/// Canal de pantalla del GamePad (PROTOCOL.md §4.4): conexión TCP propia al
/// puerto PMP del receptor, línea `screen` con la sesión y el tamaño máximo
/// que cabe en la zona táctil, y de vuelta tramas `ScreenFrames` con la
/// pantalla del GamePad de Cemu en JPEG. Todo en una cola propia: ahí se
/// decodifica y se publica en `image` (cola principal); la UI avisa con
/// `shown` al pintarla y entonces sale el byte de confirmación (una imagen en
/// vuelo: la latencia no se acumula). Si nadie la pinta en `shownFallbackMs`,
/// se confirma igualmente.
///
/// - Sin respuesta al `screen` en 3 s (o cierre sin contestar, o `err`):
///   «sin pantalla» (receptor sin canal de pantalla); reintento cada 5 s.
/// - Sin datos en 6 s tras el ok: conexión muerta; se cierra y se reconecta
///   al segundo, y si tampoco, cada 5 s.
/// - `close` es definitivo: nada vuelve a abrirse ni a publicarse.
final class ScreenClient: ObservableObject {
    /// Resolución nativa de la pantalla del GamePad: nunca se pide más.
    static let nativeWidth = 854
    static let nativeHeight = 480
    static let quality = 70

    /// Estados centinela: la pantalla los traduce (`GamePadScreen`).
    static let statusUnavailable = "\u{0}unavailable"
    static let statusReconnecting = "\u{0}reconnecting"
    /// Prefijo + detalle que manda el PC («sesión desconocida»…).
    static let statusDetail = "\u{0}detail:"

    /// Imagen decodificada lista para pintar; `seq` la identifica.
    struct Image {
        let seq: Int
        let image: UIImage
    }

    @Published private(set) var image: Image?
    /// Última línea de estado (del receptor, o propia: sin pantalla / reconectando).
    @Published private(set) var status: String?
    /// Imágenes por segundo (media de 1 s); 0 sin imagen.
    @Published private(set) var fps: Int = 0

    private let host: String
    private let port: Int
    private let sessionId: UInt32
    private let maxWidth: Int
    private let maxHeight: Int
    private let quality: Int
    private let replyTimeoutMs: Int
    private let dataTimeoutMs: Int
    private let shownFallbackMs: Int
    private let retryAfterDropMs: Int
    private let retryUnavailableMs: Int
    private let decode: (Data) -> UIImage?

    private let q = DispatchQueue(label: "pepomote.screen")
    private var conn: NWConnection?
    private var frames = ScreenFrames()
    private var running = true
    private var started = false
    /// Generación de la conexión: una confirmación de la anterior no se escribe en la nueva.
    private var gen = 0
    private var gotOk = false
    private var seq = 0
    private var pendingAck = 0
    private var replyTimer: DispatchWorkItem?
    private var dataTimer: DispatchWorkItem?
    private var retryTimer: DispatchWorkItem?
    private var fpsWindowStartNs: UInt64 = 0
    private var fpsCount = 0

    init(
        host: String,
        port: Int,
        sessionId: UInt32,
        maxWidth: Int,
        maxHeight: Int,
        quality: Int = ScreenClient.quality,
        replyTimeoutMs: Int = 3000,
        dataTimeoutMs: Int = 6000,
        shownFallbackMs: Int = 200,
        retryAfterDropMs: Int = 1000,
        retryUnavailableMs: Int = 5000,
        decode: @escaping (Data) -> UIImage? = { UIImage(data: $0) }
    ) {
        self.host = host
        self.port = port
        self.sessionId = sessionId
        self.maxWidth = maxWidth
        self.maxHeight = maxHeight
        self.quality = quality
        self.replyTimeoutMs = replyTimeoutMs
        self.dataTimeoutMs = dataTimeoutMs
        self.shownFallbackMs = shownFallbackMs
        self.retryAfterDropMs = retryAfterDropMs
        self.retryUnavailableMs = retryUnavailableMs
        self.decode = decode
    }

    // MARK: - Línea de respuesta (puro, testeable)

    /// Línea `screen` (PROTOCOL.md §4.4); `session_id` va como u32.
    static func helloLine(sessionId: UInt32, w: Int, h: Int, q: Int) -> String {
        "{\"m\":\"screen\",\"session_id\":\(sessionId),\"w\":\(w),\"h\":\(h),\"q\":\(q)}"
    }

    /// `{"m":"screen","ok":true}`: el receptor acepta.
    static func replyOk(_ line: String) -> Bool {
        jsonString(line, "m") == "screen" && line.range(of: "\"ok\"\\s*:\\s*true", options: .regularExpression) != nil
    }

    /// Valor de una clave string de la línea JSON de respuesta (solo tiene
    /// `m`, `ok`, `code` y `msg`), con los escapes deshechos.
    static func jsonString(_ line: String, _ key: String) -> String? {
        let pattern = "\"" + NSRegularExpression.escapedPattern(for: key) + "\"\\s*:\\s*\"((?:[^\"\\\\]|\\\\.)*)\""
        guard let re = try? NSRegularExpression(pattern: pattern),
              let m = re.firstMatch(in: line, range: NSRange(line.startIndex..., in: line)),
              let r = Range(m.range(at: 1), in: line)
        else { return nil }
        let raw = String(line[r])
        if !raw.contains("\\") { return raw }
        var out = ""
        let chars = Array(raw)
        var i = 0
        while i < chars.count {
            let c = chars[i]
            if c != "\\" || i + 1 >= chars.count {
                out.append(c)
                i += 1
                continue
            }
            let e = chars[i + 1]
            switch e {
            case "n": out.append("\n")
            case "t": out.append("\t")
            case "r": out.append("\r")
            case "u":
                if i + 5 < chars.count, let code = UInt32(String(chars[(i + 2)...(i + 5)]), radix: 16),
                   let scalar = Unicode.Scalar(code) {
                    out.unicodeScalars.append(scalar)
                    i += 4
                }
            default: out.append(e) // \" \\ \/
            }
            i += 2
        }
        return out
    }

    // MARK: - Ciclo de vida

    func start() {
        q.async {
            if self.started || !self.running { return }
            self.started = true
            self.connect()
        }
    }

    /// La UI acaba de pintar la imagen `seq`: ahora sí, confirmación al receptor.
    func shown(_ seq: Int) {
        q.async {
            if self.pendingAck == seq, seq != 0 {
                self.pendingAck = 0
                self.writeAck(self.gen)
            }
        }
    }

    func close() {
        q.async {
            self.running = false
            self.retryTimer?.cancel()
            self.replyTimer?.cancel()
            self.dataTimer?.cancel()
            self.conn?.cancel()
            self.conn = nil
            DispatchQueue.main.async {
                self.image = nil
                self.status = nil
                self.fps = 0
            }
        }
    }

    // MARK: - Una conexión (todo en `q`)

    private func connect() {
        guard running else { return }
        gen += 1
        let myGen = gen
        gotOk = false
        pendingAck = 0
        frames.reset()
        let tcp = NWProtocolTCP.Options()
        tcp.noDelay = true
        tcp.connectionTimeout = 4
        let params = NWParameters(tls: nil, tcp: tcp)
        let nwPort = NWEndpoint.Port(rawValue: UInt16(clamping: port)) ?? NWEndpoint.Port(rawValue: 26761)!
        let c = NWConnection(host: NWEndpoint.Host(host), port: nwPort, using: params)
        conn = c
        c.stateUpdateHandler = { [weak self] state in
            guard let self, self.gen == myGen else { return }
            switch state {
            case .ready:
                let line = ScreenClient.helloLine(sessionId: self.sessionId, w: self.maxWidth, h: self.maxHeight, q: self.quality) + "\n"
                c.send(content: Data(line.utf8), completion: .contentProcessed { _ in })
                self.armReplyTimer(myGen)
                self.receive(c, myGen)
            case .failed, .cancelled:
                self.ended(myGen)
            default:
                break
            }
        }
        // Sin respuesta al connect: también «sin pantalla»
        armReplyTimer(myGen)
        c.start(queue: q)
    }

    private func receive(_ c: NWConnection, _ myGen: Int) {
        c.receive(minimumIncompleteLength: 1, maximumLength: 256 << 10) { [weak self] data, _, isComplete, error in
            guard let self, self.gen == myGen, self.running else { return }
            if let data, !data.isEmpty {
                self.frames.push(data)
                self.consume(myGen)
            }
            if error != nil || isComplete {
                self.ended(myGen)
                return
            }
            self.receive(c, myGen)
        }
    }

    private func consume(_ myGen: Int) {
        if !gotOk {
            // Respuesta: una línea JSON. Un receptor sin pantalla cierra o calla y un `err` trae el motivo.
            guard let line = frames.takeLine() else {
                if frames.pending > 4096 { unavailable(nil); dropConnection(myGen, retryMs: retryUnavailableMs) }
                return
            }
            if !ScreenClient.replyOk(line) {
                unavailable(ScreenClient.jsonString(line, "msg"))
                dropConnection(myGen, retryMs: retryUnavailableMs)
                return
            }
            gotOk = true
            replyTimer?.cancel()
            DispatchQueue.main.async { self.status = nil }
        }
        armDataTimer(myGen)
        while let f = frames.next() {
            switch f.type {
            case ScreenFrames.typeImage: onImage(f.payload, myGen)
            case ScreenFrames.typeStatus: onStatus(String(decoding: f.payload, as: UTF8.self))
            default: break // keepalive (ya contó como dato al llegar) o tipo futuro
            }
        }
    }

    private func onImage(_ data: Data, _ myGen: Int) {
        guard running else { return }
        guard let img = decode(data) else {
            writeAck(myGen) // nada que enseñar: se confirma igual para que siga el flujo
            return
        }
        seq += 1
        let n = seq
        pendingAck = n
        countImage()
        DispatchQueue.main.async { self.image = Image(seq: n, image: img) }
        // Si la UI no llega a pintarla, que no se atasque el flujo
        q.asyncAfter(deadline: .now() + .milliseconds(shownFallbackMs)) { [weak self] in
            guard let self else { return }
            if self.pendingAck == n {
                self.pendingAck = 0
                self.writeAck(myGen)
            }
        }
    }

    /// Estado del receptor: solo lo manda cuando no tiene imagen que dar.
    private func onStatus(_ text: String) {
        resetFps()
        DispatchQueue.main.async {
            self.image = nil
            self.status = text
        }
    }

    private func unavailable(_ msg: String?) {
        guard running else { return }
        resetFps()
        let s = (msg ?? "").isEmpty ? ScreenClient.statusUnavailable : ScreenClient.statusDetail + (msg ?? "")
        DispatchQueue.main.async {
            self.image = nil
            self.status = s
        }
    }

    private func dropped() {
        guard running else { return }
        resetFps()
        DispatchQueue.main.async {
            self.image = nil
            self.status = ScreenClient.statusReconnecting
        }
    }

    /// La conexión ha terminado (fallo, cierre del PC, timeout): según la fase,
    /// «sin pantalla» o «reconectando», y reintento.
    private func ended(_ myGen: Int) {
        guard gen == myGen else { return }
        if gotOk {
            dropped()
            dropConnection(myGen, retryMs: retryAfterDropMs)
        } else {
            unavailable(nil)
            dropConnection(myGen, retryMs: retryUnavailableMs)
        }
    }

    private func dropConnection(_ myGen: Int, retryMs: Int) {
        guard gen == myGen else { return }
        gen += 1 // invalida los callbacks de esta conexión
        replyTimer?.cancel()
        dataTimer?.cancel()
        conn?.cancel()
        conn = nil
        guard running else { return }
        let w = DispatchWorkItem { [weak self] in self?.connect() }
        retryTimer = w
        q.asyncAfter(deadline: .now() + .milliseconds(retryMs), execute: w)
    }

    private func armReplyTimer(_ myGen: Int) {
        replyTimer?.cancel()
        let w = DispatchWorkItem { [weak self] in
            guard let self, self.gen == myGen, !self.gotOk else { return }
            self.unavailable(nil)
            self.dropConnection(myGen, retryMs: self.retryUnavailableMs)
        }
        replyTimer = w
        q.asyncAfter(deadline: .now() + .milliseconds(replyTimeoutMs), execute: w)
    }

    private func armDataTimer(_ myGen: Int) {
        dataTimer?.cancel()
        let w = DispatchWorkItem { [weak self] in
            guard let self, self.gen == myGen else { return }
            self.ended(myGen)
        }
        dataTimer = w
        q.asyncAfter(deadline: .now() + .milliseconds(dataTimeoutMs), execute: w)
    }

    /// Solo en `q`.
    private func writeAck(_ myGen: Int) {
        guard myGen == gen, let c = conn else { return }
        c.send(content: Data([ScreenFrames.ack]), completion: .contentProcessed { _ in })
    }

    private func countImage() {
        let now = DispatchTime.now().uptimeNanoseconds
        if fpsWindowStartNs == 0 { fpsWindowStartNs = now }
        fpsCount += 1
        let elapsed = now - fpsWindowStartNs
        if elapsed >= 1_000_000_000 {
            let f = Int((Double(fpsCount) * 1e9 / Double(elapsed)).rounded())
            DispatchQueue.main.async { self.fps = f }
            fpsWindowStartNs = now
            fpsCount = 0
        }
    }

    private func resetFps() {
        fpsWindowStartNs = 0
        fpsCount = 0
        DispatchQueue.main.async { self.fps = 0 }
    }
}
