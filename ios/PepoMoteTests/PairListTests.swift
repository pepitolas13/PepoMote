import Network
import XCTest
@testable import PepoMote

final class PairListTests: XCTestCase {
    private let salon = Pairing(host: "192.168.1.5", port: 26761, token: "tok-salon", pcName: "SALÓN-PC")
    private let cuarto = Pairing(host: "192.168.1.9", port: 26800, token: "tok-cuarto", pcName: "Cuarto & Co = raro")

    func testIdaYVueltaConCaracteresRaros() {
        let list = [salon, cuarto, Pairing(host: "10.0.0.2", port: 26761, token: "a=b&c", pcName: "ñandú")]
        XCTAssertEqual(PairList.decode(PairList.encode(list)), list)
        XCTAssertEqual(PairList.encode(list).split(separator: "\n").count, 3, "una línea por PC")
        XCTAssertTrue(PairList.decode("").isEmpty)
        XCTAssertTrue(PairList.decode(nil).isEmpty)
        XCTAssertTrue(PairList.decode("basura sin campos").isEmpty, "una línea rota se ignora")
        XCTAssertEqual(PairList.decode(PairList.encode([salon]) + "\n\nhost=1.2.3.4&port=1"), [salon], "sin token no es un PC")
    }

    func testUpsertPorTokenYForget() {
        let one = PairList.upsert([], salon)
        let two = PairList.upsert(one, cuarto)
        XCTAssertEqual(two, [salon, cuarto])
        var moved = salon
        moved.pcName = "SALON"
        moved.host = "192.168.1.50"
        let renamed = PairList.upsert(two, moved)
        XCTAssertEqual(renamed.count, 2, "mismo token: se actualiza, no se duplica")
        XCTAssertEqual(renamed[0].pcName, "SALON")
        XCTAssertEqual(renamed[0].host, "192.168.1.50")
        XCTAssertEqual(PairList.forget(two, "tok-salon"), [cuarto])
        XCTAssertEqual(PairList.forget(two, "no-existe"), two)
    }

    func testElActualTrasOlvidar() {
        let two = [salon, cuarto]
        XCTAssertEqual(PairList.nextCurrent(PairList.forget(two, "tok-salon"), "tok-salon", "tok-salon"), "tok-cuarto")
        XCTAssertEqual(PairList.nextCurrent(PairList.forget(two, "tok-cuarto"), "tok-salon", "tok-cuarto"), "tok-salon")
        XCTAssertNil(PairList.nextCurrent([], "tok-salon", "tok-salon"))
        XCTAssertEqual(PairList.nextCurrent(two, nil, "x"), "tok-salon")
    }

    func testEnLaRedYDesconocidos() {
        let seen = [
            ReceiverInfo(name: "SALÓN-PC", host: "192.168.1.5", tcpPort: 26761),
            ReceiverInfo(name: "Otro", host: "192.168.1.9", tcpPort: 26800),
            ReceiverInfo(name: "Nuevo", host: "192.168.1.77", tcpPort: 26761),
        ]
        XCTAssertTrue(PairList.isOnline(salon, seen), "por nombre")
        XCTAssertTrue(PairList.isOnline(cuarto, seen), "por IP aunque se llame distinto")
        XCTAssertFalse(PairList.isOnline(Pairing(host: "1.1.1.1", port: 1, token: "t", pcName: "Nadie"), seen))
        XCTAssertEqual(PairList.unknown(seen, [salon, cuarto]), [seen[2]])
    }

    func testRecolocarPorNombreSinPisarOtroPc() {
        let moved = [ReceiverInfo(name: "SALÓN-PC", host: "192.168.1.60", tcpPort: 26761)]
        XCTAssertEqual(PairList.relocateTarget(salon, [salon, cuarto], moved).host, "192.168.1.60")
        XCTAssertEqual(PairList.relocateTarget(salon, [salon], [ReceiverInfo(name: "SALÓN-PC", host: "192.168.1.5", tcpPort: 26761)]), salon, "mismo sitio: nada")
        let clash = [ReceiverInfo(name: "SALÓN-PC", host: "192.168.1.9", tcpPort: 26800)]
        XCTAssertEqual(PairList.relocateTarget(salon, [salon, cuarto], clash), salon)
        let refreshed = PairList.refreshFrom([salon, cuarto], moved)
        XCTAssertEqual(refreshed[0].host, "192.168.1.60")
        XCTAssertEqual(refreshed[1], cuarto)
    }

