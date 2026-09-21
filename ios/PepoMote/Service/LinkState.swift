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
    /// Mando efectivo; vocabulario Wii U o Switch según el modo confirmado.
    var pad: String = LinkState.padGamepad
    /// El receptor confirmó el Nunchuk en el mismo móvil (`ok.nunchuk` o eco
    /// de `nunchuk`): en Dolphin, apaisado = mando + Nunchuk.
    var ownNunchuk: Bool = false
    /// El receptor confirmó «solo pantalla» (`ok.screen_only` o eco de
    /// `screen_only`); nil = receptor anterior a 1.6, que no lo conoce.
    var screenOnly: Bool? = nil
    /// El receptor anuncia Switch en `ok.modes`.
    var supportsSwitch: Bool = false
    /// El receptor anuncia RetroArch en `ok.modes` (mando en red de PC o Android).
    var supportsRetroArch: Bool = false
    /// El receptor anuncia el mando universal en `ok.modes` (solo PC).
    var supportsGamepad: Bool = false
    /// RetroArch: el juego cargado según el receptor (mensaje `game`); con su
    /// consola se elige la plantilla de mando. Se conserva al cambiar de modo.
    var game: RetroGame? = nil
    /// Capacidades confirmadas al conectar; los ecos de modo/pad las conservan.
    var receiver = ReceiverCapabilities()

    /// ¿Hay teclado para este enlace? Una sola fuente de verdad: `Route`.
    var hasKeyboard: Bool { Route.showsKeyboard(.connected(self)) }

    /// `ok.rumble`: si el receptor puede hacer vibrar el móvil (Ajustes lo
    /// cuenta); nil = receptor sin vibración.
    var rumble: String? { receiver.rumble }
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

/// Modo de mando pedido y aún sin eco: Wii U y Switch comparten pantalla
/// optimista, pero conservan su identidad y sus avisos por separado.
enum PadIntent {
    case none
    case wiiU
    case switchMode
    case retroArch
    /// Mando universal: el móvil como mando de Xbox 360 para cualquier juego.
    case universalPad

    var mode: String? {
        switch self {
        case .none: return nil
        case .wiiU: return LinkState.modeCemu
        case .switchMode: return LinkState.modeSwitch
        case .retroArch: return LinkState.modeRetroArch
        case .universalPad: return LinkState.modeGamepad
        }
    }
}

/// Estado observable del enlace, publicado por `LinkService`. Todo en la cola principal.
final class LinkState: ObservableObject {
    static let shared = LinkState()

    static let roleWiimote = "wiimote"
    static let roleNunchuk = "nunchuk"
    static let modePointer = "pointer"
    static let modeDolphin = "dolphin"
    static let modeCemu = "cemu"
    static let modeSwitch = "switch"
    static let modeRetroArch = "retroarch"
    /// Mando universal: el móvil como mando de Xbox 360 para cualquier juego.
    static let modeGamepad = "gamepad"
    static let padGamepad = "gamepad"
    static let padPro = "pro"
    static let padWiimote = "wiimote"
    /// Modo RetroArch: mando apaisado de dos sticks, mando de NES (de lado) o pistola de luz.
    static let padRetroPad = "retropad"
    static let padNes = "nes"
    static let padGun = "gun"
    static let retroPads = [padRetroPad, padNes, padGun]

    static func validSwitchPad(_ pad: String) -> Bool {
        pad == padPro
    }

    static func validRetroPad(_ pad: String) -> Bool {
        retroPads.contains(pad)
    }

    @Published private(set) var link: UiLink = .disconnected
    @Published private(set) var notice: Notice?
    @Published private(set) var intent: PadIntent = .none

    /// Rol del enlace vivo (o arrancando).
    var role: String = LinkState.roleWiimote

