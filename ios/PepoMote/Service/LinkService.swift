import Foundation
import UIKit

/// Mantiene vivo el enlace: canal de control TCP, socket UDP y sensores
/// (el equivalente del servicio en primer plano de Android; en iOS la app
/// tiene que estar en pantalla). Todo en la cola principal.
///
/// Initial: aún no hubo `ok` (los fallos de red se reintentan 3 veces y se
/// rinde). Live: sesión viva. Reconnecting: la sesión se cayó y se rehace
/// sola (espera creciente, hasta dos minutos).
final class LinkService {
    static let shared = LinkService()
    private static let maxAttempts = 3

    private enum Phase { case initial, live, reconnecting }

    private let link = LinkState.shared
    private var phase = Phase.initial
    private var attempt = 0
    private var role = LinkState.roleWiimote
    private var droppedAtMs: Int64 = 0
    private var reconnectAttempt = 0
    private var reconnectWork: DispatchWorkItem?
    /// Último modo y tipo de mando pedidos por la UI: se reponen al reconectar.
    private var lastMode: String?
    /// Generación del enlace: los callbacks de un ControlClient anterior se ignoran.
    private var generation = 0
    private var control: ControlClient?
    private var udp: UdpSender?
    private var motion: MotionEngine?
    /// Vibración de los juegos: la máquina (contrato §2) y su reloj (0,1 s en
    /// la cola principal, solo mientras vibra), que para sola si el receptor
    /// deja de refrescar el RUMBLE.
    private let rumbleTrack = RumbleTrack()
    private var rumbleTimer: Timer?

    /// `role`: wiimote (mando) o nunchuk (móvil de la otra mano). Un start()
    /// con el enlace ya vivo reemplaza el enlace entero.
    func start(role: String = LinkState.roleWiimote) {
        link.role = role
        link.publish(.connecting)
        guard let pairing = PairStore.current() else {
            stop()
            return
        }
        self.role = role
        teardownLink()
        cancelReconnect()
        phase = .initial
        lastMode = nil
        ButtonState.shared.reset()
        UIApplication.shared.isIdleTimerDisabled = true
        attempt = 0
        connect(pairing)
    }

    /// Salir / desconectar (el `onDestroy` del servicio de Android).
    func stop() {
        if case .connected = link.link { UiSounds.shared.disconnect() }
        cancelReconnect()
        teardownLink()
        UIApplication.shared.isIdleTimerDisabled = false
        if link.link.alive { link.publish(.disconnected) }
    }

    private static func deviceName() -> String {
        // En iOS 16+ es el genérico («iPad») salvo con permiso especial: sirve
        // igual, lo que importa es que sea estable entre reconexiones
        UIDevice.current.name
    }

    private static func deviceModel() -> String {
        "Apple \(UIDevice.current.model)"
    }

    private func connect(_ pairing: Pairing) {
        attempt += 1
        generation += 1
        let gen = generation
        control = ControlClient(
            host: pairing.host,
            port: pairing.port,
            token: pairing.token,
            deviceName: LinkService.deviceName(),
            deviceModel: LinkService.deviceModel(),
            role: role,
            callbacks: ControlClient.Callbacks(
                onOk: { [weak self] ok in self?.onOk(ok, gen, pairing) },
                onError: { [weak self] code, msg in self?.onError(code, msg, gen, pairing) },
                onModeChanged: { [weak self] mode, byPc in
                    guard let self, gen == self.generation else { return }
                    let changed = self.link.link.connected?.mode != mode
                    if changed {
                        ButtonState.shared.reset()
                        self.motion?.kind = self.role == LinkState.roleNunchuk ? .nunchuk : .wiimote
                        if mode != LinkState.modeCemu { ScreenLink.shared.release() }
                        // En modo puntero no hay juego que vibre
                        if mode == LinkState.modePointer { self.resetRumble() }
                    }
                    self.link.updateConnected {
                        $0.mode = mode
                    }
                    self.link.resolveIntent(mode: mode, byPc: byPc)
                    self.lastMode = mode
                    if changed { self.restorePad(mode: mode) }
                },
                onPadChanged: { [weak self] pad, player in
                    guard let self, gen == self.generation else { return }
                    let current = self.link.link.connected
                    if current?.pad != pad || (player != nil && current?.player != player) {
                        ButtonState.shared.reset()
                        self.motion?.kind = self.role == LinkState.roleNunchuk ? .nunchuk : .wiimote
                    }
                    self.link.updateConnected {
                        $0.pad = pad
                        if let player, (1...4).contains(player) { $0.player = player }
                    }
                    self.syncRetroLayout()
                },
                onNunchukChanged: { [weak self] own in
                    guard let self, gen == self.generation else { return }
                    self.link.updateConnected { $0.ownNunchuk = own }
                },
                onScreenOnlyChanged: { [weak self] on in
                    guard let self, gen == self.generation else { return }
                    self.link.updateConnected { $0.screenOnly = on }
                },
                onGameChanged: { [weak self] game in
                    guard let self, gen == self.generation else { return }
                    self.link.updateConnected { $0.game = game }
                    self.syncRetroLayout()
                },
                onNotice: { [weak self] text in
                    guard let self, gen == self.generation else { return }
                    // Ajuste «Avisos del PC en pantalla»: apagado, ni banner
                    if AppPrefs.receiverNotices { self.link.publishNotice(text) }
                },
                onClosed: { [weak self] in self?.onClosed(gen, pairing) }
            ),
            ownNunchuk: AppPrefs.ownNunchuk,
            screenOnly: AppPrefs.gamePadFullScreen,
            pad: AppPrefs.switchPad
        )
    }

