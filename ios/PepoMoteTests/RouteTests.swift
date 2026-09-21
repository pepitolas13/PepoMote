import XCTest
@testable import PepoMote

/// Routing del mando: función pura de enlace + intención (mismos casos que Android).
final class RouteTests: XCTestCase {
    private func connected(
        mode: String = LinkState.modePointer,
        role: String = LinkState.roleWiimote,
        pad: String = LinkState.padGamepad,
        slot: Int = 0,
        supportsCemu: Bool = true,
        ownNunchuk: Bool = false,
        supportsSwitch: Bool = true,
        supportsGamepad: Bool = false,
        textInput: Bool = true
    ) -> UiLink {
        var c = ConnectedLink(pcName: "PC", mode: mode, rttMs: nil, sensorHz: 0, slot: slot, role: role, player: slot + 1, supportsCemu: supportsCemu, pad: pad, ownNunchuk: ownNunchuk, supportsSwitch: supportsSwitch, supportsGamepad: supportsGamepad)
        if !textInput { c.receiver = ReceiverCapabilities(ok: ["platform": "android"]) }
        return .connected(c)
    }

    /// Mismos casos que RouteTest de Android.
    func testElTecladoSeEnsenaDondeElReceptorSabeTeclear() {
        // Modo puntero: el que apunta (Jugador 1 con papel de mando)
        XCTAssertTrue(Route.showsKeyboard(connected(mode: "pointer")))
        XCTAssertFalse(Route.showsKeyboard(connected(mode: "pointer", slot: 1)))
        XCTAssertFalse(Route.showsKeyboard(connected(mode: "pointer", role: LinkState.roleNunchuk)))
        // Lo de siempre, igual que antes, y el mando universal: ahí el
        // receptor teclea en la ventana con el foco (el juego, el navegador)
        for mode in ["cemu", "retroarch", "switch", "gamepad"] {
            XCTAssertTrue(Route.showsKeyboard(connected(mode: mode)), mode)
        }
        // Dolphin no: ahí los botones van al mando emulado y no hay dónde clicar
        XCTAssertFalse(Route.showsKeyboard(connected(mode: "dolphin")))
        // Receptor que no sabe teclear (el servidor de Android)
        for mode in ["pointer", "cemu", "retroarch", "switch", "gamepad"] {
            XCTAssertFalse(Route.showsKeyboard(connected(mode: mode, textInput: false)), mode)
        }
        XCTAssertFalse(Route.showsKeyboard(.connecting))
        XCTAssertFalse(Route.showsKeyboard(.disconnected))
    }

    /// La cara del diálogo: con el teclado del PC el azul manda el texto tal
    /// cual y el Intro va aparte; con el teclado en pantalla de un emulador,
    /// «Aceptar» manda texto + Intro (es lo que lo confirma). Mismos casos
    /// que en Android (`RouteTest`).
    func testElTecladoDelPcEsElDelPunteroYElDelMandoUniversal() {
        XCTAssertTrue(Route.keyboardOnPc(connected(mode: "pointer")))
        XCTAssertTrue(Route.keyboardOnPc(connected(mode: "gamepad")))
        for mode in ["cemu", "switch", "retroarch", "dolphin"] {
            XCTAssertFalse(Route.keyboardOnPc(connected(mode: mode)), mode)
        }
        XCTAssertFalse(Route.keyboardOnPc(.connecting))
        XCTAssertFalse(Route.keyboardOnPc(.disconnected))
    }

