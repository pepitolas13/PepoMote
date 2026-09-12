import Foundation
import Network

/// Canal de control TCP (PROTOCOL.md §3): hello/ok/err, ping 1 Hz, mode, pad,
/// text, notice. Los callbacks llegan en la cola principal.
final class ControlClient {
    /// Lo que confirma el receptor en el `ok`.
    struct Ok: Equatable {
        let sessionId: UInt32
        let udpPort: Int
        let mode: String
        let slot: Int
        /// El rol que confirma el receptor.
        let role: String
        /// 1..4 (0 si el ok no lo trae).
        let player: Int
        /// `modes` contiene "cemu": el receptor sabe de Wii U.
        let supportsCemu: Bool
        /// Mando efectivo en modo Wii U: gamepad / pro / wiimote (ausente = gamepad).
        let pad: String
        /// Nombre del PC (`ok.name`); vacío si el receptor no lo manda.
        let name: String
    }

    struct Callbacks {
        var onOk: (Ok) -> Void
        /// `code` = "io" si es la red; si no, el rechazo del receptor.
        var onError: (String, String) -> Void
        /// Eco o difusión de `mode`; `byPc`: lo cambió el PC por su cuenta.
        var onModeChanged: (String, Bool) -> Void
        var onPadChanged: (String) -> Void
        var onNotice: (String) -> Void
        var onClosed: () -> Void
    }

    /// Sin nada del PC en este tiempo (el latido es 1 Hz), la conexión está muerta.
    static let readTimeoutSec = 7
    static let connectTimeoutSec = 4

    private let queue = DispatchQueue(label: "pepomote.control")
    private let conn: NWConnection
    private let token: String
    private let deviceName: String
    private let deviceModel: String
    private let role: String
    private let callbacks: Callbacks
    private let defaultPort: Int
    private var buffer = Data()
    private var running = true
    private var closedNotified = false
    private var lastDataNs: UInt64 = DispatchTime.now().uptimeNanoseconds
    private var pinger: DispatchSourceTimer?
    private var watchdog: DispatchSourceTimer?

    init(
        host: String,
        port: Int,
        token: String,
        deviceName: String,
        deviceModel: String,
        role: String,
        callbacks: Callbacks
    ) {
        self.token = token
        self.deviceName = deviceName
        self.deviceModel = deviceModel
        self.role = role
        self.callbacks = callbacks
        defaultPort = port
        let tcp = NWProtocolTCP.Options()
        tcp.noDelay = true
        tcp.connectionTimeout = ControlClient.connectTimeoutSec
        let params = NWParameters(tls: nil, tcp: tcp)
        params.serviceClass = .responsiveData
        let nwPort = NWEndpoint.Port(rawValue: UInt16(clamping: port)) ?? NWEndpoint.Port(rawValue: 26761)!
        conn = NWConnection(host: NWEndpoint.Host(host), port: nwPort, using: params)
        conn.stateUpdateHandler = { [weak self] state in self?.onState(state) }
        conn.start(queue: queue)
        startWatchdog()
    }

    // MARK: - Conexión

    private func onState(_ state: NWConnection.State) {
        switch state {
        case .ready:
            lastDataNs = DispatchTime.now().uptimeNanoseconds
            sendHello()
            receiveLoop()
            startPinger()
        case .failed(let error):
            fail("io", error.localizedDescription)
        case .waiting(let error):
            // Sin ruta aún (Wi-Fi levantándose): el watchdog decide
            _ = error
        case .cancelled:
            finishClosed()
        default:
            break
        }
    }

    private func sendHello() {
        var hello: [String: Any] = [
            "m": "hello",
            "pv": 1,
            "token": token,
            "name": deviceName,
            "model": deviceModel,
        ]
        // Ausente = wiimote (receptores anteriores no lo conocen)
        if role == "nunchuk" { hello["role"] = role }
        // Sin `pad`: en Wii U se empieza siempre como GamePad/Pro (lo dice el ok)
        sendJson(hello)
    }