    private func onOk(_ ok: ControlClient.Ok, _ gen: Int, _ pairing: Pairing) {
        guard gen == generation else { return }
        phase = .live
        reconnectAttempt = 0
        resetRumble() // sesión nueva: el receptor numera los RUMBLE desde cero
        let nunchuk = ok.role == LinkState.roleNunchuk
        // El PC se ha renombrado: el emparejamiento se actualiza solo
        var pcName = pairing.pcName
        if !ok.name.trimmingCharacters(in: .whitespaces).isEmpty, ok.name != pairing.pcName {
            pcName = ok.name
            var renamed = pairing
            renamed.pcName = ok.name
            PairStore.save(renamed)
        }
        guard let sender = UdpSender(host: pairing.host, port: ok.udpPort, sessionId: ok.sessionId, onRtt: { rtt in
            DispatchQueue.main.async { LinkState.shared.updateConnected { $0.rttMs = rtt } }
        }, onRumble: { [weak self] rumble in
            // Del hilo del socket a la cola principal: el motor solo vive ahí
            DispatchQueue.main.async { self?.onRumble(rumble, gen) }
        }) else {
            onError("io", "UDP", gen, pairing)
            return
        }
        udp = sender
        let engine = MotionEngine(sessionId: ok.sessionId, kind: nunchuk ? .nunchuk : .wiimote) { packet in
            sender.send(packet)
        }
        motion = engine
        engine.reportFrameRotation = ok.receiver.frameRotation
        // Antes de publicar Connected: la pantalla GamePad lo busca al entrar
        link.motion = engine
        engine.start()
        link.sendMode = { [weak self] m in
            self?.lastMode = m
            self?.control?.sendMode(m)
        }
        // Preferencias separadas: el vocabulario de un modo no reemplaza al otro.
        link.sendPad = { [weak self] p in
            guard let self, let current = self.link.link.connected, current.receiver.acceptsPad(p) else { return }
            let mode = current.mode
            if mode == LinkState.modeSwitch, LinkState.validSwitchPad(p) { AppPrefs.switchPad = p }
            else if mode == LinkState.modeCemu, [LinkState.padGamepad, LinkState.padWiimote].contains(p) { AppPrefs.cemuPad = p }
            else if mode == LinkState.modeRetroArch, LinkState.validRetroPad(p) { AppPrefs.retroPad = p }
            else { return }
            ButtonState.shared.reset()
            self.control?.sendPad(p)
        }
        link.sendText = { [weak self] t in
            guard let self, self.link.link.connected?.receiver.textInput == true else { return }
            self.control?.sendText(t)
        }
        link.sendNunchuk = { [weak self] own in self?.control?.sendNunchuk(own) }
        link.sendScreenOnly = { [weak self] on in self?.control?.sendScreenOnly(on) }
        link.sendHotkey = { [weak self] name, down in
            guard let self, self.link.link.connected?.mode == LinkState.modeRetroArch else { return }
            self.control?.sendHotkey(name, down: down)
        }
        // RetroArch: la plantilla de consola elegida a mano, y el gancho para
        // avisar al receptor cuando cambie la efectiva
        link.loadRetroLayouts()
        link.sendLayout = { [weak self] in self?.syncRetroLayout() }
        link.publish(.connected(ConnectedLink(
            pcName: pcName,
            mode: ok.mode,
            rttMs: nil,
            sensorHz: 0,
            slot: ok.slot,
            role: ok.role,
            // Receptor sin "player" en el ok: el jugador es el slot
            player: ok.player > 0 ? ok.player : ok.slot + 1,
            supportsCemu: ok.supportsCemu,
            pad: ok.pad,
            ownNunchuk: ok.nunchuk == "own",
            screenOnly: ok.screenOnly,
            supportsSwitch: ok.supportsSwitch,
            supportsRetroArch: ok.supportsRetroArch,
            supportsGamepad: ok.supportsGamepad,
            receiver: ok.receiver
        )))
        if let m = link.pendingMode {
            link.pendingMode = nil
            lastMode = m
            control?.sendMode(m)
            if m == ok.mode { restorePad(mode: ok.mode) }
        } else {
            lastMode = ok.mode
            restorePad(mode: ok.mode)
        }
        // Doble pantalla: la pantalla GamePad abre el canal cuando toca; va al
        // mismo puerto que este control. Un Nunchuk no tiene.
        if !nunchuk { ScreenLink.shared.bind(host: pairing.host, port: pairing.port, sessionId: ok.sessionId) }
        UiSounds.shared.prepare()
        UiSounds.shared.connect()
        // "Pulsar la diana" automáticamente al conectar: recentra y centra el cursor
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { ButtonState.shared.bumpRecenter() }
    }

