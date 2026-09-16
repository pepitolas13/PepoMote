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

final class PairOnlineTests: XCTestCase {
    private let salon = Pairing(host: "192.168.1.5", port: 26761, token: "tok-salon", pcName: "SALÓN-PC")
    private let cuarto = Pairing(host: "192.168.1.9", port: 26800, token: "tok-cuarto", pcName: "Cuarto")

    func testElPcDelEnlaceVivoEstaOnlineSinEsperarAlSondeo() {
        XCTAssertTrue(PairList.isOnline(salon, [], linkedToken: "tok-salon"), "sesión abierta con él: en verde con la lista aún vacía")
        XCTAssertFalse(PairList.isOnline(cuarto, [], linkedToken: "tok-salon"), "el otro PC sigue esperando al sondeo")
        XCTAssertFalse(PairList.isOnline(salon, [], linkedToken: nil), "sin enlace, solo el sondeo")
        XCTAssertTrue(PairList.isOnline(cuarto, [ReceiverInfo(name: "Cuarto", host: "10.9.9.9", tcpPort: 26761)], linkedToken: "tok-salon"))
    }

    func testElTokenDelEnlaceEsElActualSoloSiSuNombreEsElDeLaSesion() {
        let list = [salon, cuarto]
        XCTAssertEqual(PairList.linkedToken(list, current: "tok-salon", connectedName: "SALÓN-PC"), "tok-salon", "sesión con el actual")
        XCTAssertNil(PairList.linkedToken(list, current: "tok-salon", connectedName: nil), "sin sesión")
        // Olvidado SALÓN con su sesión aún abierta, el actual pasa a ser Cuarto: sin sesión
        XCTAssertNil(PairList.linkedToken([cuarto], current: "tok-cuarto", connectedName: "SALÓN-PC"), "el actual no es el PC de la sesión")
        XCTAssertNil(PairList.linkedToken(list, current: nil, connectedName: "SALÓN-PC"), "sin PC actual")
    }

    func testMergeAnadeYActualizaSinQuitarNada() {
        let shown = [ReceiverInfo(name: "SALÓN-PC", host: "192.168.1.5", tcpPort: 26761), ReceiverInfo(name: "Viejo", host: "192.168.1.7", tcpPort: 26761)]
        let seen = [ReceiverInfo(name: "SALON", host: "192.168.1.5", tcpPort: 26800), ReceiverInfo(name: "Nuevo", host: "192.168.1.8", tcpPort: 26761)]
        let merged = PairList.merge(shown, seen)
        XCTAssertEqual(merged.map(\.host), ["192.168.1.5", "192.168.1.7", "192.168.1.8"], "orden: lo enseñado y luego lo nuevo")
        XCTAssertEqual(merged[0], ReceiverInfo(name: "SALON", host: "192.168.1.5", tcpPort: 26800), "misma IP: se actualiza")
        XCTAssertEqual(merged[1].name, "Viejo", "un sondeo a medias no quita nada")
        XCTAssertEqual(PairList.merge(shown, []), shown)
        XCTAssertEqual(PairList.merge([], seen), seen)
    }
}

final class DiscoveryTests: XCTestCase {
    func testFoundAvisaDeCadaIpNuevaEnLaColaPrincipalYSeCierra() {
        let a = ReceiverInfo(name: "A", host: "192.168.1.5", tcpPort: 26761)
        let aAgain = ReceiverInfo(name: "A otra vez", host: "192.168.1.5", tcpPort: 26761)
        let b = ReceiverInfo(name: "B", host: "192.168.1.6", tcpPort: 26761)
        let twice = expectation(description: "dos avisos, uno por IP nueva")
        var snapshots: [[ReceiverInfo]] = []
        var onMain: [Bool] = []
        let found = Discovery.Found { list in
            snapshots.append(list)
            onMain.append(Thread.isMainThread)
            if snapshots.count == 2 { twice.fulfill() }
        }
        found.add(a)
        found.add(aAgain)
        found.add(b)
        wait(for: [twice], timeout: 2)
        XCTAssertEqual(snapshots, [[a], [a, b]], "la misma IP no vuelve a avisar")
        XCTAssertEqual(onMain, [true, true])
        XCTAssertEqual(found.close(), [a, b])
        found.add(ReceiverInfo(name: "Tarde", host: "192.168.1.7", tcpPort: 26761))
        XCTAssertEqual(found.close(), [a, b], "cerrado el sondeo, lo que llega tarde se ignora")
    }

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
