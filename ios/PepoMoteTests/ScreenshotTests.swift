import SwiftUI
import XCTest
@testable import PepoMote

/// Capturas de todas las pantallas a PNG, en varios tamaños de iPhone y de
/// iPad, para mirarlas sin dispositivo (la CI las sube como artefacto
/// `ios-shots`). Solo si `PEPOMOTE_SHOTS_DIR` está definido; sin iOS 16
/// (ImageRenderer) se omite.
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
        ("ipad-mini-land", CGSize(width: 1133, height: 744)),
        ("ipad-11-land", CGSize(width: 1180, height: 820)),
        ("ipad-13-land", CGSize(width: 1376, height: 1032)),
    ]

    private var dir: URL?

    override func setUpWithError() throws {
        guard let d = ProcessInfo.processInfo.environment["PEPOMOTE_SHOTS_DIR"], !d.isEmpty else {
            throw XCTSkip("sin PEPOMOTE_SHOTS_DIR: no se guardan capturas")
        }
        guard #available(iOS 16.0, *) else { throw XCTSkip("ImageRenderer necesita iOS 16") }
        let url = URL(fileURLWithPath: d, isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        dir = url
    }

    override func tearDown() {
        LinkState.shared.publish(.disconnected)
        ScreenLink.shared.release()
    }

    private func connected(mode: String, pad: String = LinkState.padGamepad, role: String = LinkState.roleWiimote, slot: Int = 0) -> UiLink {
        .connected(ConnectedLink(pcName: "SALON", mode: mode, rttMs: 12, sensorHz: 100, slot: slot, role: role, player: slot + 1, supportsCemu: true, pad: pad))
    }

    @available(iOS 16.0, *)
    private func shoot<V: View>(_ view: V, _ name: String, _ size: CGSize) throws {
        guard let dir else { return }
        let content = view
            .environmentObject(AppModel.shared)
            .environmentObject(L10n.shared)
            .frame(width: size.width, height: size.height)
            .background(Pepo.background)
        let renderer = ImageRenderer(content: content)
        renderer.scale = 2
        renderer.proposedSize = ProposedViewSize(size)
        guard let png = renderer.uiImage?.pngData() else {
            XCTFail("\(name): sin imagen")
            return
        }
        XCTAssertGreaterThan(png.count, 10_000, "\(name): la captura parece vacía")
        try png.write(to: dir.appendingPathComponent("\(name).png"))
    }

    func testPantallasVerticales() throws {
        guard #available(iOS 16.0, *) else { return }
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
        guard #available(iOS 16.0, *) else { return }
        for (dev, size) in Self.landscape {
            LinkState.shared.publish(connected(mode: LinkState.modePointer))
            try shoot(ControllerLandscapeScreen(showChips: true, onDisconnect: {}), "landscape-pointer-\(dev)", size)
            LinkState.shared.publish(connected(mode: LinkState.modeCemu))
            try shoot(GamePadScreen(onDisconnect: {}), "gamepad-\(dev)", size)
            LinkState.shared.publish(connected(mode: LinkState.modeCemu, pad: LinkState.padPro, slot: 1))
            try shoot(GamePadScreen(onDisconnect: {}), "gamepad-pro-\(dev)", size)
            LinkState.shared.publish(connected(mode: LinkState.modeDolphin, pad: "nunchuk", role: LinkState.roleNunchuk, slot: 1))
            try shoot(NunchukScreen(onDisconnect: {}), "nunchuk-\(dev)", size)
        }
    }
}
