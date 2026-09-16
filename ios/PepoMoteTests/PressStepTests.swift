import XCTest
@testable import PepoMote

/// Tabla de verdad de los dos ajustes de pulsación (mismos casos que Android
/// `PressStepTest` y que `on_move` en Linux).
final class PressStepTests: XCTestCase {
    private let a = Btn.a
    private let b = Btn.b

    func testDeslizandoElDedoCogeLoQueTieneDebajo() {
        // Un dedo libre pulsa al entrar en un botón
        XCTAssertEqual(PressStep.next(slide: true, sticky: true, held: nil, over: a), .to(a))
        XCTAssertEqual(PressStep.next(slide: true, sticky: false, held: nil, over: a), .to(a))
        // Deslizar a otro suelta el primero y pulsa el nuevo
        XCTAssertEqual(PressStep.next(slide: true, sticky: true, held: a, over: b), .to(b))
        // Salirse al vacío suelta y deja el dedo libre (puede volver a coger)
        XCTAssertEqual(PressStep.next(slide: true, sticky: true, held: a, over: nil), .to(nil))
        // Quieto sobre el mismo botón, nada que hacer
        XCTAssertEqual(PressStep.next(slide: true, sticky: true, held: a, over: a), .keep)
        XCTAssertEqual(PressStep.next(slide: true, sticky: false, held: a, over: a), .keep)
        XCTAssertEqual(PressStep.next(slide: true, sticky: true, held: nil, over: nil), .keep)
    }

    func testConMantenerNadaSeSuelta() {
        XCTAssertEqual(PressStep.next(slide: false, sticky: true, held: a, over: nil), .keep, "salirse no suelta")
        XCTAssertEqual(PressStep.next(slide: false, sticky: true, held: a, over: b), .keep, "ni entrar en otro botón")
        XCTAssertEqual(PressStep.next(slide: false, sticky: true, held: a, over: a), .keep)
        XCTAssertEqual(PressStep.next(slide: false, sticky: true, held: nil, over: a), .keep, "un dedo libre no coge nada")
        XCTAssertEqual(PressStep.next(slide: false, sticky: true, held: nil, over: nil), .keep)
    }

    func testSinMantenerSalirseSueltaYElDedoYaNoVuelve() {
        XCTAssertEqual(PressStep.next(slide: false, sticky: false, held: a, over: nil), .to(nil))
        XCTAssertEqual(PressStep.next(slide: false, sticky: false, held: a, over: b), .to(nil), "el de al lado tampoco se pulsa")
        XCTAssertEqual(PressStep.next(slide: false, sticky: false, held: a, over: a), .keep, "dentro del botón, sigue pulsado")
        // Ya suelto: ese dedo no coge nada hasta levantarlo
        XCTAssertEqual(PressStep.next(slide: false, sticky: false, held: nil, over: a), .keep)
        XCTAssertEqual(PressStep.next(slide: false, sticky: false, held: nil, over: nil), .keep)
    }
}
