import Foundation

/// Sesión viva con el PC (lo que confirmó el `ok` y sus ecos).
struct ConnectedLink: Equatable {
    var pcName: String
    var mode: String
    var rttMs: Float?
    var sensorHz: Float
    /// 0 = Jugador 1 (controla puntero y modo); 1..3 = jugadores extra
    var slot: Int = 0
    /// "wiimote" o "nunchuk" (lo que confirmó el receptor en el ok).
    var role: String = LinkState.roleWiimote
    /// Jugador (1..4) al que pertenece este móvil.
    var player: Int = 1
    /// El receptor sabe de Wii U (`ok.modes` trae "cemu").
    var supportsCemu: Bool = false
    /// Mando efectivo en modo Wii U: gamepad / pro / wiimote.
    var pad: String = LinkState.padGamepad
}

enum UiLink: Equatable {
    case disconnected
    case connecting
    case connected(ConnectedLink)
    case failed(code: String, msg: String)
    /// La sesión se cayó y el servicio la está rehaciendo solo.
    case reconnecting(pcName: String, attempt: Int)

    /// Hay enlace (vivo, arrancando o rehaciéndose): el mando tiene sentido en pantalla.
    var alive: Bool {
        switch self {
        case .connected, .connecting, .reconnecting: return true
        default: return false
        }
    }

    var connected: ConnectedLink? {
        if case .connected(let c) = self { return c }
        return nil
    }
}

/// Aviso transitorio (del receptor o propio): la UI lo enseña ~6 s desde `atMs`.
struct Notice: Equatable {
    let text: String
    let atMs: Int64
}

/// Intención pendiente del usuario: `wiiU` = tocó la tarjeta Wii U o el chip
/// Wii U y aún no ha llegado ningún eco/difusión de `mode` posterior.
enum PadIntent {
    case none
    case wiiU
}

/// Estado observable del enlace, publicado por `LinkService`. Todo en la cola principal.
final class LinkState: ObservableObject {
    static let shared = LinkState()

    static let roleWiimote = "wiimote"
    static let roleNunchuk = "nunchuk"
    static let modePointer = "pointer"
    static let modeDolphin = "dolphin"
    static let modeCemu = "cemu"
    static let padGamepad = "gamepad"
    static let padPro = "pro"
    static let padWiimote = "wiimote"

    @Published private(set) var link: UiLink = .disconnected
    @Published private(set) var notice: Notice?
    @Published private(set) var intent: PadIntent = .none

    /// Rol del enlace vivo (o arrancando).
    var role: String = LinkState.roleWiimote

    /// Cambia el modo pointer/dolphin/cemu; lo conecta el servicio al ControlClient.
    var sendMode: ((String) -> Void)?
    /// Modo a aplicar en cuanto se complete la próxima conexión.
    var pendingMode: String?
    /// Modo Wii U: elegir "wiimote" (Mando de Wii) o "gamepad" (volver a GamePad/Pro).
    var sendPad: ((String) -> Void)?
    /// Modo Wii U: texto para el teclado en pantalla de Cemu.
    var sendText: ((String) -> Void)?
    /// Motor de sensores del enlace vivo (la pantalla GamePad le fija kind/rotation).
    weak var motion: MotionEngine?

    static func nowMs() -> Int64 { Int64(DispatchTime.now().uptimeNanoseconds / 1_000_000) }

    /// Pide un modo al receptor. `cemu` deja la intención Wii U; cualquier otro
    /// modo la quita. Si el enlace aún no está, se aplica al llegar el `ok`.
    func requestMode(_ mode: String) {
        intent = mode == LinkState.modeCemu ? .wiiU : .none
        if let send = sendMode, case .connected = link {
            send(mode)
        } else {
            pendingMode = mode
        }
    }

    /// Salir / desconectar: no queda nada pendiente.
    func clearIntent() { intent = .none }

    /// Eco o difusión de `mode`: consume la intención Wii U y avisa si no era cemu (salvo si lo decidió el PC).
    func resolveIntent(mode: String, byPc: Bool) {
        let out = Route.afterModeEcho(intent, mode, link, byPc: byPc)
        intent = out.intent
        if let w = out.warning { publishNotice(tr(w)) }
    }

    func publish(_ state: UiLink) {
        link = state
        // Sin enlace no hay intención que mantener
        switch state {
        case .disconnected, .failed: intent = .none
        default: break
        }
    }

    func updateConnected(_ transform: (inout ConnectedLink) -> Void) {
        if case .connected(var c) = link {
            transform(&c)
            link = .connected(c)
        }
    }

    func publishNotice(_ text: String) {
        notice = Notice(text: text, atMs: LinkState.nowMs())
    }

    /// Error ya mostrado: vuelve a Desconectado para que no se re-dispare.
    func clearFailure() {
        if case .failed = link { link = .disconnected }
    }
}
