import Darwin
import Foundation
import Network

struct ReceiverInfo: Equatable {
    let name: String
    let host: String
    let tcpPort: Int
}

/// Descubrimiento de receptores (PROTOCOL.md §1): Bonjour `_pepomote._tcp`
/// (lo que iOS deja hacer sin permisos especiales; el receptor lo anuncia por
/// mDNS) y, de respaldo, el broadcast UDP `PMPDISCOVER1` → `PMPHERE1`, que en
/// iOS 14+ suele fallar sin la entitlement de multicast: se intenta y, si el
/// sistema lo rechaza, no pasa nada.
enum Discovery {
    static func scan(timeoutMs: Int = 1500) async -> [ReceiverInfo] {
        async let bonjour = scanBonjour(timeoutMs: timeoutMs)
        async let broadcast = scanBroadcast(timeoutMs: timeoutMs)
        var found: [ReceiverInfo] = []
        for r in await broadcast where !found.contains(where: { $0.host == r.host }) { found.append(r) }
        for r in await bonjour where !found.contains(where: { $0.host == r.host }) { found.append(r) }
        return found
    }

    // MARK: - Bonjour

    private final class ResultBox {
        var latest = Set<NWBrowser.Result>()
    }

    private static func scanBonjour(timeoutMs: Int) async -> [ReceiverInfo] {
        let results: Set<NWBrowser.Result> = await withCheckedContinuation { cont in
            let box = ResultBox()
            let params = NWParameters()
            params.includePeerToPeer = false
            let browser = NWBrowser(for: .bonjourWithTXTRecord(type: "_pepomote._tcp", domain: nil), using: params)
            let q = DispatchQueue(label: "pepomote.discover")
            browser.browseResultsChangedHandler = { results, _ in box.latest = results }
            browser.stateUpdateHandler = { _ in }
            browser.start(queue: q)
            q.asyncAfter(deadline: .now() + .milliseconds(timeoutMs)) {
                browser.cancel()
                cont.resume(returning: box.latest)
            }
        }
        var out: [ReceiverInfo] = []
        for r in results {
            guard case .service(let name, _, _, _) = r.endpoint else { continue }
            var pcName = name.hasPrefix("PepoMote-") ? String(name.dropFirst("PepoMote-".count)) : name
            if case .bonjour(let txt) = r.metadata, let n = txt.dictionary["name"], !n.isEmpty {
                pcName = n
            }
            if let (host, port) = await resolve(r.endpoint) {
                out.append(ReceiverInfo(name: pcName, host: host, tcpPort: port))
            }
        }
        return out
    }

