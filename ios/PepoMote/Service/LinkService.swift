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
    private var lastPad: String?
    /// Generación del enlace: los callbacks de un ControlClient anterior se ignoran.
    private var generation = 0
    private var control: ControlClient?
    private var udp: UdpSender?
    private var motion: MotionEngine?

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
        lastPad = nil
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
                    self.link.updateConnected { $0.mode = mode }
                    self.link.resolveIntent(mode: mode, byPc: byPc)
                },
                onPadChanged: { [weak self] pad in
                    guard let self, gen == self.generation else { return }
                    self.link.updateConnected { $0.pad = pad }
                },
                onNotice: { [weak self] text in
                    guard let self, gen == self.generation else { return }
                    self.link.publishNotice(text)
                },
                onClosed: { [weak self] in self?.onClosed(gen, pairing) }
            )
        )
    }

    private func onOk(_ ok: ControlClient.Ok, _ gen: Int, _ pairing: Pairing) {
        guard gen == generation else { return }
        let recovered = phase == .reconnecting
        phase = .live
        reconnectAttempt = 0
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
        }) else {
            onError("io", "UDP", gen, pairing)
            return
        }
        udp = sender
        let engine = MotionEngine(sessionId: ok.sessionId, kind: nunchuk ? .nunchuk : .wiimote) { packet in
            sender.send(packet)
        }
        motion = engine
        // Antes de publicar Connected: la pantalla GamePad lo busca al entrar
        link.motion = engine
        engine.start()
        link.sendMode = { [weak self] m in
            self?.lastMode = m
            self?.control?.sendMode(m)
        }
        // Cada conexión empieza como GamePad/Pro; lo pedido se recuerda para reponerlo si hay que reconectar
        link.sendPad = { [weak self] p in
            self?.lastPad = p
            self?.control?.sendPad(p)
        }
        link.sendText = { [weak self] t in self?.control?.sendText(t) }
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
            pad: ok.pad
        )))
        if let m = link.pendingMode {
            link.pendingMode = nil
            lastMode = m
            control?.sendMode(m)
        }
        // Tras una caída, el Mando de Wii vuelve a serlo
        if recovered, lastPad == LinkState.padWiimote { control?.sendPad(LinkState.padWiimote) }
        // Doble pantalla: la pantalla GamePad abre el canal cuando toca; va al
        // mismo puerto que este control. Un Nunchuk no tiene.
        if !nunchuk { ScreenLink.shared.bind(host: pairing.host, port: pairing.port, sessionId: ok.sessionId) }
        UiSounds.shared.prepare()
        UiSounds.shared.connect()
        // "Pulsar la diana" automáticamente al conectar: recentra y centra el cursor
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { ButtonState.shared.bumpRecenter() }
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
        var moved = pairing
        moved.host = f.host
        moved.port = f.tcpPort
        await MainActor.run { PairStore.save(moved) }
        return moved
    }

    /// Cierra el enlace actual (si lo hay) e invalida sus callbacks.
    private func teardownLink() {
        generation += 1
        link.sendMode = nil
        link.sendPad = nil
        link.sendText = nil
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