    func testParsePairUrl() {
        let p = PairStore.parsePairUrl("pepomote://pair?v=1&host=192.168.1.5&port=26761&t=0123456789abcdef0123456789abcdef&name=SAL%C3%93N-PC")
        XCTAssertEqual(p, Pairing(host: "192.168.1.5", port: 26761, token: "0123456789abcdef0123456789abcdef", pcName: "SALÓN-PC"))
        XCTAssertEqual(PairStore.parsePairUrl("pepomote://pair?v=1&host=10.0.0.2&t=abc")?.port, 26761, "puerto por defecto")
        XCTAssertEqual(PairStore.parsePairUrl("pepomote://pair?v=1&host=10.0.0.2&t=abc")?.pcName, "PC", "nombre por defecto")
        XCTAssertNil(PairStore.parsePairUrl("pepomote://pair?v=2&host=10.0.0.2&t=abc"), "otra versión")
        XCTAssertNil(PairStore.parsePairUrl("pepomote://pair?v=1&host=10.0.0.2"), "sin token")
        XCTAssertNil(PairStore.parsePairUrl("https://example.com/pair?v=1&host=1&t=2"), "otro esquema")
        XCTAssertNil(PairStore.parsePairUrl("basura"))
    }

    func testPairStoreGuardaYSelecciona() {
        // Estado limpio en los UserDefaults del bundle de tests
        UserDefaults.standard.removeObject(forKey: "pairing.list")
        UserDefaults.standard.removeObject(forKey: "pairing.current")
        XCTAssertNil(PairStore.current())
        PairStore.save(salon)
        PairStore.save(cuarto)
        XCTAssertEqual(PairStore.all(), [salon, cuarto])
        XCTAssertEqual(PairStore.current(), cuarto, "el último guardado es el actual")
        PairStore.select("tok-salon")
        XCTAssertEqual(PairStore.current(), salon)
        PairStore.forget("tok-salon")
        XCTAssertEqual(PairStore.all(), [cuarto])
        XCTAssertEqual(PairStore.current(), cuarto)
        PairStore.forget("tok-cuarto")
        XCTAssertNil(PairStore.current())
    }
}

final class DiscoveryTests: XCTestCase {
    func testParseHere() {
        let r = Discovery.parseHere("PMPHERE1 {\"pv\":1,\"name\":\"SALON\",\"tcp\":26761}", from: "192.168.1.5")
        XCTAssertEqual(r, ReceiverInfo(name: "SALON", host: "192.168.1.5", tcpPort: 26761))
        XCTAssertNil(Discovery.parseHere("PMPHERE1 {\"pv\":2,\"name\":\"X\"}", from: "1.2.3.4"), "otra versión")
        XCTAssertNil(Discovery.parseHere("hola", from: "1.2.3.4"))
        XCTAssertEqual(Discovery.parseHere("PMPHERE1 {\"pv\":1}", from: "1.2.3.4")?.name, "1.2.3.4", "sin nombre: la IP")
        XCTAssertEqual(Discovery.parseHere("PMPHERE1 {\"pv\":1}", from: "1.2.3.4")?.tcpPort, 26761)
    }

    func testHostString() {
        XCTAssertEqual(Discovery.hostString(.ipv4(IPv4Address("192.168.1.5")!)), "192.168.1.5")
        XCTAssertEqual(Discovery.hostString(.name("salon.local", nil)), "salon.local")
        XCTAssertFalse(Discovery.hostString(.ipv6(IPv6Address("fe80::1%en0")!)).contains("%"), "sin sufijo de interfaz")
    }
}
