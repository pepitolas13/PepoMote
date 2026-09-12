import XCTest
@testable import PepoMote

/// es.lproj y en.lproj tienen las mismas claves y los mismos huecos (%1$@…):
/// ninguna pantalla se queda a medias en un idioma.
final class StringsParityTests: XCTestCase {
    private func strings(_ code: String) throws -> [String: String] {
        let bundle = L10n.bundle(for: code)
        XCTAssertNotEqual(bundle, Bundle.main, "falta \(code).lproj")
        let url = try XCTUnwrap(bundle.url(forResource: "Localizable", withExtension: "strings"), "sin Localizable.strings en \(code)")
        let dict = try XCTUnwrap(NSDictionary(contentsOf: url) as? [String: String])
        return dict
    }

    private func holes(_ s: String) -> [String] {
        let re = try! NSRegularExpression(pattern: "%\\d+\\$[@d]")
        return re.matches(in: s, range: NSRange(s.startIndex..., in: s)).map { String(s[Range($0.range, in: s)!]) }.sorted()
    }

    func testMismasClavesEnLosDosIdiomas() throws {
        let es = try strings("es")
        let en = try strings("en")
        XCTAssertTrue(es.count > 90, "hay textos (\(es.count))")
        XCTAssertEqual(Set(es.keys).subtracting(en.keys), [], "claves que faltan en inglés")
        XCTAssertEqual(Set(en.keys).subtracting(es.keys), [], "claves que sobran en inglés")
        for (k, v) in es {
            XCTAssertEqual(holes(v), holes(en[k] ?? ""), "huecos de \(k)")
            XCTAssertFalse(v.trimmingCharacters(in: .whitespaces).isEmpty, "\(k) vacío")
            XCTAssertFalse((en[k] ?? "").trimmingCharacters(in: .whitespaces).isEmpty, "\(k) vacío en inglés")
        }
    }

    func testElIdiomaSeResuelveYSeFormatea() {
        XCTAssertEqual(L10n.resolve("es"), "es")
        XCTAssertEqual(L10n.resolve("en"), "en")
        XCTAssertTrue(["es", "en"].contains(L10n.resolve("system")))
        let es = L10n.bundle(for: "es")
        let en = L10n.bundle(for: "en")
        XCTAssertEqual(String(format: es.localizedString(forKey: "status_connected_to", value: nil, table: nil), "SALON"), "Conectado a SALON")
        XCTAssertEqual(String(format: en.localizedString(forKey: "nunchuk_line", value: nil, table: nil), 2, "Dolphin"), "Nunchuk · Player 2 · Dolphin")
        XCTAssertEqual(es.localizedString(forKey: "exit", value: nil, table: nil), "Salir")
        XCTAssertEqual(en.localizedString(forKey: "exit", value: nil, table: nil), "Exit")
    }

    /// Las claves que usa el código existen (una clave sin texto se vería tal cual).
    func testClavesUsadasPorElCodigo() throws {
        let es = try strings("es")
        for key in ["exit", "keyboard", "precision", "in_cemu", "gamepad", "pro_controller", "wii_remote", "activating_wiiu",
                    "screen_fps", "tv_pad", "blow", "home_btn", "touch_screen", "screen_connecting", "screen_unavailable",
                    "screen_unavailable_detail", "screen_reconnecting", "kb_title", "kb_help", "kb_placeholder", "kb_delete",
                    "kb_write", "kb_close", "kb_accept", "warn_needs_13", "warn_player_1", "lost_connection", "re_pair_reason",
                    "pc_not_responding", "qr_not_pepomote", "error_prefix", "your_pc", "camera_denied", "local_network_hint",
                    "ios_note", "scan_prompt", "cancel"] {
            XCTAssertNotNil(es[key], "falta \(key)")
        }
    }
}
