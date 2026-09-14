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
        ScreenLink.shared.release()
        UserDefaults.standard.removeObject(forKey: AppPrefs.gamePadNoScreenKey)
        UserDefaults.standard.removeObject(forKey: AppPrefs.gamePadFullScreenKey)
        UserDefaults.standard.removeObject(forKey: AppPrefs.gamePadFullScreenKeyboardKey)
    }

    private func connected(mode: String, pad: String = LinkState.padGamepad, role: String = LinkState.roleWiimote, slot: Int = 0) -> UiLink {
        .connected(ConnectedLink(pcName: "SALON", mode: mode, rttMs: 12, sensorHz: 100, slot: slot, role: role, player: slot + 1, supportsCemu: true, pad: pad))
    }

    private func shoot<V: View>(_ view: V, _ name: String, _ size: CGSize, minBytes: Int = 10_000) throws {
        guard let dir else { return }
        let content = view
            .environmentObject(AppModel.shared)
            .environmentObject(L10n.shared)
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
        RunLoop.main.run(until: Date().addingTimeInterval(0.15))
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
            LinkState.shared.publish(connected(mode: LinkState.modeCemu, pad: LinkState.padWiimote))
            try shoot(ControllerScreen(showChips: true, onDisconnect: {}), "controller-wiiu-wiimote-\(dev)", size)
            LinkState.shared.publish(connected(mode: LinkState.modeDolphin, pad: "nunchuk", role: LinkState.roleNunchuk, slot: 1))
            try shoot(NunchukScreen(onDisconnect: {}), "nunchuk-\(dev)", size)
        }
    }

    func testPantallasApaisadas() throws {
        for (dev, size) in Self.landscape {
            LinkState.shared.publish(connected(mode: LinkState.modePointer))
            try shoot(ControllerLandscapeScreen(showChips: true, onDisconnect: {}), "landscape-pointer-\(dev)", size)
            LinkState.shared.publish(connected(mode: LinkState.modeCemu))
            try shoot(GamePadScreen(onDisconnect: {}), "gamepad-\(dev)", size)
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
            try shoot(WiimoteNunchukScreen(showChips: true, onDisconnect: {}), "wii-nunchuk-\(dev)", size)
        }
    }
}
