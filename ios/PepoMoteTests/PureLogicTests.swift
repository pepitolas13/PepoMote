import UIKit
import XCTest
@testable import PepoMote

/// El pulgar en pt (y hacia abajo) → stick −127..127 (y hacia arriba).
final class StickMapTests: XCTestCase {
    private let r: Float = 100

    private func assertStick(_ v: (x: Int, y: Int), _ x: Int, _ y: Int, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(v.x, x, file: file, line: line)
        XCTAssertEqual(v.y, y, file: file, line: line)
    }

    func testCentroYZonaMuertaDanCero() {
        assertStick(StickMap.map(0, 0, radius: r), 0, 0)
        assertStick(StickMap.map(5, 5, radius: r), 0, 0) // |v| = 7 < 8 % de 100
        assertStick(StickMap.map(0, -7.9, radius: r), 0, 0)
    }

    func testEjesLlenosYSignos() {
        assertStick(StickMap.map(r, 0, radius: r), 127, 0) // derecha
        assertStick(StickMap.map(-r, 0, radius: r), -127, 0) // izquierda
        assertStick(StickMap.map(0, -r, radius: r), 0, 127) // dedo arriba = +y
        assertStick(StickMap.map(0, r, radius: r), 0, -127) // dedo abajo = −y
    }

    func testFueraDelRadioSatura() {
        assertStick(StickMap.map(3 * r, 0, radius: r), 127, 0)
        let v = StickMap.map(-300, 400, radius: r) // 3-4-5: dirección conservada
        XCTAssertTrue(abs(v.x + 76) <= 1 && abs(v.y + 102) <= 1, "\(v)")
    }

    func testSinEscalonAlSalirDelCentro() {
        let x = StickMap.map(r * StickMap.deadZone + 0.5, 0, radius: r).x
        XCTAssertTrue((1...2).contains(x), "justo fuera de la zona muerta debe ser casi 0, fue \(x)")
        let mid = StickMap.map(r / 2, 0, radius: r).x
        XCTAssertTrue((55...62).contains(mid), "a mitad de radio debe rondar la mitad, fue \(mid)")
    }

    func testDiagonalNoSuperaElRadio() {
        let v = StickMap.map(r, -r, radius: r)
        XCTAssertTrue((88...92).contains(v.x) && (88...92).contains(v.y), "\(v)") // 127·cos 45°
    }

    func testRedondeoComoKotlin() {
        XCTAssertEqual(StickMap.roundHalfUp(2.5), 3)
        XCTAssertEqual(StickMap.roundHalfUp(-2.5), -2)
        XCTAssertEqual(StickMap.roundHalfUp(2.4), 2)
        XCTAssertEqual(StickMap.roundHalfUp(-2.6), -3)
    }
}

/// «Teclado» del modo Wii U: codificación del mensaje `text` y semántica de sus botones.
final class TextInputTests: XCTestCase {
    func testMensajeText() {
        XCTAssertEqual(TextInput.encode("Link"), "{\"m\":\"text\",\"text\":\"Link\"}")
        XCTAssertEqual(TextInput.encode(""), "{\"m\":\"text\",\"text\":\"\"}")
    }

    func testIntroYBorrarVanEscapadosComoJson() {
        XCTAssertEqual(TextInput.enter, "\n")
        XCTAssertEqual(TextInput.backspace.unicodeScalars.count, 1)
        XCTAssertEqual(TextInput.backspace.unicodeScalars.first?.value, 0x08)
        XCTAssertEqual(TextInput.encode("Link\n"), "{\"m\":\"text\",\"text\":\"Link\\n\"}")
        XCTAssertEqual(TextInput.encode("\u{8}"), "{\"m\":\"text\",\"text\":\"\\b\"}")
        XCTAssertEqual(TextInput.encode("\n"), "{\"m\":\"text\",\"text\":\"\\n\"}")
    }

    func testEscapadoJsonDeComillasBarrasControlesYUnicode() {
        XCTAssertEqual(
            TextInput.encode("a\"b\\c/d\t\r\u{c}\u{1} ñ 日本"),
            "{\"m\":\"text\",\"text\":\"a\\\"b\\\\c\\/d\\t\\r\\f\\u0001 ñ 日本\"}"
        )
    }