    func testSoloSeClicaAntesDeEscribirEnModoPuntero() {
        XCTAssertTrue(Route.clicksBeforeKeyboard(connected(mode: "pointer"), true))
        // El ajuste apagado: se abre el teclado y el clic lo da el usuario con A
        XCTAssertFalse(Route.clicksBeforeKeyboard(connected(mode: "pointer"), false))
        // Fuera del puntero los botones van al mando emulado (y en la pistola
        // de RetroArch A es el clic DERECHO): nunca se clica
        for mode in ["cemu", "switch", "retroarch", "dolphin"] {
            XCTAssertFalse(Route.clicksBeforeKeyboard(connected(mode: mode), true), mode)
        }
        XCTAssertFalse(Route.clicksBeforeKeyboard(connected(mode: "pointer", slot: 2), true))
        XCTAssertFalse(Route.clicksBeforeKeyboard(connected(mode: "pointer", role: LinkState.roleNunchuk), true))
        XCTAssertFalse(Route.clicksBeforeKeyboard(.connecting, true))
    }

    func testLaPoseSoloSeCongelaSiEsteMovilMueveElCursor() {
        XCTAssertTrue(Route.holdsPointerForKeyboard(connected(mode: "pointer")))
        XCTAssertFalse(Route.holdsPointerForKeyboard(connected(mode: "pointer", slot: 1)))
        XCTAssertFalse(Route.holdsPointerForKeyboard(connected(mode: "pointer", role: LinkState.roleNunchuk)))
        for mode in ["cemu", "switch", "retroarch", "dolphin"] {
            XCTAssertFalse(Route.holdsPointerForKeyboard(connected(mode: mode)), mode)
        }
        XCTAssertFalse(Route.holdsPointerForKeyboard(.disconnected))
    }

    func testUniversalPadIsTheLandscapeGamePadWithoutTurningTheDpad() {
        let link = connected(mode: LinkState.modeGamepad, supportsGamepad: true)
        XCTAssertTrue(Route.isUniversalPad(link))
        XCTAssertEqual(Route.route(link, .none), .gamePad)
        XCTAssertTrue(Route.forcesLandscape(link, .none))
        XCTAssertTrue(Route.extendedOperative(link, .none), "manda los 80 bytes con el stick derecho")
        // No es un Mando de Wii de lado: la cruceta no se gira.
        XCTAssertFalse(Route.sidewaysDpad(link))
        XCTAssertEqual(Route.wantedMode(link, .none), LinkState.modeGamepad)
    }

    func testUniversalPadNeedsTheReceiverToAnnounceIt() {
        let old = connected(mode: LinkState.modeGamepad, supportsGamepad: false)
        XCTAssertFalse(Route.isUniversalPad(old))
        XCTAssertFalse(Route.extendedOperative(old, .none))
        let out = Route.afterModeEcho(.universalPad, LinkState.modePointer, old)
        XCTAssertEqual(out.intent, .none)
        XCTAssertEqual(out.warning, Route.warnNeedsGamepad)
    }

    func testUniversalPadIntentShowsThePadBeforeTheEcho() {
        let link = connected(mode: LinkState.modePointer, supportsGamepad: true)
        XCTAssertEqual(Route.route(link, .universalPad), .gamePad)
        XCTAssertEqual(Route.wantedMode(link, .universalPad), LinkState.modeGamepad)
        let ok = Route.afterModeEcho(.universalPad, LinkState.modeGamepad, link)
        XCTAssertEqual(ok.intent, .none)
        XCTAssertNil(ok.warning)
        // Solo el Jugador 1 cambia el modo.
        let p2 = connected(mode: LinkState.modePointer, slot: 1, supportsGamepad: true)
        XCTAssertEqual(Route.afterModeEcho(.universalPad, LinkState.modePointer, p2).warning, Route.warnPlayer1)
    }

    func testSwitchAlwaysRoutesToCompleteLandscapeProController() {
        for pad in ["pro", "joycons", "joycon_side", "joycon_r"] {
            let link = connected(mode: "switch", pad: pad)
            XCTAssertTrue(Route.isSwitch(link))
            XCTAssertEqual(Route.route(link, .none), .gamePad)
            XCTAssertTrue(Route.forcesLandscape(link, .none))
        }
        XCTAssertFalse(Route.isSwitch(connected(mode: "switch", role: "nunchuk")))
        XCTAssertEqual(Route.route(connected(mode: "switch", role: "nunchuk"), .switchMode), .nunchuk)
    }

