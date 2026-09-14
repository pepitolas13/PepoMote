import XCTest
@testable import PepoMote

/// Lado del mando + Nunchuk apaisado: preferencia, máscara y lado visible (mismos casos que Android).
final class LandscapeSideTests: XCTestCase {
    func testPreferenciaIdaYVuelta() {
        for s in [LandscapeSide.unset, .sensor, .left, .right] {
            XCTAssertEqual(LandscapeSide.fromPref(s.rawValue), s)
        }
        XCTAssertEqual(LandscapeSide.fromPref(""), .unset)
        XCTAssertEqual(LandscapeSide.fromPref("lo que sea"), .unset)
        XCTAssertEqual(LandscapeSide.fromPref("left"), .left)
        XCTAssertEqual(LandscapeSide.fromPref("sensor"), .sensor)
    }

    func testMascaraYVuelta() {
        // left = borde superior del móvil a la izquierda = landscapeRight de UIKit
        XCTAssertEqual(LandscapeSide.left.mask, .landscapeRight)
        XCTAssertEqual(LandscapeSide.right.mask, .landscapeLeft)
        // Sin elegir, y con «según el sensor»: los dos apaisados, como siempre
        XCTAssertEqual(LandscapeSide.unset.mask, .landscape)
        XCTAssertEqual(LandscapeSide.sensor.mask, .landscape)
        XCTAssertEqual(LandscapeSide.left.flipped, .right)
        XCTAssertEqual(LandscapeSide.right.flipped, .left)
        XCTAssertEqual(LandscapeSide.unset.flipped, .unset)
        XCTAssertEqual(LandscapeSide.sensor.flipped, .sensor)
    }

    func testLadoVisibleSegunLaOrientacion() {
        XCTAssertEqual(LandscapeSide.current(.landscapeRight), .left)
        XCTAssertEqual(LandscapeSide.current(.landscapeLeft), .right)
        XCTAssertEqual(LandscapeSide.current(.portrait), .left)
        // Coherente con el remapeo de los sensores: landscapeRight = ROTATION_90 = borde superior a la izquierda
        XCTAssertEqual(OrientationLock.frameRotation(.landscapeRight), Frame.rotation90)
    }

    func testElProvisionalManda() {
        XCTAssertEqual(LandscapeSide.effective(saved: .unset, provisional: nil), .unset)
        XCTAssertEqual(LandscapeSide.effective(saved: .unset, provisional: .right), .right)
        XCTAssertEqual(LandscapeSide.effective(saved: .left, provisional: nil), .left)
        XCTAssertEqual(LandscapeSide.effective(saved: .left, provisional: .right), .right)
    }
}