    func testBorrarMandaUnBorradoYNoTocaElCampo() {
        let a = TextInput.delete("Li")
        XCTAssertEqual(a.send, TextInput.backspace)
        XCTAssertEqual(a.field, "Li")
        XCTAssertFalse(a.close)
    }

    func testEscribirMandaElCampoTalCualYLoVacia() {
        let a = TextInput.write("Link")
        XCTAssertEqual(a.send, "Link")
        XCTAssertEqual(a.field, "")
        XCTAssertFalse(a.close)
        let v = TextInput.write("")
        XCTAssertNil(v.send)
        XCTAssertEqual(v.field, "")
        XCTAssertFalse(v.close)
    }

    func testAceptarMandaElCampoMasIntroYCierra() {
        let a = TextInput.accept("Link")
        XCTAssertEqual(a.send, "Link\n")
        XCTAssertEqual(a.field, "")
        XCTAssertTrue(a.close)
        let v = TextInput.accept("")
        XCTAssertEqual(v.send, "\n")
        XCTAssertTrue(v.close)
    }

    func testCerrarNoMandaNada() {
        let a = TextInput.close("Li")
        XCTAssertNil(a.send)
        XCTAssertEqual(a.field, "Li")
        XCTAssertTrue(a.close)
    }
}

/// Ejes del móvil apaisado → marco del GamePad (contrato Wii U §4).
final class FrameTests: XCTestCase {
    private let eps: Float = 1e-5
    private let h: Float = 0.70710678

    private func assertClose(_ a: [Float], _ b: [Float], _ tol: Float = 1e-5, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(a.count, b.count, file: file, line: line)
        for (x, y) in zip(a, b) { XCTAssertEqual(x, y, accuracy: tol, "\(a) vs \(b)", file: file, line: line) }
    }

    func testTumbadoPlanoLaGravedadNoCambia() {
        let g: [Float] = [0, 0, 9.8]
        assertClose(Frame.remapAccel(g, Frame.rotation90), g)
        assertClose(Frame.remapAccel(g, Frame.rotation270), g)
        assertClose(Frame.remapGyro(g, Frame.rotation90), g)
        assertClose(Frame.remapGyro(g, Frame.rotation270), g)
    }

    func testGiroIzquierdaVectores() {
        assertClose(Frame.remapAccel([1, 0, 0], Frame.rotation90), [0, 1, 0])
        assertClose(Frame.remapAccel([0, 1, 0], Frame.rotation90), [-1, 0, 0])
        assertClose(Frame.remapGyro([1, 0, 0], Frame.rotation90), [0, 1, 0])
        assertClose(Frame.remapGyro([0, 1, 0], Frame.rotation90), [-1, 0, 0])
    }

    func testGiroDerechaVectores() {
        assertClose(Frame.remapAccel([1, 0, 0], Frame.rotation270), [0, -1, 0])
        assertClose(Frame.remapAccel([0, 1, 0], Frame.rotation270), [1, 0, 0])
        assertClose(Frame.remapGyro([1, 0, 0], Frame.rotation270), [0, -1, 0])
        assertClose(Frame.remapGyro([0, 1, 0], Frame.rotation270), [1, 0, 0])
    }

    func testFormulaCompletaDeLosVectores() {
        let v: [Float] = [1, 2, 3]
        assertClose(Frame.remapAccel(v, Frame.rotation90), [-2, 1, 3])
        assertClose(Frame.remapAccel(v, Frame.rotation270), [2, -1, 3])
    }

    func testQuatIdentidad() {
        let id: [Float] = [1, 0, 0, 0]
        assertClose(Frame.remapQuat(id, Frame.rotation90), [h, 0, 0, -h], 1e-4)
        assertClose(Frame.remapQuat(id, Frame.rotation270), [h, 0, 0, h], 1e-4)
    }

    func testQuatProductoDeHamiltonConRALaDerecha() {
        let q: [Float] = [0.5, 0.5, 0.5, 0.5]
        assertClose(Frame.remapQuat(q, Frame.rotation90), [h, 0, h, 0], 1e-4)
        assertClose(Frame.remapQuat(q, Frame.rotation270), [0, h, 0, h], 1e-4)
    }