    func testSwitchIntentIsDistinctFromWiiUAndBlocksInputsBeforeEcho() {
        XCTAssertEqual(Route.route(.connecting, .switchMode), .gamePad)
        XCTAssertEqual(Route.route(.disconnected, .switchMode), .wii)
        XCTAssertEqual(Route.wantedMode(connected(mode: "cemu"), .switchMode), "switch")
        XCTAssertEqual(Route.wantedMode(connected(mode: "switch"), .wiiU), "cemu")
        XCTAssertFalse(Route.extendedOperative(connected(mode: "cemu"), .switchMode))
        XCTAssertFalse(Route.extendedOperative(connected(mode: "switch"), .wiiU))
        XCTAssertFalse(Route.extendedOperative(.connecting, .switchMode))
        XCTAssertTrue(Route.extendedOperative(connected(mode: "switch", pad: "pro"), .none))
        XCTAssertTrue(Route.extendedOperative(connected(mode: "cemu", pad: "pro"), .none))
        XCTAssertFalse(Route.extendedOperative(connected(mode: "cemu", pad: "wiimote"), .none))
        XCTAssertEqual(Route.route(connected(mode: "switch", pad: "joycons"), .wiiU), .gamePad)
    }

    func testSwitchEchoWarningsUseItsOwnCapability() {
        XCTAssertEqual(Route.afterModeEcho(.switchMode, "switch", connected(mode: "switch")).intent, .none)
        XCTAssertNil(Route.afterModeEcho(.switchMode, "switch", connected(mode: "switch")).warning)
        XCTAssertEqual(Route.afterModeEcho(.switchMode, "pointer", connected(slot: 1, supportsSwitch: true)).warning, Route.warnPlayer1)
        XCTAssertEqual(Route.afterModeEcho(.switchMode, "pointer", connected(slot: 1, supportsCemu: true, supportsSwitch: false)).warning, Route.warnNeedsSwitch)
        XCTAssertEqual(Route.afterModeEcho(.switchMode, "pointer", .connecting).warning, Route.warnNeedsSwitch)
        XCTAssertNil(Route.afterModeEcho(.switchMode, "dolphin", connected(), byPc: true).warning)
    }

    func testMandoDeLadoGiraLaCrucetaYLosSensores() {
        // Dolphin y Wii U como Mando de Wii: mando girado (cruceta girada, sensores normalizados al IR a la izquierda)
        for link in [connected(mode: "dolphin"), connected(mode: "cemu", pad: "wiimote")] {
            XCTAssertTrue(Route.sidewaysDpad(link))
            XCTAssertEqual(Route.sidewaysRotation(link, displayRotation: Frame.rotation90), Frame.rotation0)
            XCTAssertEqual(Route.sidewaysRotation(link, displayRotation: Frame.rotation270), Frame.rotation180)
        }
        // Puntero: flechas del PC y el móvil apunta con su borde largo (como el GamePad)
        XCTAssertFalse(Route.sidewaysDpad(connected(mode: "pointer")))
        XCTAssertEqual(Route.sidewaysRotation(connected(mode: "pointer"), displayRotation: Frame.rotation90), Frame.rotation90)
        XCTAssertEqual(Route.sidewaysRotation(connected(mode: "pointer"), displayRotation: Frame.rotation270), Frame.rotation270)
        // Sin enlace confirmado: como puntero
        XCTAssertFalse(Route.sidewaysDpad(.connecting))
        XCTAssertEqual(Route.sidewaysRotation(.connecting, displayRotation: Frame.rotation90), Frame.rotation90)
    }