    private func receiveLoop() {
        conn.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, isComplete, error in
            guard let self, self.running else { return }
            if let data, !data.isEmpty {
                self.lastDataNs = DispatchTime.now().uptimeNanoseconds
                self.buffer.append(data)
                self.drainLines()
            }
            if let error {
                self.fail("io", error.localizedDescription)
                return
            }
            if isComplete {
                // El PC cerró
                self.fail(nil, "")
                return
            }
            self.receiveLoop()
        }
    }

    private func drainLines() {
        while let nl = buffer.firstIndex(of: 0x0A) {
            let line = buffer.subdata(in: buffer.startIndex..<nl)
            buffer.removeSubrange(buffer.startIndex...nl)
            handle(line: line)
        }
        if buffer.count > 1 << 20 { buffer.removeAll() } // basura sin `\n`: no es un receptor
    }

    private func handle(line: Data) {
        guard !line.isEmpty,
              let obj = try? JSONSerialization.jsonObject(with: line) as? [String: Any]
        else { return } // una línea que no sea JSON no rompe el bucle
        let m = obj["m"] as? String ?? ""
        switch m {
        case "ok":
            let ok = Ok(
                sessionId: UInt32(truncatingIfNeeded: (obj["session_id"] as? NSNumber)?.int64Value ?? 0),
                udpPort: (obj["udp_port"] as? NSNumber)?.intValue ?? defaultPort,
                mode: obj["mode"] as? String ?? "pointer",
                slot: (obj["slot"] as? NSNumber)?.intValue ?? 0,
                role: obj["role"] as? String ?? role,
                player: (obj["player"] as? NSNumber)?.intValue ?? 0,
                supportsCemu: ControlClient.supportsCemu(obj),
                pad: obj["pad"] as? String ?? "gamepad",
                name: obj["name"] as? String ?? ""
            )
            DispatchQueue.main.async { self.callbacks.onOk(ok) }
        case "err":
            let code = obj["code"] as? String ?? ""
            let msg = obj["msg"] as? String ?? ""
            running = false
            DispatchQueue.main.async { self.callbacks.onError(code, msg) }
            conn.cancel()
        case "ping":
            sendJson(["m": "pong", "t": obj["t"] ?? NSNull()])
        case "mode":
            let mode = obj["mode"] as? String ?? "pointer"
            let byPc = (obj["by"] as? String) == "pc"
            DispatchQueue.main.async { self.callbacks.onModeChanged(mode, byPc) }
        case "pad":
            let pad = obj["pad"] as? String ?? "gamepad"
            DispatchQueue.main.async { self.callbacks.onPadChanged(pad) }
        case "notice":
            if let text = obj["text"] as? String, !text.trimmingCharacters(in: .whitespaces).isEmpty {
                DispatchQueue.main.async { self.callbacks.onNotice(text) }
            }
        default:
            break // pong o mensaje desconocido: se ignora
        }
    }

    /// `ok.modes` contiene "cemu" (un receptor antiguo no manda `modes`).
    static func supportsCemu(_ ok: [String: Any]) -> Bool {
        guard let modes = ok["modes"] as? [Any] else { return false }
        return modes.contains { ($0 as? String) == "cemu" }
    }

    // MARK: - Temporizadores

    private func startPinger() {
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now() + 1, repeating: 1)
        t.setEventHandler { [weak self] in
            guard let self, self.running else { return }
            self.sendJson(["m": "ping", "t": DispatchTime.now().uptimeNanoseconds / 1000])
        }
        t.resume()
        pinger = t
    }

    private func startWatchdog() {
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now() + 1, repeating: 1)
        t.setEventHandler { [weak self] in
            guard let self, self.running else { return }
            let silent = DispatchTime.now().uptimeNanoseconds &- self.lastDataNs
            if silent > UInt64(ControlClient.readTimeoutSec) * 1_000_000_000 {
                self.fail("io", "sin respuesta del PC")
            }
        }
        t.resume()
        watchdog = t
    }

    // MARK: - Envío

    func sendMode(_ mode: String) { sendJson(["m": "mode", "mode": mode]) }

    /// Modo Wii U: "wiimote" (Mando de Wii) o "gamepad" (volver a GamePad/Pro).
    func sendPad(_ pad: String) { sendJson(["m": "pad", "pad": pad]) }

    /// Modo Wii U: texto para el teclado en pantalla de Cemu.
    func sendText(_ text: String) { sendLine(TextInput.encode(text)) }

    private func sendJson(_ obj: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: obj, options: [.sortedKeys]), let s = String(data: data, encoding: .utf8) else { return }
        sendLine(s)
    }

    /// Una línea JSON al receptor (el `\n` final se pone aquí).
    private func sendLine(_ text: String) {
        conn.send(content: Data((text + "\n").utf8), completion: .contentProcessed { _ in })
    }

    // MARK: - Cierre

    /// Fallo de red (`code` "io") o cierre limpio del PC (`code` nil).
    private func fail(_ code: String?, _ msg: String) {
        guard running else { return }
        running = false
        if let code {
            DispatchQueue.main.async { self.callbacks.onError(code, msg) }
        }
        conn.cancel()
    }

    private func finishClosed() {
        pinger?.cancel()
        watchdog?.cancel()
        if closedNotified { return }
        closedNotified = true
        DispatchQueue.main.async { self.callbacks.onClosed() }
    }

    func close() {
        queue.async {
            guard self.running else {
                self.conn.cancel()
                return
            }
            self.running = false
            // `bye` y después cerrar (cancel tras procesar el envío)
            let bye = Data("{\"m\":\"bye\"}\n".utf8)
            self.conn.send(content: bye, completion: .contentProcessed { _ in self.conn.cancel() })
        }
    }
}