    /// IP:puerto de un servicio Bonjour: se conecta (IPv4) y lee el extremo real.
    private static func resolve(_ endpoint: NWEndpoint) async -> (String, Int)? {
        await withCheckedContinuation { cont in
            let params = NWParameters.tcp
            if let ip = params.defaultProtocolStack.internetProtocol as? NWProtocolIP.Options {
                ip.version = .v4
            }
            let c = NWConnection(to: endpoint, using: params)
            let q = DispatchQueue(label: "pepomote.resolve")
            let done = ResultFlag()
            func finish(_ v: (String, Int)?) {
                if done.flag { return }
                done.flag = true
                c.cancel()
                cont.resume(returning: v)
            }
            c.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    if case .hostPort(let host, let port)? = c.currentPath?.remoteEndpoint {
                        finish((hostString(host), Int(port.rawValue)))
                    } else {
                        finish(nil)
                    }
                case .failed, .cancelled:
                    finish(nil)
                default:
                    break
                }
            }
            c.start(queue: q)
            q.asyncAfter(deadline: .now() + 2) { finish(nil) }
        }
    }

    private final class ResultFlag {
        var flag = false
    }

    static func hostString(_ h: NWEndpoint.Host) -> String {
        let s: String
        switch h {
        case .ipv4(let a): s = "\(a)"
        case .ipv6(let a): s = "\(a)"
        case .name(let n, _): s = n
        @unknown default: s = "\(h)"
        }
        // Sin el sufijo de interfaz (%en0) de las direcciones de enlace local
        if let i = s.firstIndex(of: "%") { return String(s[..<i]) }
        return s
    }

    // MARK: - Broadcast UDP (respaldo)

    /// `PMPHERE1 {json}` → receptor, o nil.
    static func parseHere(_ text: String, from host: String) -> ReceiverInfo? {
        guard text.hasPrefix(PmpCodec.herePrefix) else { return nil }
        let json = String(text.dropFirst(PmpCodec.herePrefix.count))
        guard let obj = try? JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any],
              (obj["pv"] as? NSNumber)?.intValue == 1
        else { return nil }
        let name = (obj["name"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? host
        let port = (obj["tcp"] as? NSNumber)?.intValue ?? 26761
        return ReceiverInfo(name: name, host: host, tcpPort: port)
    }

    private static func scanBroadcast(timeoutMs: Int) async -> [ReceiverInfo] {
        await withCheckedContinuation { cont in
            DispatchQueue.global(qos: .utility).async {
                cont.resume(returning: broadcastSync(timeoutMs: timeoutMs))
            }
        }
    }

    private static func broadcastSync(timeoutMs: Int) -> [ReceiverInfo] {
        let fd = socket(AF_INET, SOCK_DGRAM, 0)
        if fd < 0 { return [] }
        defer { Darwin.close(fd) }
        var on: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_BROADCAST, &on, socklen_t(MemoryLayout<Int32>.size))
        var tv = timeval(tv_sec: 0, tv_usec: 300_000)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))
        let probe = PmpCodec.discover
        for target in broadcastTargets() {
            var sa = sockaddr_in()
            sa.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
            sa.sin_family = sa_family_t(AF_INET)
            sa.sin_port = in_port_t(26761).bigEndian
            sa.sin_addr = target
            _ = probe.withUnsafeBytes { p in
                withUnsafePointer(to: &sa) { a in
                    a.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                        sendto(fd, p.baseAddress, probe.count, 0, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
                    }
                }
            }
        }
        var found: [ReceiverInfo] = []
        var buf = [UInt8](repeating: 0, count: 1024)
        let deadline = Date().addingTimeInterval(Double(timeoutMs) / 1000)
        while Date() < deadline {
            var from = sockaddr_in()
            var len = socklen_t(MemoryLayout<sockaddr_in>.size)
            let n = withUnsafeMutablePointer(to: &from) { f in
                f.withMemoryRebound(to: sockaddr.self, capacity: 1) { fp in
                    buf.withUnsafeMutableBytes { recvfrom(fd, $0.baseAddress, 1024, 0, fp, &len) }
                }
            }
            if n <= 0 { continue }
            var ip = [CChar](repeating: 0, count: Int(INET_ADDRSTRLEN))
            var a = from.sin_addr
            guard inet_ntop(AF_INET, &a, &ip, socklen_t(INET_ADDRSTRLEN)) != nil else { continue }
            let host = String(cString: ip)
            let text = String(decoding: buf[0..<n], as: UTF8.self)
            if let r = parseHere(text, from: host), !found.contains(where: { $0.host == host }) {
                found.append(r)
            }
        }
        return found
    }

    /// 255.255.255.255 y el broadcast dirigido de cada interfaz activa.
    private static func broadcastTargets() -> [in_addr] {
        var out: [in_addr] = []
        var limited = in_addr()
        limited.s_addr = INADDR_BROADCAST
        out.append(limited)
        var ifap: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&ifap) == 0, let first = ifap else { return out }
        defer { freeifaddrs(ifap) }
        var p: UnsafeMutablePointer<ifaddrs>? = first
        while let ifa = p {
            let flags = Int32(ifa.pointee.ifa_flags)
            if flags & IFF_UP != 0, flags & IFF_LOOPBACK == 0, flags & IFF_BROADCAST != 0,
               let dst = ifa.pointee.ifa_dstaddr, dst.pointee.sa_family == sa_family_t(AF_INET) {
                let bcast = dst.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { $0.pointee.sin_addr }
                if !out.contains(where: { $0.s_addr == bcast.s_addr }) { out.append(bcast) }
            }
            p = ifa.pointee.ifa_next
        }
        return out
    }
}
