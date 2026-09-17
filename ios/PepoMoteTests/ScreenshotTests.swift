import SwiftUI
import XCTest
@testable import PepoMote

/// Capturas de todas las pantallas a PNG, en varios tamaños de iPhone y de
/// iPad, para mirarlas sin dispositivo (la CI las sube como artefacto
/// `ios-shots`). Solo si `PEPOMOTE_SHOTS_DIR` está definido. Se pinta con
/// un UIHostingController en una ventana propia: al contrario que
/// ImageRenderer, así salen también los ScrollView y las rejillas perezosas
/// (Inicio, Conectar, Ajustes).
@MainActor
final class ScreenshotTests: XCTestCase {
    private static let portrait: [(String, CGSize)] = [
        ("iphone-se", CGSize(width: 375, height: 667)),
        ("iphone-16", CGSize(width: 393, height: 852)),
        ("ipad-mini", CGSize(width: 744, height: 1133)),
        ("ipad-11", CGSize(width: 820, height: 1180)),
        ("ipad-13", CGSize(width: 1032, height: 1376)),
    ]
    private static let landscape: [(String, CGSize)] = [
        ("iphone-16-land", CGSize(width: 852, height: 393)),
        // lo que queda al iPhone 16 de lado descontando las zonas seguras (59 pt por lado)
        ("iphone-16-land-safe", CGSize(width: 734, height: 393)),
        ("ipad-mini-land", CGSize(width: 1133, height: 744)),
        ("ipad-11-land", CGSize(width: 1180, height: 820)),
        ("ipad-13-land", CGSize(width: 1376, height: 1032)),
    ]
    private static let switchLandscape = landscape + [
        ("iphone-se-land", CGSize(width: 667, height: 320)),
        ("iphone-se-land-safe", CGSize(width: 548, height: 320)),
    ]

    private var dir: URL?