    func testApaisadoFijoConElGamePadYConElNunchukPropio() {
        XCTAssertTrue(Route.forcesLandscape(connected(mode: "cemu"), .none))
        XCTAssertTrue(Route.forcesLandscape(.connecting, .wiiU))
        XCTAssertTrue(Route.forcesLandscape(connected(mode: "dolphin", ownNunchuk: true), .none))
        XCTAssertFalse(Route.forcesLandscape(connected(mode: "dolphin"), .none))
        XCTAssertFalse(Route.forcesLandscape(connected(mode: "pointer", ownNunchuk: true), .none))
        XCTAssertFalse(Route.forcesLandscape(connected(mode: "dolphin", role: "nunchuk", ownNunchuk: true), .none))
        XCTAssertFalse(Route.forcesLandscape(connected(mode: "cemu", pad: "wiimote", ownNunchuk: true), .none))
        XCTAssertFalse(Route.forcesLandscape(.disconnected, .none))
    }

    func testApaisadoConNunchukSoloEnDolphinConfirmado() {
        XCTAssertTrue(Route.wiiLandscapeNunchuk(connected(mode: "dolphin", ownNunchuk: true)))
        // cualquier jugador, no solo el 1
        XCTAssertTrue(Route.wiiLandscapeNunchuk(connected(mode: "dolphin", slot: 2, ownNunchuk: true)))
        // sin confirmación del receptor (antiguo, o ajuste apagado): NES de siempre
        XCTAssertFalse(Route.wiiLandscapeNunchuk(connected(mode: "dolphin")))
        // en puntero y en Wii U no hay Nunchuk propio
        XCTAssertFalse(Route.wiiLandscapeNunchuk(connected(mode: "pointer", ownNunchuk: true)))
        XCTAssertFalse(Route.wiiLandscapeNunchuk(connected(mode: "cemu", pad: "wiimote", ownNunchuk: true)))
        // un Nunchuk (rol) nunca
        XCTAssertFalse(Route.wiiLandscapeNunchuk(connected(mode: "dolphin", role: "nunchuk", ownNunchuk: true)))
        XCTAssertFalse(Route.wiiLandscapeNunchuk(.connecting))
        // la ruta principal no cambia: sigue siendo el layout Wii
        XCTAssertEqual(Route.route(connected(mode: "dolphin", ownNunchuk: true), .none), .wii)
    }

    func testGamePadConCemuConfirmado() {
        XCTAssertEqual(Route.route(connected(mode: "cemu"), .none), .gamePad)
        XCTAssertEqual(Route.route(connected(mode: "cemu", pad: "pro", slot: 1), .none), .gamePad)
        XCTAssertEqual(Route.route(connected(mode: "cemu"), .wiiU), .gamePad)
    }

    func testMandoDeWiiDentroDeWiiUEsLayoutWii() {
        XCTAssertEqual(Route.route(connected(mode: "cemu", pad: "wiimote"), .none), .wii)
    }

    func testGamePadOptimistaConIntencionPendiente() {
        XCTAssertEqual(Route.route(.connecting, .wiiU), .gamePad)
        XCTAssertEqual(Route.route(connected(mode: "pointer"), .wiiU), .gamePad)
        XCTAssertEqual(Route.route(connected(mode: "dolphin"), .wiiU), .gamePad)
    }

    func testNunchukSiempreNunchuk() {
        XCTAssertEqual(Route.route(connected(role: "nunchuk"), .none), .nunchuk)
        XCTAssertEqual(Route.route(connected(mode: "cemu", role: "nunchuk", pad: "nunchuk"), .none), .nunchuk)
        XCTAssertEqual(Route.route(connected(role: "nunchuk"), .wiiU), .nunchuk)
    }

    func testEnCualquierOtroCasoLayoutWii() {
        XCTAssertEqual(Route.route(connected(mode: "pointer"), .none), .wii)
        XCTAssertEqual(Route.route(connected(mode: "dolphin"), .none), .wii)
        XCTAssertEqual(Route.route(.connecting, .none), .wii)
        XCTAssertEqual(Route.route(.disconnected, .none), .wii)
        XCTAssertEqual(Route.route(.disconnected, .wiiU), .wii)
        XCTAssertEqual(Route.route(.failed(code: "io", msg: "x"), .wiiU), .wii)
    }

