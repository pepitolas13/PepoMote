import XCTest
@testable import PepoMote

/// Los textos de la cabecera plegable del GamePad: lo que dice la pastilla
/// (el modo que se quiere, o el estado del enlace) y la línea de estado de la
/// tarjeta, que ya no se recorta por ancho porque la tarjeta tiene sitio.
/// Mismos vectores que Android (`GamePadHeaderTextTest`).
final class GamePadHeaderTextTests: XCTestCase {

    func testConectadoLaPastillaDiceElModo() {
        XCTAssertEqual(
            "Wii U",
            GamePadHeaderText.handle(connected: true, connecting: false, mode: "Wii U", connectingText: "Conectando…", disconnectedText: "Sin conexión")
        )
        XCTAssertEqual(
            "Switch",
            GamePadHeaderText.handle(connected: true, connecting: false, mode: "Switch", connectingText: "Conectando…", disconnectedText: "Sin conexión")
        )
    }

    func testSinEnlaceLaPastillaDiceElEstado() {
        XCTAssertEqual(
            "Conectando…",
            GamePadHeaderText.handle(connected: false, connecting: true, mode: "Wii U", connectingText: "Conectando…", disconnectedText: "Sin conexión")
        )
        XCTAssertEqual(
            "Sin conexión",
            GamePadHeaderText.handle(connected: false, connecting: false, mode: "Wii U", connectingText: "Conectando…", disconnectedText: "Sin conexión")
        )
    }

    func testConElModoConfirmadoElEstadoLlevaMandoYRitmo() {
        XCTAssertEqual(
            "J1 · GamePad · 23 ms",
            GamePadHeaderText.status(operative: true, player: 1, padName: "GamePad", rttMs: 23.4, activating: "Activando Wii U…")
        )
        XCTAssertEqual(
            "J2 · Pro Controller · 8 ms",
            GamePadHeaderText.status(operative: true, player: 2, padName: "Pro Controller", rttMs: 7.6, activating: "Activando Switch…")
        )
    }

    func testSinIdaYVueltaConocidoNoSeInventaNada() {
        XCTAssertEqual(
            "J1 · GamePad",
            GamePadHeaderText.status(operative: true, player: 1, padName: "GamePad", rttMs: nil, activating: "Activando Wii U…")
        )
    }

    func testMientrasSeEsperaElEcoSaleElAviso() {
        XCTAssertEqual(
            "Activando Wii U…",
            GamePadHeaderText.status(operative: false, player: 1, padName: "GamePad", rttMs: 12, activating: "Activando Wii U…")
        )
        XCTAssertEqual(
            "Activando Switch…",
            GamePadHeaderText.status(operative: false, player: 1, padName: "Pro Controller", rttMs: nil, activating: "Activando Switch…")
        )
    }
}