    override func setUpWithError() throws {
        guard let d = ProcessInfo.processInfo.environment["PEPOMOTE_SHOTS_DIR"], !d.isEmpty else {
            throw XCTSkip("sin PEPOMOTE_SHOTS_DIR: no se guardan capturas")
        }
        let url = URL(fileURLWithPath: d, isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        dir = url
    }

    override func tearDown() {
        LinkState.shared.publish(.disconnected)
        LinkState.shared.clearIntent()
        ScreenLink.shared.release()
        AppModel.shared.nunchukSide = .unset
        AppModel.shared.gamePadSide = .unset
        UserDefaults.standard.removeObject(forKey: AppPrefs.gamePadNoScreenKey)
        UserDefaults.standard.removeObject(forKey: AppPrefs.gamePadFullScreenKey)
        UserDefaults.standard.removeObject(forKey: AppPrefs.gamePadFullScreenKeyboardKey)
    }

    private func connected(mode: String, pad: String = LinkState.padGamepad, role: String = LinkState.roleWiimote, slot: Int = 0) -> UiLink {
        .connected(ConnectedLink(pcName: "SALON", mode: mode, rttMs: 12, sensorHz: 100, slot: slot, role: role, player: slot + 1, supportsCemu: true, pad: pad, supportsSwitch: true, supportsRetroArch: true))
    }

    private func shoot<V: View>(_ view: V, _ name: String, _ size: CGSize, minBytes: Int = 10_000, settleSeconds: Double = 0.15) throws {
        guard let dir else { return }
        let content = view
            .environmentObject(AppModel.shared)
            .environmentObject(L10n.shared)
            // Las capturas muestran el estado final, nunca una mezcla de la
            // tarjeta que se está plegando y los botones que quedan debajo.
            .transaction { transaction in
                transaction.animation = nil
                transaction.disablesAnimations = true
            }
            .frame(width: size.width, height: size.height)
            .background(Pepo.background)
        let host = UIHostingController(rootView: content)
        // Sin zonas seguras del dispositivo del simulador (un iPad no lleva la
        // muesca del iPhone): la pantalla se pinta de borde a borde
        if #available(iOS 16.4, *) { host.safeAreaRegions = [] }
        let bounds = CGRect(origin: .zero, size: size)
        let window = UIWindow(frame: bounds)
        window.rootViewController = host
        window.isHidden = false
        host.view.frame = bounds
        host.view.setNeedsLayout()
        host.view.layoutIfNeeded()
        // una vuelta del run loop: las rejillas perezosas crean sus celdas
        RunLoop.main.run(until: Date().addingTimeInterval(settleSeconds))
        let format = UIGraphicsImageRendererFormat()
        format.scale = 2
        let image = UIGraphicsImageRenderer(size: size, format: format).image { ctx in
            if !host.view.drawHierarchy(in: bounds, afterScreenUpdates: true) {
                host.view.layer.render(in: ctx.cgContext)
            }
        }
        window.isHidden = true
        window.rootViewController = nil
        guard let png = image.pngData() else {
            XCTFail("\(name): sin imagen")
            return
        }
        XCTAssertGreaterThan(png.count, minBytes, "\(name): la captura parece vacía")
        try png.write(to: dir.appendingPathComponent("\(name).png"))
    }

    func testPantallasVerticales() throws {
        for (dev, size) in Self.portrait {
            LinkState.shared.publish(.disconnected)
            try shoot(HomeScreen(), "home-\(dev)", size)
            try shoot(PairScreen(), "pair-\(dev)", size)
            try shoot(SettingsScreen(), "settings-\(dev)", size)
            try shoot(OnboardingScreen(onDone: {}), "onboarding-\(dev)", size)
            LinkState.shared.publish(connected(mode: LinkState.modePointer))
            try shoot(ControllerScreen(showChips: true, onDisconnect: {}), "controller-pointer-\(dev)", size)
            // Dolphin: los tres modos y el interruptor «Nunchuk» (verde) en una fila
            LinkState.shared.publish(connected(mode: LinkState.modeDolphin))
            try shoot(ControllerScreen(showChips: true, onDisconnect: {}), "controller-dolphin-\(dev)", size)
            LinkState.shared.publish(connected(mode: LinkState.modeCemu, pad: LinkState.padWiimote))
            try shoot(ControllerScreen(showChips: true, onDisconnect: {}), "controller-wiiu-wiimote-\(dev)", size)
            LinkState.shared.publish(connected(mode: LinkState.modeDolphin, pad: "nunchuk", role: LinkState.roleNunchuk, slot: 1))
            try shoot(NunchukScreen(onDisconnect: {}), "nunchuk-\(dev)", size)
        }
    }

    func testPantallasApaisadas() throws {
        for (dev, size) in Self.landscape {
            // El lado del GamePad ya elegido en casi todas; la pregunta, en una
            AppModel.shared.gamePadSide = .left
            LinkState.shared.publish(connected(mode: LinkState.modePointer))
            try shoot(ControllerLandscapeScreen(showChips: true, onDisconnect: {}), "landscape-pointer-\(dev)", size)
            LinkState.shared.publish(connected(mode: LinkState.modeCemu))
            try shoot(GamePadScreen(onDisconnect: {}), "gamepad-\(dev)", size)
            AppModel.shared.gamePadSide = .unset
            // Con el modo aún sin confirmar la tarjeta de la cabecera no se
            // pliega sola y la pregunta del lado se enseña debajo de ella
            LinkState.shared.publish(connected(mode: LinkState.modePointer))
            LinkState.shared.requestMode(LinkState.modeCemu)
            try shoot(GamePadScreen(onDisconnect: {}), "gamepad-ask-\(dev)", size)
            LinkState.shared.clearIntent()
            LinkState.shared.pendingMode = nil
            AppModel.shared.gamePadSide = .left
            LinkState.shared.publish(connected(mode: LinkState.modeCemu, pad: LinkState.padPro, slot: 1))
            try shoot(GamePadScreen(onDisconnect: {}), "gamepad-pro-\(dev)", size)
            // Ajuste «GamePad sin pantalla táctil»: botones más grandes
            UserDefaults.standard.set(true, forKey: AppPrefs.gamePadNoScreenKey)
            LinkState.shared.publish(connected(mode: LinkState.modeCemu))
            try shoot(GamePadScreen(onDisconnect: {}), "gamepad-noscreen-\(dev)", size)
            UserDefaults.standard.removeObject(forKey: AppPrefs.gamePadNoScreenKey)
            // Ajuste «Pantalla del GamePad a pantalla completa»: negro, ✕ y Teclado,
            // el placeholder de la pantalla (sin canal en los tests: pesa poco)
            UserDefaults.standard.set(true, forKey: AppPrefs.gamePadFullScreenKey)
            LinkState.shared.publish(connected(mode: LinkState.modeCemu))
            try shoot(GamePadScreen(onDisconnect: {}), "gamepad-fullscreen-\(dev)", size, minBytes: 2_000)
            UserDefaults.standard.removeObject(forKey: AppPrefs.gamePadFullScreenKey)
            LinkState.shared.publish(connected(mode: LinkState.modeDolphin, pad: "nunchuk", role: LinkState.roleNunchuk, slot: 1))
            try shoot(NunchukScreen(onDisconnect: {}), "nunchuk-\(dev)", size)
            // Dolphin con el Nunchuk en el mismo móvil (confirmado por el receptor)
            LinkState.shared.publish(.connected(ConnectedLink(pcName: "SALON", mode: LinkState.modeDolphin, rttMs: 12, sensorHz: 100, slot: 0, role: LinkState.roleWiimote, player: 1, supportsCemu: true, pad: LinkState.padGamepad, ownNunchuk: true)))
            AppModel.shared.nunchukSide = .left
            try shoot(WiimoteNunchukScreen(showChips: true, onDisconnect: {}), "wii-nunchuk-\(dev)", size)
            // La primera vez: la pregunta del lado encima del mando
            AppModel.shared.nunchukSide = .unset
            try shoot(WiimoteNunchukScreen(showChips: true, onDisconnect: {}), "wii-nunchuk-ask-\(dev)", size)
        }
    }

    /// RetroArch: el mando de cada consola (NES, Mega Drive, N64, PlayStation) y el RetroPad completo.
    func testRetroArchConsolePads() throws {
        AppModel.shared.gamePadSide = .left
        let state = LinkState.shared
        state.clearIntent()
        for (device, size) in [
            ("iphone-16", CGSize(width: 852, height: 393)),
            ("iphone-se-safe", CGSize(width: 548, height: 320)),
            ("ipad-mini", CGSize(width: 1133, height: 744)),
        ] {
            for (console, title) in [("nes", "Alter Ego"), ("md", "Cave Story"), ("n64", "Mario 64"), ("psx", "Crash"), ("retropad", "")] {
                if case .connected(var c) = connected(mode: LinkState.modeRetroArch, pad: LinkState.padRetroPad) {
                    c.game = title.isEmpty ? nil : RetroGame(console: console, system: "", core: "core", title: title, path: "/roms/\(title).zip")
                    state.publish(.connected(c))
                }
                // Una imagen con píxeles no basta: tiene que ser el mando
                // operativo de esta consola, sin el aviso «Activando Wii U».
                XCTAssertTrue(Route.isRetroPad(state.link))
                XCTAssertTrue(Route.extendedOperative(state.link, state.intent))
                XCTAssertEqual(Route.wantedMode(state.link, state.intent), LinkState.modeRetroArch)
                XCTAssertEqual(state.effectiveRetroLayout(state.link.connected), console)
                try shoot(GamePadScreen(onDisconnect: {}), "gamepad-retro-\(console)-\(device)", size,
                          settleSeconds: HeaderCollapse.autoSeconds + 0.3)
            }
        }
    }

    func testSwitchControllers() throws {
        AppModel.shared.gamePadSide = .left
        for (dev, size) in Self.switchLandscape {
            LinkState.shared.publish(connected(mode: LinkState.modeSwitch, pad: LinkState.padPro))
            try shoot(GamePadScreen(onDisconnect: {}), "gamepad-switch-\(dev)", size)
        }
        LinkState.shared.publish(connected(mode: LinkState.modeCemu))
        LinkState.shared.requestMode(LinkState.modeSwitch)
        try shoot(GamePadScreen(onDisconnect: {}), "gamepad-switch-activating", CGSize(width: 548, height: 320))
        LinkState.shared.clearIntent()
        LinkState.shared.pendingMode = nil
    }
}
