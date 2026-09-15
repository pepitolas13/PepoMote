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
        /// Mando efectivo con el vocabulario del modo confirmado.
        let pad: String
        /// Nombre del PC (`ok.name`); vacío si el receptor no lo manda.
        let name: String
        /// El receptor confirma el Nunchuk propio ("own"); "none" si no, o receptor antiguo.
        var nunchuk: String = "none"
        /// El receptor conoce «solo pantalla» (`ok.screen_only`); nil = receptor anterior a 1.6.
        var screenOnly: Bool? = nil
        var supportsSwitch: Bool = false
    }

    struct Callbacks {
        var onOk: (Ok) -> Void
        /// `code` = "io" si es la red; si no, el rechazo del receptor.
        var onError: (String, String) -> Void
        /// Eco o difusión de `mode`; `byPc`: lo cambió el PC por su cuenta.
        var onModeChanged: (String, Bool) -> Void
        var onPadChanged: (String, Int?) -> Void
        /// Eco del `nunchuk`: el receptor aplica (o no) el Nunchuk propio (opcional: los tests no lo usan).
        var onNunchukChanged: (Bool) -> Void = { _ in }
        /// Eco del `screen_only`: el receptor aplica (o no) el modo «solo pantalla».
        var onScreenOnlyChanged: (Bool) -> Void = { _ in }
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
    /// Mando con su propio Nunchuk (`"nunchuk":"own"` en el hello; solo rol mando).
    private let ownNunchuk: Bool
    /// Modo Wii U: el móvil solo hace de pantalla táctil (`"screen_only":true` en el hello; solo rol mando).
    private let screenOnly: Bool
    private let pad: String?
    private let callbacks: Callbacks
    private let defaultPort: Int
    private var buffer = Data()
    private var mode = LinkState.modePointer
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
        callbacks: Callbacks,
        ownNunchuk: Bool = false,
        screenOnly: Bool = false,
        pad: String? = nil
    ) {
        self.token = token
        self.deviceName = deviceName
        self.deviceModel = deviceModel
        self.role = role
        self.ownNunchuk = ownNunchuk
        self.screenOnly = screenOnly
        self.pad = pad.map { ["joycons", "joycon_side", "joycon_r"].contains($0) ? LinkState.padPro : $0 }
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
        // Nunchuk en el mismo móvil: un receptor antiguo lo ignora (y no lo confirma)
        if role != "nunchuk", ownNunchuk { hello["nunchuk"] = "own" }
        // Solo pantalla (Wii U): un receptor antiguo lo ignora (y no lo confirma)
        if role != "nunchuk", screenOnly { hello["screen_only"] = true }
        if role != "nunchuk", let pad { hello["pad"] = pad }
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
            mode = obj["mode"] as? String ?? LinkState.modePointer
            let ok = Ok(
                sessionId: UInt32(truncatingIfNeeded: (obj["session_id"] as? NSNumber)?.int64Value ?? 0),
                udpPort: (obj["udp_port"] as? NSNumber)?.intValue ?? defaultPort,
                mode: obj["mode"] as? String ?? "pointer",
                slot: (obj["slot"] as? NSNumber)?.intValue ?? 0,
                role: obj["role"] as? String ?? role,
                player: (obj["player"] as? NSNumber)?.intValue ?? 0,
                supportsCemu: ControlClient.supportsCemu(obj),
                pad: mode == LinkState.modeSwitch ? LinkState.padPro : obj["pad"] as? String ?? "gamepad",
                name: obj["name"] as? String ?? "",
                nunchuk: obj["nunchuk"] as? String ?? "none",
                screenOnly: obj["screen_only"] as? Bool,
                supportsSwitch: ControlClient.supportsSwitch(obj)
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
            self.mode = mode
            let byPc = (obj["by"] as? String) == "pc"
            DispatchQueue.main.async { self.callbacks.onModeChanged(mode, byPc) }
        case "pad":
            let pad = mode == LinkState.modeSwitch ? LinkState.padPro : obj["pad"] as? String ?? "gamepad"
            let rawPlayer = (obj["player"] as? NSNumber)?.intValue
            let player = rawPlayer.flatMap { (1...4).contains($0) ? $0 : nil }
            DispatchQueue.main.async { self.callbacks.onPadChanged(pad, player) }
        case "nunchuk":
            let own = obj["own"] as? Bool ?? false
            DispatchQueue.main.async { self.callbacks.onNunchukChanged(own) }
        case "screen_only":
            let on = obj["on"] as? Bool ?? false
            DispatchQueue.main.async { self.callbacks.onScreenOnlyChanged(on) }
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

    static func supportsSwitch(_ ok: [String: Any]) -> Bool {
        guard let modes = ok["modes"] as? [Any] else { return false }
        return modes.contains { ($0 as? String) == LinkState.modeSwitch }
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

    /// Mando dentro del modo actual: Wii U o Switch usan vocabularios distintos.
    func sendPad(_ pad: String) {
        queue.async {
            self.sendJson(["m": "pad", "pad": self.mode == LinkState.modeSwitch ? LinkState.padPro : pad])
        }
    }

    /// Nunchuk en el mismo móvil, encendido o apagado; el receptor lo confirma con el eco.
    func sendNunchuk(_ own: Bool) { sendJson(["m": "nunchuk", "own": own]) }

    /// Modo Wii U: el móvil solo como pantalla táctil, sí o no; el receptor lo confirma con el eco.
    func sendScreenOnly(_ on: Bool) { sendJson(["m": "screen_only", "on": on]) }

    /// Texto para el teclado en pantalla del emulador de Wii U o Switch.
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
