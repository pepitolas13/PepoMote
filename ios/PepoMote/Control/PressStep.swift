import Foundation

/// Qué hacer con el dedo cuando se mueve, según los dos ajustes de pulsación.
/// Función pura calcada en las tres apps (Android `PressStep.kt`, Linux
/// `touch.rs`): la interfaz solo obedece lo que decide aquí.
enum PressStep: Equatable {
    /// El dedo sigue igual: ni suelta ni pulsa nada.
    case keep
    /// Soltar lo que tuviera y pulsar ese bit; `nil` deja el dedo libre.
    case to(UInt32?)

    /// `held`: lo que este dedo tiene pulsado ahora (`nil` = nada).
    /// `over`: el botón que le queda debajo (`nil` = ninguno).
    ///
    /// - «Pulsar deslizando»: el dedo va cogiendo lo que pisa y soltando lo
    ///   anterior (un dedo libre también pulsa al entrar en un botón).
    /// - Sin deslizar y con «Mantener al salir del botón»: nada cambia hasta
    ///   levantar el dedo.
    /// - Sin deslizar y sin mantener: salirse del botón lo suelta, y ese dedo
    ///   ya no vuelve a coger nada (como el `tryAwaitRelease` de Compose).
    static func next(slide: Bool, sticky: Bool, held: UInt32?, over: UInt32?) -> PressStep {
        if slide { return over == held ? .keep : .to(over) }
        if sticky { return .keep }
        if let held, over != held { return .to(nil) }
        return .keep
    }
}