    func testQuatConservaLaNorma() {
        let q: [Float] = [0.8, 0.1, -0.5, 0.3]
        let n = q.map { $0 * $0 }.reduce(0, +).squareRoot()
        let r = Frame.remapQuat(q, Frame.rotation90)
        XCTAssertEqual(n, r.map { $0 * $0 }.reduce(0, +).squareRoot(), accuracy: 1e-5)
    }

    func testOtrasRotacionesNoTocanNada() {
        let v: [Float] = [1, 2, 3]
        let q: [Float] = [0.8, 0.1, -0.5, 0.3]
        for rot in [Frame.rotation0, Frame.rotation180, 7, -1] {
            XCTAssertEqual(Frame.remapAccel(v, rot), v)
            XCTAssertEqual(Frame.remapGyro(v, rot), v)
            XCTAssertEqual(Frame.remapQuat(q, rot), q)
        }
    }

    func testValoresDeSurface() {
        XCTAssertEqual(Frame.rotation0, 0)
        XCTAssertEqual(Frame.rotation90, 1)
        XCTAssertEqual(Frame.rotation180, 2)
        XCTAssertEqual(Frame.rotation270, 3)
    }

    /// Orientaciones de iOS → rotaciones de Android: landscapeLeft = botón
    /// Home a la izquierda = borde superior a la derecha (270).
    func testOrientacionesDeIos() {
        XCTAssertEqual(OrientationLock.frameRotation(.landscapeLeft), Frame.rotation270)
        XCTAssertEqual(OrientationLock.frameRotation(.landscapeRight), Frame.rotation90)
        XCTAssertEqual(OrientationLock.frameRotation(.portrait), Frame.rotation0)
        XCTAssertEqual(OrientationLock.frameRotation(.portraitUpsideDown), Frame.rotation180)
    }
}

final class ReconnectTests: XCTestCase {
    func testLaEsperaCreceYSeQuedaEn15s() {
        XCTAssertEqual((1...7).map(Reconnect.delayMs), [1_000, 2_000, 4_000, 8_000, 15_000, 15_000, 15_000])
        XCTAssertEqual(Reconnect.delayMs(0), 1_000, "fuera de rango por abajo: como el primero")
    }

    func testSeRindeALosDosMinutos() {
        let dropped: Int64 = 10_000
        XCTAssertFalse(Reconnect.giveUp(dropped, dropped + 119_999))
        XCTAssertTrue(Reconnect.giveUp(dropped, dropped + Reconnect.giveUpMs))
        XCTAssertTrue(Reconnect.giveUp(dropped, dropped + 300_000))
    }
}

/// Un `bad_token` lleva al escáner; cualquier otro error, aviso y al inicio.
final class LinkFailureTests: XCTestCase {
    func testSoloElTokenYElCodigoPidenQrNuevo() {
        XCTAssertTrue(LinkFailure.needsNewQr("bad_token"))
        XCTAssertTrue(LinkFailure.needsNewQr("bad_code"))
        XCTAssertFalse(LinkFailure.needsNewQr("bad_version"))
        XCTAssertFalse(LinkFailure.needsNewQr("busy"))
        XCTAssertFalse(LinkFailure.needsNewQr("io"))
        XCTAssertFalse(LinkFailure.needsNewQr(""))
    }

    func testElNombreDelPcOElGenerico() {
        XCTAssertEqual(LinkFailure.pcLabel("SALON-PC", "Tu PC"), "SALON-PC")
        XCTAssertEqual(LinkFailure.pcLabel(" SALON-PC ", "Tu PC"), "SALON-PC")
        XCTAssertEqual(LinkFailure.pcLabel(nil, "Tu PC"), "Tu PC")
        XCTAssertEqual(LinkFailure.pcLabel("   ", "Your PC"), "Your PC")
    }

    func testConVariosPcsUnFalloDeRedOfreceElegirOtro() {
        XCTAssertTrue(LinkFailure.offerAnotherPc("io", 2))
        XCTAssertTrue(LinkFailure.offerAnotherPc("io", 3))
        XCTAssertFalse(LinkFailure.offerAnotherPc("io", 1), "con un solo PC no hay nada que elegir")
        XCTAssertFalse(LinkFailure.offerAnotherPc("busy", 2), "un error del PC no es de red")
    }
}