    /// Cambia el modo pointer/dolphin/cemu/switch; conectado al ControlClient.
    var sendMode: ((String) -> Void)?
    /// Modo a aplicar en cuanto se complete la próxima conexión.
    var pendingMode: String?
    /// Elegir el mando dentro del modo confirmado (vocabulario distinto en cada uno).
    var sendPad: ((String) -> Void)?
    /// Texto para el teclado en pantalla de Wii U o Switch.
    var sendText: ((String) -> Void)?
    /// Nunchuk en el mismo móvil (modo Dolphin): pedirlo o quitarlo; el eco lo confirma.
    var sendNunchuk: ((Bool) -> Void)?
    /// Modo Wii U: el móvil solo como pantalla táctil (pantalla completa); el eco lo confirma.
    var sendScreenOnly: ((Bool) -> Void)?
    /// Modo RetroArch: tecla rápida por su nombre del protocolo y si se pulsa o se suelta.
    var sendHotkey: ((String, Bool) -> Void)?
    /// RetroArch: la plantilla de consola efectiva cambió (juego nuevo, eco de
    /// `pad` o elección a mano): el servicio la manda al receptor (`pad.layout`).
    var sendLayout: (() -> Void)?
    /// RetroArch: mando de consola elegido a mano (preferencias); lo carga el servicio al conectar.
    @Published private(set) var retroLayoutChoice = RetroLayoutChoice()

    func loadRetroLayouts() {
        retroLayoutChoice = AppPrefs.retroLayoutChoice
    }

    /// Elegir a mano (nil = automático) para el juego `path`; se guarda y se avisa al receptor.
    func pickRetroLayout(path: String?, id: String?) {
        let next = retroLayoutChoice.pick(path: path, id: id)
        retroLayoutChoice = next
        AppPrefs.retroLayoutChoice = next
        sendLayout?()
    }

    /// Plantilla que tocaría como RetroPad en esta sesión: la elegida a mano
    /// para el juego (o la global), si no la consola que anunció el PC, si no
    /// el RetroPad completo.
    func wouldBeRetroLayout(_ link: ConnectedLink?) -> String {
        let game = link?.game
        let path = (game?.path).flatMap { $0.isEmpty ? nil : $0 }
        return RetroLayouts.effective(console: game?.console, chosen: retroLayoutChoice.choiceFor(path))
    }

    /// La que se enseña: solo en RetroArch y siendo el RetroPad; si no, el RetroPad completo.
    func effectiveRetroLayout(_ link: ConnectedLink?) -> String {
        guard let link, link.mode == LinkState.modeRetroArch, link.pad == LinkState.padRetroPad else { return RetroLayouts.retroPad.id }
        return wouldBeRetroLayout(link)
    }
    /// Motor de sensores del enlace vivo (la pantalla GamePad le fija kind/rotation).
    weak var motion: MotionEngine?

    static func nowMs() -> Int64 { Int64(DispatchTime.now().uptimeNanoseconds / 1_000_000) }

    /// Pide un modo al receptor. Wii U y Switch dejan su intención pendiente.
    /// Si el enlace aún no está, se aplica al llegar el `ok`.
    func requestMode(_ mode: String) {
        intent = mode == LinkState.modeCemu ? .wiiU
            : mode == LinkState.modeSwitch ? .switchMode
            : mode == LinkState.modeRetroArch ? .retroArch
            : mode == LinkState.modeGamepad ? .universalPad
            : .none
        ButtonState.shared.reset()
        // La doble pantalla es solo de Cemu: los demás modos la sueltan.
        if mode == LinkState.modeSwitch || mode == LinkState.modeRetroArch || mode == LinkState.modeGamepad {
            ScreenLink.shared.release()
        }
        if intent != .none, link.connected?.mode != mode {
            motion?.kind = role == LinkState.roleNunchuk ? .nunchuk : .wiimote
        }
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
        if case .connected(var connected) = state {
            Self.normalizeController(&connected)
            link = .connected(connected)
        } else {
            link = state
        }
        // Sin enlace no hay intención que mantener
        switch state {
        case .disconnected, .failed: intent = .none
        default: break
        }
    }

    func updateConnected(_ transform: (inout ConnectedLink) -> Void) {
        if case .connected(var c) = link {
            transform(&c)
            Self.normalizeController(&c)
            link = .connected(c)
        }
    }

    private static func normalizeController(_ connected: inout ConnectedLink) {
        if connected.mode == modeSwitch { connected.pad = padPro }
        // RetroArch solo conoce sus tres mandos; cualquier otro nombre es el RetroPad
        if connected.mode == modeRetroArch, !validRetroPad(connected.pad) { connected.pad = padRetroPad }
        if connected.mode == modeRetroArch { connected.pad = connected.receiver.restoredPad(connected.pad) }
    }

    func publishNotice(_ text: String) {
        notice = Notice(text: text, atMs: LinkState.nowMs())
    }

    /// Error ya mostrado: vuelve a Desconectado para que no se re-dispare.
    func clearFailure() {
        if case .failed = link { link = .disconnected }
    }
}
