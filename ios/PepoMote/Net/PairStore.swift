import Foundation

struct Pairing: Equatable {
    var host: String
    var port: Int
    var token: String
    var pcName: String
}

/// Varios PCs guardados: funciones puras sobre la lista, testeadas en
/// PairListTests. Un token es un PC; su nombre y su IP pueden cambiar
/// (renombrado, DHCP) y se actualizan sin perder el token.
enum PairList {
    private static let allowed: CharacterSet = {
        var s = CharacterSet.alphanumerics
        s.insert(charactersIn: "-_.")
        return s
    }()

    private static func enc(_ s: String) -> String {
        s.addingPercentEncoding(withAllowedCharacters: allowed) ?? s
    }

    private static func dec(_ s: String) -> String {
        s.removingPercentEncoding ?? s
    }

    /// Una línea por PC: `t=…&host=…&port=…&name=…`, valores URL-encoded.
    static func encode(_ list: [Pairing]) -> String {
        list.map { p in "t=\(enc(p.token))&host=\(enc(p.host))&port=\(p.port)&name=\(enc(p.pcName))" }
            .joined(separator: "\n")
    }

    static func decode(_ text: String?) -> [Pairing] {
        guard let text else { return [] }
        return text.split(separator: "\n", omittingEmptySubsequences: true).compactMap { line -> Pairing? in
            if line.trimmingCharacters(in: .whitespaces).isEmpty { return nil }
            var f: [String: String] = [:]
            for kv in line.split(separator: "&") {
                guard let i = kv.firstIndex(of: "="), i > kv.startIndex else { continue }
                f[String(kv[..<i])] = dec(String(kv[kv.index(after: i)...]))
            }
            guard let token = f["t"], let host = f["host"] else { return nil }
            return Pairing(host: host, port: f["port"].flatMap(Int.init) ?? 26761, token: token, pcName: f["name"] ?? "PC")
        }
    }

    /// Añade o actualiza (por token) conservando el orden.
    static func upsert(_ list: [Pairing], _ p: Pairing) -> [Pairing] {
        if list.contains(where: { $0.token == p.token }) {
            return list.map { $0.token == p.token ? p : $0 }
        }
        return list + [p]
    }

    static func forget(_ list: [Pairing], _ token: String) -> [Pairing] {
        list.filter { $0.token != token }
    }

    /// PC actual tras olvidar `forgotten`: el mismo si sigue en la lista; si no, el primero que quede.
    static func nextCurrent(_ list: [Pairing], _ current: String?, _ forgotten: String) -> String? {
        if let current, current != forgotten, list.contains(where: { $0.token == current }) { return current }
        return list.first?.token
    }

    /// Un PC guardado se ve en la red (por nombre o por IP).
    static func isOnline(_ p: Pairing, _ receivers: [ReceiverInfo]) -> Bool {
        receivers.contains { $0.name == p.pcName || $0.host == p.host }
    }

    /// Receptores de la red que no son ninguno de los guardados.
    static func unknown(_ receivers: [ReceiverInfo], _ list: [Pairing]) -> [ReceiverInfo] {
        receivers.filter { r in !list.contains { $0.pcName == r.name || $0.host == r.host } }
    }

    /// IP y puerto de los PCs guardados que la red enseña en otro sitio, por nombre.
    static func refreshFrom(_ list: [Pairing], _ receivers: [ReceiverInfo]) -> [Pairing] {
        list.map { relocateTarget($0, list, receivers) }
    }

    /// `p` con la IP/puerto donde la red dice que está ahora su nombre (o tal
    /// cual). Un sitio ya ocupado por OTRO PC guardado no se adopta.
    static func relocateTarget(_ p: Pairing, _ list: [Pairing], _ receivers: [ReceiverInfo]) -> Pairing {
        guard let found = receivers.first(where: { $0.name == p.pcName && ($0.host != p.host || $0.tcpPort != p.port) }) else {
            return p
        }
        let claimed = list.contains { $0.token != p.token && $0.host == found.host && $0.port == found.tcpPort }
        if claimed { return p }
        var moved = p
        moved.host = found.host
        moved.port = found.tcpPort
        return moved
    }
}

/// Los PCs emparejados (varios) y cuál es el actual, en UserDefaults.
enum PairStore {
    private static let keyList = "pairing.list"
    private static let keyCurrent = "pairing.current"
    private static var d: UserDefaults { .standard }

    /// Todos los PCs guardados, en el orden en que se emparejaron.
    static func all() -> [Pairing] {
        PairList.decode(d.string(forKey: keyList))
    }

    /// El PC actual (con el que se conecta); nil sin ninguno.
    static func current() -> Pairing? {
        let list = all()
        let token = d.string(forKey: keyCurrent)
        return list.first { $0.token == token } ?? list.first
    }

    /// Guarda (por token) y lo deja como actual.
    static func save(_ p: Pairing) {
        write(PairList.upsert(all(), p), p.token)
    }

    /// Otro de los guardados pasa a ser el actual.
    static func select(_ token: String) {
        let list = all()
        if list.contains(where: { $0.token == token }) { write(list, token) }
    }

    static func forget(_ token: String) {
        let current = d.string(forKey: keyCurrent)
        let list = PairList.forget(all(), token)
        write(list, PairList.nextCurrent(list, current, token))
    }

    private static func write(_ list: [Pairing], _ current: String?) {
        d.set(PairList.encode(list), forKey: keyList)
        if let current { d.set(current, forKey: keyCurrent) } else { d.removeObject(forKey: keyCurrent) }
    }

    /// Parsea pepomote://pair?v=1&host=..&port=..&t=..&name=..
    static func parsePairUrl(_ url: String) -> Pairing? {
        guard let c = URLComponents(string: url.trimmingCharacters(in: .whitespacesAndNewlines)),
              c.scheme?.lowercased() == "pepomote", c.host?.lowercased() == "pair"
        else { return nil }
        var q: [String: String] = [:]
        for item in c.queryItems ?? [] where q[item.name] == nil { q[item.name] = item.value ?? "" }
        guard q["v"] == "1", let host = q["host"], !host.isEmpty, let token = q["t"], !token.isEmpty else { return nil }
        let port = q["port"].flatMap(Int.init) ?? 26761
        let name = q["name"].flatMap { $0.isEmpty ? nil : $0 } ?? "PC"
        return Pairing(host: host, port: port, token: token, pcName: name)
    }
}
