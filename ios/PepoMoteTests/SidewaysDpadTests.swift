import XCTest
@testable import PepoMote

/// La cruceta de lado manda los botones de un mando girado con el IR a la izquierda (mismos casos que Android).
final class SidewaysDpadTests: XCTestCase {
    func testDeLado() {
        XCTAssertEqual(Btn.sideways(Btn.dpadUp), Btn.dpadRight)
        XCTAssertEqual(Btn.sideways(Btn.dpadDown), Btn.dpadLeft)
        XCTAssertEqual(Btn.sideways(Btn.dpadLeft), Btn.dpadUp)
        XCTAssertEqual(Btn.sideways(Btn.dpadRight), Btn.dpadDown)
        // los cuatro brazos siguen siendo los cuatro botones, sin repetir
        let bits: Set<UInt32> = [Btn.sideways(Btn.dpadUp), Btn.sideways(Btn.dpadDown), Btn.sideways(Btn.dpadLeft), Btn.sideways(Btn.dpadRight)]
        XCTAssertEqual(bits, [Btn.dpadUp, Btn.dpadDown, Btn.dpadLeft, Btn.dpadRight])
        // otros botones no se tocan
        XCTAssertEqual(Btn.sideways(Btn.a), Btn.a)
        XCTAssertEqual(Btn.sideways(Btn.one), Btn.one)
    }
}
