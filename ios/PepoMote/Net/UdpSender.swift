import Darwin
import Foundation

/// Socket UDP caliente (BSD, sin capas): envía INPUT, responde a los PING del
/// receptor y mide RTT con sus propios PING (1 Hz). DSCP EF para que la Wi-Fi
/// (WMM) lo meta en la cola de voz; si el sistema lo ignora, no pasa nada.
final class UdpSender {
    private var fd: Int32 = -1
    private let sessionId: UInt32
    private let onRtt: (Float) -> Void
    private var running = true
    private var listener: Thread?
    private var pinger: Thread?

    /// nil si el host no se puede resolver o el socket no se abre.
    init?(host: String, port: Int, sessionId: UInt32, onRtt: @escaping (Float) -> Void) {
        self.sessionId = sessionId
        self.onRtt = onRtt
        guard let addr = UdpSender.resolve(host, port: port) else { return nil }
        let s = socket(AF_INET, SOCK_DGRAM, 0)
        if s < 0 { return nil }
        fd = s
        var tos: Int32 = 0xB8
        setsockopt(fd, IPPROTO_IP, IP_TOS, &tos, socklen_t(MemoryLayout<Int32>.size))
        var tv = timeval(tv_sec: 0, tv_usec: 500_000)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))
        var sa = addr
        let ok = withUnsafePointer(to: &sa) { p in
            p.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.connect(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size)) }
        }
        if ok != 0 {
            Darwin.close(fd)
            return nil
        }
        let l = Thread { [weak self] in self?.listen() }
        l.name = "pepomote-udp-listener"
        l.qualityOfService = .userInteractive
        l.start()
        listener = l
        let p = Thread { [weak self] in self?.ping() }
        p.name = "pepomote-udp-ping"
        p.start()
        pinger = p
    }

    /// IPv4 numérica o por nombre.
    static func resolve(_ host: String, port: Int) -> sockaddr_in? {
        var addr = sockaddr_in()
        addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = in_port_t(UInt16(clamping: port)).bigEndian
        if host.withCString({ inet_pton(AF_INET, $0, &addr.sin_addr) }) == 1 {
            return addr
        }
        var hints = addrinfo()
        hints.ai_family = AF_INET
        hints.ai_socktype = SOCK_DGRAM
        var res: UnsafeMutablePointer<addrinfo>?
        guard getaddrinfo(host, nil, &hints, &res) == 0, let info = res else { return nil }
        defer { freeaddrinfo(res) }
        guard let sa = info.pointee.ai_addr else { return nil }
        let in4 = sa.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { $0.pointee }
        addr.sin_addr = in4.sin_addr
        return addr
    }

    private func listen() {
        var buf = [UInt8](repeating: 0, count: 256)
        while running {
            let n = buf.withUnsafeMutableBytes { Darwin.recv(fd, $0.baseAddress, 256, 0) }
            if n <= 0 { continue } // timeout o socket cerrado
            let data = Data(buf[0..<n])
            switch PmpCodec.packetType(data) {
            case PmpCodec.typePing:
                // Eco: mismo cuerpo, tipo PONG
                if n >= PmpCodec.pingLen {
                    send(PmpCodec.encodePong(sessionId: PmpCodec.pingSession(data), tUs: PmpCodec.pingT(data)))
                }
            case PmpCodec.typePong:
                if n >= PmpCodec.pingLen, PmpCodec.pingSession(data) == sessionId {
                    let sent = PmpCodec.pingT(data)
                    let now = UdpSender.nowUs()
                    if now >= sent, now - sent <= 5_000_000 {
                        onRtt(Float(now - sent) / 1000)
                    }
                }
            default:
                break
            }
        }
    }

    private func ping() {
        while running {
            send(PmpCodec.encodePing(sessionId: sessionId, tUs: UdpSender.nowUs()))
            Thread.sleep(forTimeInterval: 1)
        }
    }

    func send(_ data: Data) {
        guard running, fd >= 0 else { return }
        _ = data.withUnsafeBytes { Darwin.send(fd, $0.baseAddress, data.count, 0) }
    }

    func close() {
        running = false
        if fd >= 0 {
            shutdown(fd, SHUT_RDWR)
            Darwin.close(fd)
            fd = -1
        }
    }

    /// Reloj monótono en µs (el mismo que llevan los PING).
    static func nowUs() -> UInt64 { DispatchTime.now().uptimeNanoseconds / 1000 }
}