    func testReconectandoElGamePadSeQuedaSiHayIntencion() {
        let re = UiLink.reconnecting(pcName: "PC", attempt: 2)
        XCTAssertEqual(Route.route(re, .wiiU), .gamePad)
        XCTAssertEqual(Route.route(re, .none), .wii)
    }

    func testEcoCemuConsumeLaIntencionSinAviso() {
        let out = Route.afterModeEcho(.wiiU, "cemu", connected(mode: "cemu"))
        XCTAssertEqual(out.intent, .none)
        XCTAssertNil(out.warning)
    }

    func testDifusionDelPcConsumeLaIntencionSinAviso() {
        let out = Route.afterModeEcho(.wiiU, "dolphin", connected(mode: "dolphin", slot: 1), byPc: true)
        XCTAssertEqual(out.intent, .none)
        XCTAssertNil(out.warning)
        XCTAssertEqual(Route.afterModeEcho(.none, "pointer", connected(), byPc: true).intent, .none)
    }

    func testEcoPointerConIntencionPendienteVuelveAWiiConAviso() {
        let link = connected(mode: "pointer", supportsCemu: false)
        let out = Route.afterModeEcho(.wiiU, "pointer", link)
        XCTAssertEqual(out.intent, .none)
        XCTAssertEqual(out.warning, Route.warnNeeds13)
        XCTAssertEqual(Route.route(link, out.intent), .wii)
    }

    func testEcoDeOtroModoSiendoJugador2AvisaQueSoloElJugador1CambiaElModo() {
        let link = connected(mode: "pointer", slot: 1, supportsCemu: true)
        let out = Route.afterModeEcho(.wiiU, "pointer", link)
        XCTAssertEqual(out.intent, .none)
        XCTAssertEqual(out.warning, Route.warnPlayer1)
        XCTAssertEqual(Route.route(link, out.intent), .wii)
    }

    func testEcoSinIntencionNoCambiaNada() {
        let out = Route.afterModeEcho(.none, "pointer", connected(mode: "pointer"))
        XCTAssertEqual(out.intent, .none)
        XCTAssertNil(out.warning)
        XCTAssertNil(Route.afterModeEcho(.none, "cemu", connected(mode: "cemu")).warning)
    }

    func testEcoAntesDelOkConIntencionAvisaDePcAntiguo() {
        let out = Route.afterModeEcho(.wiiU, "pointer", .connecting)
        XCTAssertEqual(out.intent, .none)
        XCTAssertEqual(out.warning, Route.warnNeeds13)
    }

    func testIsGamePad() {
        XCTAssertTrue(Route.isGamePad(connected(mode: "cemu")))
        XCTAssertTrue(Route.isGamePad(connected(mode: "cemu", pad: "pro", slot: 2)))
        XCTAssertFalse(Route.isGamePad(connected(mode: "cemu", pad: "wiimote")))
        XCTAssertFalse(Route.isGamePad(connected(mode: "cemu", role: "nunchuk", pad: "nunchuk")))
        XCTAssertFalse(Route.isGamePad(connected(mode: "dolphin")))
        XCTAssertFalse(Route.isGamePad(.connecting))
    }

    /// Los avisos existen en los dos idiomas.
    func testLosAvisosTienenTexto() {
        for code in ["es", "en"] {
            let b = L10n.bundle(for: code)
            XCTAssertNotEqual(b.localizedString(forKey: Route.warnNeeds13, value: "", table: nil), "")
            XCTAssertNotEqual(b.localizedString(forKey: Route.warnPlayer1, value: "", table: nil), "")
            XCTAssertNotEqual(b.localizedString(forKey: Route.warnNeedsSwitch, value: "", table: nil), "")
        }
    }
}