    private func restorePad(mode: String) {
        guard role == LinkState.roleWiimote else { return }
        if mode == LinkState.modeSwitch { control?.sendPad(AppPrefs.switchPad) }
        else if mode == LinkState.modeCemu { control?.sendPad(AppPrefs.cemuPad) }
        else if mode == LinkState.modeRetroArch {
            // con la plantilla de consola que toca (solo cuenta siendo el RetroPad)
            guard let receiver = link.link.connected?.receiver else { return }
            let pad = receiver.restoredPad(AppPrefs.retroPad)
            let layout = link.wouldBeRetroLayout(link.link.connected)
            lastSentLayout = pad == LinkState.padRetroPad ? layout : nil
            control?.sendPad(pad, layout: layout)
        }
    }

    /// RetroArch: última plantilla de consola avisada al receptor (`pad.layout`).
    private var lastSentLayout: String?

    /// RetroArch como RetroPad: la plantilla de consola que se enseña (por el
    /// juego que anunció el PC o la elegida a mano) va al receptor en cuanto
    /// cambia; si cambia jugando, se suelta todo y se avisa en pantalla.
    private func syncRetroLayout() {
        guard let cur = link.link.connected else { return }
        guard cur.mode == LinkState.modeRetroArch, cur.pad == LinkState.padRetroPad else {
            lastSentLayout = nil
            return
        }
        let eff = link.effectiveRetroLayout(cur)
        if eff == lastSentLayout { return }
        if lastSentLayout != nil {
            ButtonState.shared.reset()
            link.publishNotice(tr("retro_layout_changed", RetroLayouts.byId(eff)?.name ?? eff))
        }
        lastSentLayout = eff
        control?.sendPad(LinkState.padRetroPad, layout: eff)
    }

    private func onError(_ code: String, _ msg: String, _ gen: Int, _ pairing: Pairing) {
        guard gen == generation else { return }
        if code != "io" {
            // El PC nos rechaza (token, código, ocupado…): no hay reintento que valga
            link.publish(.failed(code: code, msg: msg))
            stop()
            return
        }
        if phase != .initial {
            // La sesión estaba viva (o se estaba rehaciendo): se sigue intentando solo
            dropped(pairing)
            return
        }
        // Fallos de red transitorios: reintenta antes de rendirse
        if attempt < LinkService.maxAttempts {
            generation += 1
            let retryGen = generation
            Task { [weak self] in
                guard let self else { return }
                let next = await self.relocate(pairing)
                await MainActor.run {
                    if retryGen == self.generation { self.connect(next) }
                }
            }
        } else {
            link.publish(.failed(code: code, msg: msg))
            stop()
        }
    }

    private func onClosed(_ gen: Int, _ pairing: Pairing) {
        guard gen == generation else { return }
        var isFailed = false
        if case .failed = link.link { isFailed = true }
        if phase != .initial, !isFailed {
            // El PC cerró (reinicio, red caída): se rehace sola
            dropped(pairing)
            return
        }
        if !isFailed { link.publish(.disconnected) }
        stop()
    }

