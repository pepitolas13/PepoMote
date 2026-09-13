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

    func testLaLocationDeGitHubDaLaVersion() {
        let loc = "https://github.com/pepitolas13/PepoMote/releases/tag/v1.5.0"
        XCTAssertEqual(UpdateCheck.version(fromLocation: loc), v(1, 5, 0))
        XCTAssertEqual(UpdateCheck.version(fromLocation: loc + "?x=1"), v(1, 5, 0))
        XCTAssertEqual(UpdateCheck.version(fromLocation: loc + "/"), v(1, 5, 0))
        XCTAssertNil(UpdateCheck.version(fromLocation: "https://github.com/pepitolas13/PepoMote/releases"))
        XCTAssertNil(UpdateCheck.version(fromLocation: ""))
        XCTAssertNil(UpdateCheck.version(fromLocation: nil))
    }

    func testPendingSoloSiEsMayorYNoDescartada() {
        let cur = v(1, 5, 0)
        XCTAssertNil(UpdateCheck.pending(current: cur, latest: nil, dismissed: nil))
        XCTAssertNil(UpdateCheck.pending(current: nil, latest: v(1, 6, 0), dismissed: nil))
        XCTAssertNil(UpdateCheck.pending(current: cur, latest: v(1, 5, 0), dismissed: nil))
        XCTAssertNil(UpdateCheck.pending(current: cur, latest: v(1, 4, 9), dismissed: nil))
        XCTAssertEqual(UpdateCheck.pending(current: cur, latest: v(1, 6, 0), dismissed: nil), v(1, 6, 0))
        XCTAssertNil(UpdateCheck.pending(current: cur, latest: v(1, 6, 0), dismissed: v(1, 6, 0)))
        // se ocultó la 1.6.0, pero la 1.7.0 es otra: se anuncia
        XCTAssertEqual(UpdateCheck.pending(current: cur, latest: v(1, 7, 0), dismissed: v(1, 6, 0)), v(1, 7, 0))
    }

    func testDueRespetaActivadoY24h() {
        let day: TimeInterval = 24 * 3600
        XCTAssertTrue(UpdateCheck.due(enabled: true, last: 0, now: 1), "nunca consultado: ya")
        XCTAssertFalse(UpdateCheck.due(enabled: false, last: 0, now: 1_000_000_000))
        XCTAssertFalse(UpdateCheck.due(enabled: true, last: 1000, now: 1000 + day - 1))
        XCTAssertTrue(UpdateCheck.due(enabled: true, last: 1000, now: 1000 + day))
        // reloj hacia atrás: no dispara
        XCTAssertFalse(UpdateCheck.due(enabled: true, last: 5000, now: 4000))
    }

    func testLaUrlDeLaReleaseYLaVersionActual() {
        XCTAssertEqual(UpdateCheck.releaseURL(v(1, 6, 0)).absoluteString, "https://github.com/pepitolas13/PepoMote/releases/tag/v1.6.0")
        // la app anfitriona de los tests lleva su versión en el Info.plist
        XCTAssertNotNil(UpdateCheck.current)
    }
}
