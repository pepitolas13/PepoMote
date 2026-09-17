import XCTest
@testable import PepoMote

/// La tabla de plantillas de consola es una copia a mano de `pmp::retro`: se
/// compara campo a campo con `protocol/retro-layouts.json` (generado desde
/// Rust y copiado al bundle de tests), bits incluidos, y sus invariantes.
final class RetroLayoutsTests: XCTestCase {
    private func json() throws -> [String: Any] {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "retro-layouts", withExtension: "json"), "retro-layouts.json en el bundle de tests")
        let data = try Data(contentsOf: url)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func testTheTableMatchesTheRustJson() throws {
        let j = try json()
        XCTAssertEqual(j["version"] as? Int, 1)
        let bits = try XCTUnwrap(j["bits"] as? [String: Int])
        for (name, value) in RetroLayouts.bitNames {
            XCTAssertEqual(bits[name], Int(value), "bit \(name)")
        }
        XCTAssertEqual(bits.count, RetroLayouts.bitNames.count)
        XCTAssertEqual(j["console_ids"] as? [String], RetroLayouts.consoleIds)
        let layouts = try XCTUnwrap(j["layouts"] as? [[String: Any]])
        XCTAssertEqual(layouts.count, RetroLayouts.all.count, "mismas plantillas, mismo orden")
        for (l, k) in zip(layouts, RetroLayouts.all) {
            let id = l["id"] as? String ?? "?"
            XCTAssertEqual(id, k.id)
            XCTAssertEqual(l["name"] as? String, k.name, id)
            XCTAssertEqual(l["shape"] as? String, k.shape.rawValue, id)
            XCTAssertEqual(l["stagger"] as? Int, k.stagger, id)
            let face = try XCTUnwrap(l["face"] as? [[String: Any]], id)
            XCTAssertEqual(face.count, k.face.count, id)
            for (fj, fk) in zip(face, k.face) {
                XCTAssertEqual(fj["slot"] as? String, fk.slot, id)
                XCTAssertEqual(fj["label"] as? String, fk.label, "\(id)/\(fk.slot)")
                XCTAssertEqual(fj["bit"] as? String, RetroLayouts.bitName(fk.bit), "\(id)/\(fk.slot) bit")
                XCTAssertEqual(fj["color"] as? String, fk.color.rawValue, "\(id)/\(fk.slot) color")
                XCTAssertEqual(fj["primary"] as? Bool, fk.primary, "\(id)/\(fk.slot) primary")
                XCTAssertEqual(fj["size"] as? Double ?? -1, fk.size, accuracy: 1e-6, "\(id)/\(fk.slot) size")
            }
            XCTAssertEqual(l["left_stick"] as? Bool, k.leftStick, id)
            XCTAssertEqual(l["right_stick"] as? String, k.rightStick.rawValue, id)
            XCTAssertEqual(l["stick_clicks"] as? Bool, k.stickClicks, id)
            let sh = try XCTUnwrap(l["shoulders"] as? [String: Any], id)
            XCTAssertEqual(sh["l"] as? String, k.shoulders.l, id)
            XCTAssertEqual(sh["r"] as? String, k.shoulders.r, id)
            XCTAssertEqual(sh["l2"] as? String, k.shoulders.l2, id)
            XCTAssertEqual(sh["r2"] as? String, k.shoulders.r2, id)
            let center = try XCTUnwrap(l["center"] as? [[String: Any]], id)
            XCTAssertEqual(center.count, k.center.count, id)
            for (cj, ck) in zip(center, k.center) {
                XCTAssertEqual(cj["label"] as? String, ck.label, id)
                XCTAssertEqual(cj["bit"] as? String, RetroLayouts.bitName(ck.bit), id)
            }
        }
    }

    func testEveryLayoutIsConsistent() {
        let ids = RetroLayouts.all.map(\.id)
        XCTAssertEqual(Set(ids).count, ids.count, "ids únicos")
        XCTAssertEqual(RetroLayouts.all.first?.id, "retropad")
        XCTAssertTrue(RetroLayouts.consoleIds.allSatisfy { RetroLayouts.byId($0) != nil })
        XCTAssertFalse(RetroLayouts.isConsole("retropad") || RetroLayouts.isConsole("md3"))
        for l in RetroLayouts.all {
            XCTAssertEqual(l.shape.slots.count, l.face.count, l.id)
            XCTAssertEqual(Set(l.shape.slots), Set(l.face.map(\.slot)), l.id)
            XCTAssertEqual(Set(l.face.map(\.bit)).count, l.face.count, "\(l.id): bits repetidos")
            XCTAssertLessThanOrEqual(l.face.filter(\.primary).count, 1, "\(l.id): más de un primario")
            XCTAssertTrue(!l.stickClicks || (l.leftStick && l.rightStick == .analog), "\(l.id): clics sin sticks")
            XCTAssertTrue(l.shape == .two || l.stagger == 0, "\(l.id): stagger solo en dos")
            XCTAssertTrue((1...2).contains(l.center.count) && l.center.allSatisfy { $0.bit == Btn.plus || $0.bit == Btn.minus }, "\(l.id): centro")
            XCTAssertTrue(l.face.allSatisfy { RetroLayouts.bitName($0.bit) != nil }, l.id)
        }
    }
}
