import Foundation
import SwiftUI

/// `controller` es el mando en general: la pantalla real (GamePad de Wii U,
/// layouts Wii o Nunchuk) la decide `Route` a partir del enlace y de la
/// intención pendiente del usuario.
enum Screen {
    case onboarding
    case home
    case pair
    case controller
    case nunchuk
    case settings
}

/// Estado de navegación y acciones del inicio (lo que en Android es MainActivity).
final class AppModel: ObservableObject {
    static let shared = AppModel()

    @Published var screen: Screen
    /// true = se entró al mando por la tarjeta Dolphin (pantalla solo-Dolphin).
    @Published var controllerDolphinOnly = false
    /// Rol con el que se entró (Mando/Dolphin/Wii U = wiimote, Nunchuk = nunchuk).
    @Published var linkRole = LinkState.roleWiimote
    /// Por qué está abierta la pantalla Conectar (rechazo del PC); nil = normal.
    @Published var pairReason: String?
    /// Sube con cada cambio de la lista de PCs guardados (la pantalla Conectar la relee).
    @Published var pairVersion = 0
    /// Aviso corto (el Toast de Android).
    @Published var toast: String?
    /// Escáner QR abierto.
    @Published var scanning = false

    private let link = LinkState.shared
    private let service = LinkService.shared

    init() {
        screen = AppPrefs.onboarded ? .home : .onboarding
    }

    func showToast(_ text: String) {
        toast = text
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.5) { [weak self] in
            if self?.toast == text { self?.toast = nil }
        }
    }

    /// A la pantalla Conectar (escáner QR), con explicación si viene de un rechazo.
    func openPair(reason: String? = nil) {
        pairReason = reason
        screen = .pair
    }

    /// El enlace ha muerto con error (el servicio ya se ha parado).
    func onLinkFailed(code: String, msg: String) {
        link.clearFailure()
        let pc = LinkFailure.pcLabel(PairStore.current()?.pcName, tr("your_pc"))
        if LinkFailure.needsNewQr(code) {
            openPair(reason: tr("re_pair_reason", pc))
        } else if LinkFailure.offerAnotherPc(code, PairStore.all().count) {
            openPair(reason: tr("pc_not_responding", pc))
        } else {
            showToast(tr("error_prefix", msg))
            link.pendingMode = nil
            screen = .home
        }
    }

    /// Hay un enlace vivo (o arrancando o rehaciéndose) como Nunchuk.
    private func linkIsNunchuk() -> Bool {
        link.link.alive && link.role == LinkState.roleNunchuk
    }

    /// Al mando (wiimote) en el modo pedido. `cemu` deja intención Wii U.
    func openController(mode: String, dolphinOnly: Bool) {
        controllerDolphinOnly = dolphinOnly
        linkRole = LinkState.roleWiimote
        if link.link.alive, !linkIsNunchuk() {
            link.requestMode(mode)
            screen = .controller
        } else if PairStore.current() != nil {
            link.requestMode(mode) // se aplica al llegar el ok
            service.start()
            screen = .controller
        } else {
            link.requestMode(mode)
            openPair()
        }
    }

    /// Tarjeta «Mando»: al mando sin tocar el modo del receptor.
    func openPad() {
        controllerDolphinOnly = false
        linkRole = LinkState.roleWiimote
        if PairStore.current() == nil {
            openPair()
        } else {
            if linkIsNunchuk() { service.start() }
            screen = .controller
        }
    }

    /// Un PC de «Tus PCs»: pasa a ser el actual y se conecta con él.
    func onSavedPcChosen(_ p: Pairing) {
        PairStore.select(p.token)
        pairVersion += 1
        pairReason = nil
        if linkRole == LinkState.roleWiimote, let m = link.pendingMode { link.requestMode(m) }
        service.start(role: linkRole)
        screen = linkRole == LinkState.roleNunchuk ? .nunchuk : .controller
    }

    func onForgetPc(_ p: Pairing) {
        PairStore.forget(p.token)
        pairVersion += 1
    }

    /// Tarjeta «Nunchuk»: el móvil de la otra mano.
    func openNunchuk() {
        linkRole = LinkState.roleNunchuk
        if linkIsNunchuk() {
            screen = .nunchuk
        } else if PairStore.current() != nil {
            service.start(role: LinkState.roleNunchuk)
            screen = .nunchuk
        } else {
            openPair()
        }
    }

    /// Contenido del QR o del enlace profundo: si vale, empareja, arranca el enlace y abre su pantalla.
    func onPairContent(_ contents: String) {
        guard let pairing = PairStore.parsePairUrl(contents) else {
            showToast(tr("qr_not_pepomote"))
            return
        }
        PairStore.save(pairing)
        pairVersion += 1
        pairReason = nil
        if linkRole == LinkState.roleWiimote, let m = link.pendingMode { link.requestMode(m) }
        service.start(role: linkRole)
        screen = linkRole == LinkState.roleNunchuk ? .nunchuk : .controller
    }

    /// «Salir» del mando: desconecta y al inicio.
    func disconnect() {
        link.clearIntent()
        service.stop()
        screen = .home
    }

    /// «Salir» del Nunchuk.
    func disconnectNunchuk() {
        service.stop()
        screen = .home
    }

    /// Reconectar desde una cabecera «Sin conexión».
    func reconnect(role: String) {
        service.start(role: role)
    }
}
