import XCTest
@testable import PepoMote

/// Mismos vectores que desktop/src/update.rs y Android (UpdateCheckTest).
final class UpdateCheckTests: XCTestCase {
    private func v(_ a: Int, _ b: Int, _ c: Int) -> AppVersion { AppVersion(major: a, minor: b, patch: c) }

    func testParseaEtiquetasConYSinV() {
        XCTAssertEqual(AppVersion.parse("v1.6.0"), v(1, 6, 0))
        XCTAssertEqual(AppVersion.parse("1.6.0"), v(1, 6, 0))
        XCTAssertEqual(AppVersion.parse(" V1.10.2 "), v(1, 10, 2))
        XCTAssertEqual(AppVersion.parse("v2.0"), v(2, 0, 0))
        for bad: String? in ["v1.6.0-beta", "abc", "", "v", "1", "1.2.3.4", "1..2", "1.x.0", nil] {
            XCTAssertNil(AppVersion.parse(bad), bad ?? "nil")
        }
    }

    func testComparaNumericamenteNoPorTexto() {
        XCTAssertTrue(v(1, 10, 0) > v(1, 9, 9))
        XCTAssertTrue(v(2, 0, 0) > v(1, 99, 99))
        XCTAssertTrue(v(1, 6, 1) > v(1, 6, 0))
        XCTAssertEqual(v(1, 6, 0), v(1, 6, 0))
        XCTAssertEqual(v(1, 6, 0).description, "1.6.0")
    }

    func testDueRespetaActivadoYUnaHora() {
        let day: TimeInterval = 3600
        XCTAssertTrue(UpdateCheck.due(enabled: true, last: 0, now: 1), "nunca consultado: ya")
        XCTAssertFalse(UpdateCheck.due(enabled: false, last: 0, now: 1_000_000_000))
        XCTAssertFalse(UpdateCheck.due(enabled: true, last: 1000, now: 1000 + day - 1))
        XCTAssertTrue(UpdateCheck.due(enabled: true, last: 1000, now: 1000 + day))
        // El reloj hacia atrás no deja bloqueadas futuras comprobaciones.
        XCTAssertTrue(UpdateCheck.due(enabled: true, last: 5000, now: 4000))
    }

    func testLaUrlDeLaReleaseYLaVersionActual() {
        XCTAssertEqual(UpdateCheck.releaseURL(v(1, 6, 0)).absoluteString, "https://github.com/pepitolas13/PepoMote/releases/tag/v1.6.0")
        // la app anfitriona de los tests lleva su versión en el Info.plist
        XCTAssertNotNil(UpdateCheck.current)
    }
}
