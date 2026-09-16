import CoreGraphics
import XCTest
@testable import PepoMote

/// Registro de zonas: qué botón hay bajo el dedo (círculo de los redondos,
/// solapes, vacío) y que al salir de la pantalla no quede nada pulsado.
final class PressRegistryTests: XCTestCase {
    private var reg = PressRegistry()
    private let aId = UUID()
    private let bId = UUID()
    private let zId = UUID()

    override func setUp() {
        super.setUp()
        ButtonState.shared.reset() // sin nada retenido del latch de otra prueba
        reg = PressRegistry()
        // A: redondo de 100 pt centrado en (100, 100)
        reg.place(aId, PressZone(bit: Btn.a, rect: CGRect(x: 50, y: 50, width: 100, height: 100), circular: true, pop: true))
        // B: redondo de 100 pt pegado a su derecha, centrado en (200, 100)
        reg.place(bId, PressZone(bit: Btn.b, rect: CGRect(x: 150, y: 50, width: 100, height: 100), circular: true, pop: false))
        // Z: pastilla rectangular ancha, abajo
        reg.place(zId, PressZone(bit: Btn.z, rect: CGRect(x: 0, y: 200, width: 300, height: 60), circular: false, pop: false))
    }

    override func tearDown() {
        reg.releaseAll()
        ButtonState.shared.reset()
        super.tearDown()
    }

    func testCadaDedoEncuentraSuBoton() {
        XCTAssertEqual(reg.resolve(CGPoint(x: 100, y: 100)), aId)
        XCTAssertEqual(reg.resolve(CGPoint(x: 200, y: 100)), bId)
        XCTAssertEqual(reg.resolve(CGPoint(x: 10, y: 230)), zId)
        XCTAssertEqual(reg.bit(aId), Btn.a)
        XCTAssertEqual(reg.bit(zId), Btn.z)
    }

    func testLaEsquinaDeUnBotonRedondoNoEsDeNadie() {
        // Dentro de su cuadrado pero fuera del círculo
        XCTAssertNil(reg.resolve(CGPoint(x: 55, y: 55)))
        XCTAssertNil(reg.resolve(CGPoint(x: 145, y: 145)))
        // Justo dentro del círculo, arriba del todo
        XCTAssertEqual(reg.resolve(CGPoint(x: 100, y: 55)), aId)
    }

    func testEnUnSolapeGanaElCentroMasCercano() {
        // Zona ancha que cubre los dos botones redondos
        let wide = UUID()
        reg.place(wide, PressZone(bit: Btn.one, rect: CGRect(x: 0, y: 0, width: 300, height: 200), circular: false, pop: false))
        XCTAssertEqual(reg.resolve(CGPoint(x: 100, y: 100)), aId, "el centro de A está al lado")
        XCTAssertEqual(reg.resolve(CGPoint(x: 200, y: 100)), bId)
        XCTAssertEqual(reg.resolve(CGPoint(x: 280, y: 10)), wide, "donde no llega ningún botón, la zona ancha")
    }

    func testElDedoEnElVacioNoPulsaNada() {
        XCTAssertNil(reg.resolve(CGPoint(x: 400, y: 400)))
        XCTAssertNil(reg.resolve(CGPoint(x: 100, y: 180)), "entre los botones y la pastilla")
        XCTAssertNil(reg.bit(nil))
    }

    func testSoltarTodoAlSalirDeLaPantalla() {
        let dedo = UUID()
        reg.press(aId, by: dedo)
        reg.press(zId, by: dedo)
        XCTAssertEqual(reg.pressed, [aId, zId])
        XCTAssertNotEqual(ButtonState.shared.current() & Btn.a, 0, "el flanco de bajada sale al instante")
        reg.release(aId, by: dedo)
        XCTAssertEqual(reg.pressed, [zId])
        reg.releaseAll()
        XCTAssertTrue(reg.pressed.isEmpty)
        // Un botón que se va de la pantalla pulsado tampoco se queda pulsado
        reg.press(bId, by: dedo)
        reg.remove(bId)
        XCTAssertTrue(reg.pressed.isEmpty)
        XCTAssertNil(reg.resolve(CGPoint(x: 200, y: 100)))
    }

    /// Mismo contrato que `SlideTrackerTest.unBotonQueLlevaOtroDedoNoSeLeQuita`
    /// en Android: la zona es del primer dedo hasta que la suelta.
    func testUnBotonQueLlevaOtroDedoNoSeLeQuita() {
        let primero = UUID()
        let segundo = UUID()
        reg.press(aId, by: primero)
        XCTAssertFalse(reg.free(aId, for: segundo), "el segundo dedo no puede cogerla")
        XCTAssertTrue(reg.free(aId, for: primero), "el suyo sí la mantiene")
        XCTAssertTrue(reg.free(bId, for: segundo), "las demás siguen libres")
        // El segundo pasa por encima y se levanta: la A no se suelta
        reg.press(aId, by: segundo)
        reg.release(aId, by: segundo)
        XCTAssertEqual(reg.pressed, [aId], "el primero la sigue apretando")
        reg.release(aId, by: primero)
        XCTAssertTrue(reg.pressed.isEmpty)
        XCTAssertTrue(reg.free(aId, for: segundo), "ya está libre para el siguiente")
    }

    func testConLaPantallaInerteNoSePulsaNada() {
        reg.enabled = false
        reg.press(aId, by: UUID())
        XCTAssertTrue(reg.pressed.isEmpty)
        // Y las zonas siguen ahí para cuando vuelva a estar viva
        XCTAssertEqual(reg.resolve(CGPoint(x: 100, y: 100)), aId)
    }
}