    /// La sesión se ha caído con el enlace vivo (o un intento de reconexión ha
    /// fallado): se vuelve a intentar sola, con espera creciente, hasta rendirse
    /// a los dos minutos.
    private func dropped(_ pairing: Pairing) {
        let now = LinkState.nowMs()
        if phase != .reconnecting {
            droppedAtMs = now
            reconnectAttempt = 0
        }
        phase = .reconnecting
        teardownLink()
        ButtonState.shared.reset()
        if Reconnect.giveUp(droppedAtMs, now) {
            link.publish(.failed(code: "io", msg: tr("lost_connection", pairing.pcName)))
            stop()
            return
        }
        reconnectAttempt += 1
        link.publish(.reconnecting(pcName: pairing.pcName, attempt: reconnectAttempt))
        // Lo que el usuario había pedido se vuelve a pedir en cuanto llegue el ok
        if let m = lastMode, m != LinkState.modePointer { link.requestMode(m) }
        scheduleReconnect(pairing)
    }

    private func scheduleReconnect(_ pairing: Pairing) {
        let gen = generation
        let work = DispatchWorkItem { [weak self] in
            guard let self, gen == self.generation else { return }
            Task { [weak self] in
                guard let self else { return }
                let next = await self.relocate(pairing)
                await MainActor.run {
                    if gen == self.generation { self.connect(next) }
                }
            }
        }
        reconnectWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(Int(Reconnect.delayMs(reconnectAttempt))), execute: work)
    }

    private func cancelReconnect() {
        reconnectWork?.cancel()
        reconnectWork = nil
    }

    /// Entre reintentos, busca el PC por nombre en la red: si ha cambiado de IP
    /// el emparejamiento se actualiza solo. El token no cambia: vive en el PC.
    private func relocate(_ pairing: Pairing) async -> Pairing {
        let found = await Discovery.scan(timeoutMs: 1200)
        guard let f = found.first(where: { $0.name == pairing.pcName && ($0.host != pairing.host || $0.tcpPort != pairing.port) }) else {
            return pairing
        }
        let moved = Pairing(host: f.host, port: f.tcpPort, token: pairing.token, pcName: pairing.pcName)
        await MainActor.run { PairStore.save(moved) }
        return moved
    }

    // MARK: - Vibración de los juegos

    private static func rumbleNowMs() -> UInt64 { DispatchTime.now().uptimeNanoseconds / 1_000_000 }

    /// Un RUMBLE del receptor (ya en la cola principal y de esta sesión).
    private func onRumble(_ rumble: PmpCodec.Rumble, _ gen: Int) {
        guard gen == generation else { return }
        runRumble(rumbleTrack.apply(
            seq: rumble.seq, strong: rumble.strong, weak: rumble.weak, ttlMs: rumble.ttlMs,
            nowMs: LinkService.rumbleNowMs()
        ))
    }

    /// La orden al motor, con la escala del ajuste leída en cada una (cambiar
    /// el ajuste vale al instante). El reloj solo corre mientras vibra.
    private func runRumble(_ command: RumbleCommand?) {
        guard let command = command else { return }
        let scale = RumblePref.scale(AppPrefs.rumble)
        switch command {
        case .start(let level):
            GameRumble.shared.start(level: level * scale)
            startRumbleClock()
        case .change(let level):
            GameRumble.shared.change(level: level * scale)
        case .stop:
            GameRumble.shared.stop()
            stopRumbleClock()
        }
    }

    private func startRumbleClock() {
        guard rumbleTimer == nil else { return }
        let timer = Timer(timeInterval: 0.1, repeats: true) { [weak self] _ in
            guard let self = self else { return }
            self.runRumble(self.rumbleTrack.tick(nowMs: LinkService.rumbleNowMs()))
        }
        RunLoop.main.add(timer, forMode: .common)
        rumbleTimer = timer
    }

    private func stopRumbleClock() {
        rumbleTimer?.invalidate()
        rumbleTimer = nil
    }

    /// Sin enlace, en segundo plano o en modo puntero no queda vibración, y
    /// la máquina olvida la sesión (el siguiente RUMBLE empieza de cero).
    private func resetRumble() {
        _ = rumbleTrack.reset()
        stopRumbleClock()
        GameRumble.shared.stop()
    }

    /// La app deja de verse (o vuelve): sin ella a la vista no queda
    /// vibración (iOS para el motor de todas formas).
    func setForeground(_ foreground: Bool) {
        if !foreground { resetRumble() }
    }

    /// Cierra el enlace actual (si lo hay) e invalida sus callbacks.
    private func teardownLink() {
        generation += 1
        resetRumble()
        link.sendMode = nil
        link.sendPad = nil
        link.sendText = nil
        link.sendNunchuk = nil
        link.sendScreenOnly = nil
        link.sendHotkey = nil
        link.sendLayout = nil
        lastSentLayout = nil
        link.motion = nil
        ScreenLink.shared.unbind() // sin enlace no hay pantalla que recibir
        motion?.stop()
        udp?.close()
        control?.close()
        motion = nil
        udp = nil
        control = nil
    }
}
