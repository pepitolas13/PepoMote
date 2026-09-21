import XCTest
@testable import PepoMote

/// La máquina de la vibración (contrato §2), con los mismos casos que
/// `RumbleTrackTest` en Android y el test de Rust: si esto diverge, un móvil
/// vibra distinto de otro con el mismo receptor.
final class RumbleTrackTests: XCTestCase {
    /// Nivel de un Start/Change (nil si es Stop o no hay orden).
    private func level(_ c: RumbleCommand?) -> Float? {
        if case .start(let l)? = c { return l }
        if case .change(let l)? = c { return l }
        return nil
    }

    private func isStart(_ c: RumbleCommand?) -> Bool {
        if case .start(_)? = c { return true }
        return false
    }

    private func isChange(_ c: RumbleCommand?) -> Bool {
        if case .change(_)? = c { return true }
        return false
    }

    func testLaVibracionEmpiezaCambiaYPara() throws {
        let t = RumbleTrack()
        XCTAssertEqual(t.apply(seq: 1, strong: 255, weak: 0, ttlMs: 400, nowMs: 0), RumbleCommand.start(1))
        let change = t.apply(seq: 2, strong: 128, weak: 0, ttlMs: 400, nowMs: 50)
        XCTAssertTrue(isChange(change))
        XCTAssertEqual(try XCTUnwrap(level(change)), Float(128) / 255, accuracy: 1e-3)
        XCTAssertEqual(t.apply(seq: 3, strong: 0, weak: 0, ttlMs: 0, nowMs: 100), RumbleCommand.stop)
        XCTAssertNil(t.apply(seq: 4, strong: 0, weak: 0, ttlMs: 0, nowMs: 150), "ya parada: nada")
    }

    func testUnSeqViejoODuplicadoSeIgnora() {
        let t = RumbleTrack()
        XCTAssertTrue(isStart(t.apply(seq: 5, strong: 255, weak: 255, ttlMs: 400, nowMs: 0)))
        XCTAssertNil(t.apply(seq: 5, strong: 0, weak: 0, ttlMs: 0, nowMs: 10), "repetido")
        XCTAssertNil(t.apply(seq: 4, strong: 0, weak: 0, ttlMs: 0, nowMs: 20), "viejo")
        XCTAssertNil(t.tick(nowMs: 100), "sigue vibrando")
        XCTAssertEqual(t.apply(seq: 6, strong: 0, weak: 0, ttlMs: 0, nowMs: 30), RumbleCommand.stop)
        // Desbordamiento: 0 es «más nuevo» que 0xFFFFFFFF
        let w = RumbleTrack()
        XCTAssertTrue(isStart(w.apply(seq: 0xFFFF_FFFF, strong: 255, weak: 0, ttlMs: 400, nowMs: 0)))
        XCTAssertEqual(w.apply(seq: 0, strong: 0, weak: 0, ttlMs: 0, nowMs: 1), RumbleCommand.stop)
    }

    func testSinRefrescoLaVibracionCaducaSola() {
        let t = RumbleTrack()
        XCTAssertTrue(isStart(t.apply(seq: 1, strong: 200, weak: 0, ttlMs: 400, nowMs: 1000)))
        XCTAssertNil(t.tick(nowMs: 1399))
        XCTAssertEqual(t.tick(nowMs: 1400), RumbleCommand.stop)
        XCTAssertNil(t.tick(nowMs: 1500), "ya parada: nada")
    }

    func testElRefrescoAlargaLaVibracion() {
        let t = RumbleTrack()
        XCTAssertTrue(isStart(t.apply(seq: 1, strong: 200, weak: 0, ttlMs: 400, nowMs: 1000)))
        XCTAssertNil(t.apply(seq: 2, strong: 200, weak: 0, ttlMs: 400, nowMs: 1300), "el refresco solo alarga")
        XCTAssertNil(t.tick(nowMs: 1400))
        XCTAssertEqual(t.tick(nowMs: 1700), RumbleCommand.stop)
    }

    func testElMismoNivelNoRepiteOrdenes() throws {
        let t = RumbleTrack()
        let start = t.apply(seq: 1, strong: 100, weak: 40, ttlMs: 400, nowMs: 0)
        XCTAssertTrue(isStart(start))
        XCTAssertEqual(try XCTUnwrap(level(start)), Float(100) / 255, accuracy: 1e-3)
        XCTAssertNil(t.apply(seq: 2, strong: 100, weak: 40, ttlMs: 400, nowMs: 10))
        XCTAssertNil(t.apply(seq: 3, strong: 40, weak: 100, ttlMs: 400, nowMs: 20), "el máximo es el mismo")
    }

    func testResetParaSiEstabaVibrando() {
        let t = RumbleTrack()
        XCTAssertNil(t.reset(), "sobre nuevo, nada")
        XCTAssertTrue(isStart(t.apply(seq: 1, strong: 255, weak: 0, ttlMs: 400, nowMs: 0)))
        XCTAssertEqual(t.reset(), RumbleCommand.stop)
        XCTAssertNil(t.reset())
        XCTAssertTrue(isStart(t.apply(seq: 1, strong: 255, weak: 0, ttlMs: 400, nowMs: 0)), "lastSeq borrado")
    }

    func testSinTtlSeUsanCuatrocientosMs() {
        let t = RumbleTrack()
        XCTAssertTrue(isStart(t.apply(seq: 1, strong: 255, weak: 0, ttlMs: 0, nowMs: 0)))
        XCTAssertNil(t.tick(nowMs: 399))
        XCTAssertEqual(t.tick(nowMs: 400), RumbleCommand.stop)
    }

    func testLaEscalaDelAjuste() {
        XCTAssertEqual(RumblePref.scale("high"), 1, accuracy: 1e-3)
        XCTAssertEqual(RumblePref.scale("normal"), 0.65, accuracy: 1e-3)
        XCTAssertEqual(RumblePref.scale("low"), 0.35, accuracy: 1e-3)
        XCTAssertEqual(RumblePref.scale("off"), 0, accuracy: 1e-3)
        XCTAssertEqual(RumblePref.normalize(nil), "normal")
        XCTAssertEqual(RumblePref.normalize("fuerte"), "normal", "desconocido → normal")
        XCTAssertEqual(RumblePref.normalize("low"), "low")
        XCTAssertEqual(RumblePref.all.map { $0.rawValue }, ["high", "normal", "low", "off"])
        // La preferencia: normal de serie, y un valor desconocido guardado se lee como normal
        let previous = UserDefaults.standard.object(forKey: AppPrefs.rumbleKey)
        defer { UserDefaults.standard.set(previous, forKey: AppPrefs.rumbleKey) }
        UserDefaults.standard.removeObject(forKey: AppPrefs.rumbleKey)
        XCTAssertEqual(AppPrefs.rumble, "normal")
        UserDefaults.standard.set("fuerte", forKey: AppPrefs.rumbleKey)
        XCTAssertEqual(AppPrefs.rumble, "normal")
        AppPrefs.rumble = "off"
        XCTAssertEqual(AppPrefs.rumble, "off")
        XCTAssertEqual(RumblePref.scale(AppPrefs.rumble), 0, accuracy: 1e-3)
    }
}
